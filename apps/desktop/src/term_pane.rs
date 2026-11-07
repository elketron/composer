//! A terminal pane: a server terminal's bytes go into wezterm-term's model and its screen is
//! drawn with gpui text runs. Keys are encoded by wezterm-term itself (the kitty keyboard protocol
//! too, when the program asks), so the pane only maps gpui keystrokes to its keys.
//!
//! The mouse goes to the program when it asks for it (shift: the pane's own selection anyway);
//! otherwise dragging selects (copied to the primary selection on Linux, `ctrl+shift+c` to the
//! clipboard) and the wheel scrolls back through the pane's history. OSC 52 writes reach the
//! clipboard; reads aren't answered (a program can't read your clipboard behind your back; pastes
//! come in as bracketed pastes).

use std::sync::{Arc, Mutex};

use gpui::{
    App, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, FontStyle, FontWeight,
    HighlightStyle, Hsla, KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, Rgba, ScrollDelta, ScrollEvent, StrikethroughStyle,
    StyledText, UnderlineStyle, Window, canvas, div, font, prelude::*, px,
};
use tattoy_wezterm_term as wt;
use wezterm_surface::{CursorShape, CursorVisibility};
use wt::{KeyCode, KeyModifiers, Terminal, TerminalSize, color::ColorPalette};

use crate::grid::{Geometry, Scrollback, Selection};
use crate::key_map::{Held, terminal_key};
use crate::terminal_link::{LinkWriter, TerminalLink};

/// The bundled grid font (assets/fonts).
pub const FONT: &str = "JetBrainsMono Nerd Font Mono";
const FONT_SIZE: f32 = 14.0;
const LINE_H: f32 = 18.0;
/// The pane's padding around the grid.
const PAD: f32 = 4.0;
/// Lines kept in the pane's own history.
const HISTORY: usize = 5000;

#[derive(Debug)]
struct Config;
impl wt::TerminalConfiguration for Config {
    fn color_palette(&self) -> ColorPalette {
        ColorPalette::default()
    }
    fn enable_kitty_keyboard(&self) -> bool {
        true
    }
    fn scrollback_size(&self) -> usize {
        HISTORY
    }
}

/// OSC 52 from the program (nvim yanking) lands here; the pane moves it to the OS clipboard.
#[derive(Default)]
struct Osc52(Mutex<Option<String>>);
impl wt::Clipboard for Osc52 {
    fn set_contents(
        &self,
        _sel: wt::ClipboardSelection,
        data: Option<String>,
    ) -> anyhow::Result<()> {
        *self.0.lock().expect("osc52 lock") = data;
        Ok(())
    }
}

/// What a pane asks of its window.
pub enum PaneEvent {
    /// The terminal ended: open a fresh one where it was.
    Reopen,
}

pub struct TermPane {
    term: Terminal,
    link: TerminalLink,
    osc52: Arc<Osc52>,
    focus: FocusHandle,
    size: (usize, usize),
    cell_w: f32,
    origin: Point<Pixels>,
    ended: bool,
    scroll: Scrollback,
    selection: Option<Selection>,
    /// A drag is selecting (not one the program gets).
    selecting: bool,
    /// The button held in a drag the program gets.
    held: Option<wt::MouseButton>,
}

impl EventEmitter<PaneEvent> for TermPane {}

impl TermPane {
    pub fn new(link: TerminalLink, cx: &mut Context<Self>) -> Self {
        let size = TerminalSize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
            dpi: 96,
        };
        let mut term = Terminal::new(
            size,
            Arc::new(Config),
            "composer",
            env!("CARGO_PKG_VERSION"),
            Box::new(LinkWriter(link.clone())),
        );
        let osc52 = Arc::new(Osc52::default());
        let clip: Arc<dyn wt::Clipboard> = osc52.clone();
        term.set_clipboard(&clip);
        Self {
            term,
            link,
            osc52,
            focus: cx.focus_handle(),
            size: (0, 0),
            cell_w: 8.4,
            origin: Point::default(),
            ended: false,
            scroll: Scrollback::default(),
            selection: None,
            selecting: false,
            held: None,
        }
    }

    pub fn terminal(&self) -> &str {
        self.link.terminal()
    }

    /// The size it last laid out at (columns, rows); 0 before the first layout.
    pub fn grid(&self) -> (usize, usize) {
        self.size
    }

    /// Bytes from the terminal.
    pub fn feed(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        self.ended = false;
        self.term.advance_bytes(bytes);
        if let Some(text) = self.osc52.0.lock().expect("osc52 lock").take() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        cx.notify();
    }

    /// Old output (a terminal's scrollback when the pane opens): drawn, never answered.
    pub fn replay(&mut self, bytes: &[u8], cx: &mut Context<Self>) {
        let link = self.link.clone();
        link.muted(|| self.term.advance_bytes(bytes));
        self.ended = false;
        cx.notify();
    }

    pub fn set_ended(&mut self, cx: &mut Context<Self>) {
        self.ended = true;
        self.term.advance_bytes(
            b"\r\n\x1b[2m[the terminal ended: click or press Enter for a new one here]\x1b[0m\r\n",
        );
        cx.notify();
    }

    /// Lines of history above the live screen.
    fn history(&self) -> usize {
        let screen = self.term.screen();
        screen.scrollback_rows() - screen.physical_rows
    }

    fn geometry(&self) -> Geometry {
        Geometry {
            left: f32::from(self.origin.x) + PAD,
            top: f32::from(self.origin.y) + PAD,
            cell_w: self.cell_w,
            line_h: LINE_H,
            cols: self.size.0.max(1),
            rows: self.size.1.max(1),
        }
    }

    /// The absolute row and column under a point.
    fn cell(&self, at: Point<Pixels>) -> (usize, usize, usize) {
        let (col, row) = self.geometry().cell_at(f32::from(at.x), f32::from(at.y));
        (self.scroll.top_row(self.history()) + row, col, row)
    }

    fn resize_to(&mut self, width: Pixels, height: Pixels) {
        let cols = ((f32::from(width) - 2.0 * PAD) / self.cell_w)
            .floor()
            .max(10.0) as usize;
        let rows = ((f32::from(height) - 2.0 * PAD) / LINE_H).floor().max(4.0) as usize;
        if (cols, rows) != self.size {
            self.size = (cols, rows);
            self.term.resize(TerminalSize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
                dpi: 96,
            });
            self.link.resize(cols as u16, rows as u16);
        }
    }

    /// The program takes the mouse (unless shift is held: then the pane selects).
    fn program_mouse(&self, modifiers: &Modifiers) -> bool {
        self.term.is_mouse_grabbed() && !modifiers.shift
    }

    fn send_mouse(
        &mut self,
        kind: wt::MouseEventKind,
        button: wt::MouseButton,
        at: Point<Pixels>,
        modifiers: &Modifiers,
    ) {
        let (_, col, row) = self.cell(at);
        let _ = self.term.mouse_event(wt::MouseEvent {
            kind,
            x: col,
            y: row as i64,
            x_pixel_offset: 0,
            y_pixel_offset: 0,
            button,
            modifiers: key_modifiers(modifiers),
        });
    }

    fn on_mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if self.ended {
            cx.emit(PaneEvent::Reopen);
            return;
        }
        let button = match ev.button {
            MouseButton::Left => wt::MouseButton::Left,
            MouseButton::Middle => wt::MouseButton::Middle,
            MouseButton::Right => wt::MouseButton::Right,
            _ => return,
        };
        if self.program_mouse(&ev.modifiers) {
            self.held = Some(button);
            self.send_mouse(
                wt::MouseEventKind::Press,
                button,
                ev.position,
                &ev.modifiers,
            );
        } else if ev.button == MouseButton::Left {
            let (row, col, _) = self.cell(ev.position);
            self.selection = Some(Selection::at(row, col));
            self.selecting = true;
        } else if ev.button == MouseButton::Middle {
            paste_primary(&mut self.term, cx);
        }
        cx.notify();
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if let Some(button) = self.held {
            self.send_mouse(wt::MouseEventKind::Move, button, ev.position, &ev.modifiers);
        } else if self.selecting
            && let Some(selection) = self.selection
        {
            let (row, col, _) = self.cell(ev.position);
            self.selection = Some(selection.to(row, col));
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, ev: &MouseUpEvent, _w: &mut Window, cx: &mut Context<Self>) {
        if let Some(button) = self.held.take() {
            self.send_mouse(
                wt::MouseEventKind::Release,
                button,
                ev.position,
                &ev.modifiers,
            );
            return;
        }
        if !self.selecting {
            return;
        }
        self.selecting = false;
        match self.selection {
            Some(selection) if !selection.is_empty() => {
                write_primary(self.selected_text(selection), cx);
            }
            _ => self.selection = None,
        }
        cx.notify();
    }

    fn on_scroll(&mut self, ev: &ScrollEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let lines = match ev.delta {
            ScrollDelta::Lines(l) => l.y.round() as i64,
            ScrollDelta::Pixels(p) => (f32::from(p.y) / LINE_H).round() as i64,
        };
        if lines == 0 {
            return;
        }
        if self.program_mouse(&ev.modifiers) {
            let button = if lines > 0 {
                wt::MouseButton::WheelUp(lines.unsigned_abs() as usize)
            } else {
                wt::MouseButton::WheelDown(lines.unsigned_abs() as usize)
            };
            self.send_mouse(
                wt::MouseEventKind::Press,
                button,
                ev.position,
                &ev.modifiers,
            );
        } else if self.term.is_alt_screen_active() {
            // A full-screen program without the mouse (less, man): the wheel is arrows.
            let key = if lines > 0 {
                KeyCode::UpArrow
            } else {
                KeyCode::DownArrow
            };
            for _ in 0..lines.unsigned_abs() {
                let _ = self.term.key_down(key, KeyModifiers::NONE);
            }
        } else {
            self.scroll = self.scroll.scrolled(lines, self.history());
        }
        cx.notify();
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        if self.ended {
            if ks.key == "enter" {
                cx.emit(PaneEvent::Reopen);
            }
            cx.stop_propagation();
            return;
        }
        // Copy and paste: ctrl+shift+c / ctrl+shift+v (bracketed when the program asked for it).
        if m.control && m.shift && ks.key == "c" {
            if let Some(selection) = self.selection.filter(|s| !s.is_empty()) {
                cx.write_to_clipboard(ClipboardItem::new_string(self.selected_text(selection)));
            }
            cx.stop_propagation();
            return;
        }
        if m.control && m.shift && ks.key == "v" {
            if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                let _ = self.term.send_paste(&text);
            }
            cx.stop_propagation();
            return;
        }
        let held = Held {
            shift: m.shift,
            control: m.control,
            alt: m.alt,
            platform: m.platform,
        };
        let Some((code, mods)) = terminal_key(&ks.key, ks.key_char.as_deref(), held) else {
            return;
        };
        // Typing goes back to the live screen, and lets go of a selection (its rows may change).
        self.scroll = Scrollback::default();
        self.selection = None;
        let _ = self.term.key_down(code, mods);
        cx.stop_propagation();
        cx.notify();
    }

    /// The text of absolute rows `first..first + count`, one string per row, a char per column.
    fn row_texts(&self, first: usize, count: usize) -> Vec<String> {
        let screen = self.term.screen();
        let end = (first + count).min(screen.scrollback_rows());
        screen
            .lines_in_phys_range(first.min(end)..end)
            .iter()
            .map(|line| {
                let mut text = String::new();
                let mut col = 0;
                for cell in line.visible_cells() {
                    while col < cell.cell_index() {
                        text.push(' ');
                        col += 1;
                    }
                    text.push_str(if cell.str().is_empty() {
                        " "
                    } else {
                        cell.str()
                    });
                    col += 1;
                    // A wide character fills the columns it covers.
                    for _ in 1..cell.width() {
                        text.push(' ');
                        col += 1;
                    }
                }
                text
            })
            .collect()
    }

    fn selected_text(&self, selection: Selection) -> String {
        let ((first, _), (last, _)) = selection.ordered();
        selection.text(first, &self.row_texts(first, last - first + 1))
    }
}

fn key_modifiers(m: &Modifiers) -> KeyModifiers {
    let mut mods = KeyModifiers::NONE;
    if m.shift {
        mods |= KeyModifiers::SHIFT;
    }
    if m.control {
        mods |= KeyModifiers::CTRL;
    }
    if m.alt {
        mods |= KeyModifiers::ALT;
    }
    mods
}

/// What was selected goes to the primary selection (Linux; middle click pastes it).
fn write_primary(text: String, cx: &mut App) {
    #[cfg(target_os = "linux")]
    cx.write_to_primary(ClipboardItem::new_string(text));
    #[cfg(not(target_os = "linux"))]
    let _ = (text, cx);
}

fn paste_primary(term: &mut Terminal, cx: &mut App) {
    #[cfg(target_os = "linux")]
    if let Some(text) = cx.read_from_primary().and_then(|c| c.text()) {
        let _ = term.send_paste(&text);
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (term, cx);
}

fn hsla(c: wt::color::SrgbaTuple) -> Hsla {
    Rgba {
        r: c.0,
        g: c.1,
        b: c.2,
        a: c.3,
    }
    .into()
}

/// A cell's look: colours (reverse and hidden applied), weight, slant, underline, strike, dim.
fn cell_style(attrs: &wt::CellAttributes, palette: &ColorPalette) -> (Hsla, Hsla, HighlightStyle) {
    let (mut fg, mut bg) = (
        hsla(palette.resolve_fg(attrs.foreground())),
        hsla(palette.resolve_bg(attrs.background())),
    );
    if attrs.reverse() {
        std::mem::swap(&mut fg, &mut bg);
    }
    if attrs.invisible() {
        fg = bg;
    }
    if attrs.intensity() == wt::Intensity::Half {
        fg.a *= 0.6;
    }
    let mut style = HighlightStyle {
        color: Some(fg),
        ..Default::default()
    };
    if attrs.intensity() == wt::Intensity::Bold {
        style.font_weight = Some(FontWeight::BOLD);
    }
    if attrs.italic() {
        style.font_style = Some(FontStyle::Italic);
    }
    let underline = attrs.underline();
    if underline != wt::Underline::None {
        let color = match attrs.underline_color() {
            wt::color::ColorAttribute::Default => fg,
            other => hsla(palette.resolve_fg(other)),
        };
        style.underline = Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(color),
            wavy: underline == wt::Underline::Curly,
        });
    }
    if attrs.strikethrough() {
        style.strikethrough = Some(StrikethroughStyle {
            thickness: px(1.),
            color: Some(fg),
        });
    }
    (fg, bg, style)
}

impl Focusable for TermPane {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for TermPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let font_id = window.text_system().resolve_font(&font(FONT));
        if let Ok(adv) = window.text_system().advance(font_id, px(FONT_SIZE), 'm') {
            self.cell_w = f32::from(adv.width);
        }
        let palette = self.term.get_config().color_palette();
        let default_bg = hsla(palette.background);
        let selected_bg = hsla(palette.selection_bg);
        let focused = self.focus.is_focused(window);
        let history = self.history();
        let top = self.scroll.top_row(history);
        let live = self.scroll.lines() == 0;
        let cursor = self.term.cursor_pos();
        let selection = self.selection;
        let lines = {
            let screen = self.term.screen();
            let end = (top + screen.physical_rows).min(screen.scrollback_rows());
            screen.lines_in_phys_range(top..end)
        };

        let mut rows = Vec::with_capacity(lines.len());
        for (y, line) in lines.iter().enumerate() {
            let row = top + y;
            let mut text = String::new();
            let mut runs: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
            let mut col = 0usize;
            for cell in line.visible_cells() {
                while col < cell.cell_index() {
                    text.push(' ');
                    col += 1;
                }
                let start = text.len();
                let s = cell.str();
                text.push_str(if s.is_empty() { " " } else { s });
                let (fg, mut bg, mut style) = cell_style(cell.attrs(), &palette);
                let block_cursor = live
                    && focused
                    && cursor.visibility == CursorVisibility::Visible
                    && cursor.y as usize == y
                    && cursor.x == col
                    && block(cursor.shape);
                if block_cursor {
                    style.color = Some(bg);
                    bg = fg;
                } else if selection.is_some_and(|s| s.contains(row, col)) {
                    bg = selected_bg;
                }
                if bg != default_bg {
                    style.background_color = Some(bg);
                }
                runs.push((start..text.len(), style));
                col += cell.width().max(1);
            }
            rows.push(
                div()
                    .h(px(LINE_H))
                    .whitespace_nowrap()
                    .child(StyledText::new(text).with_highlights(runs)),
            );
        }

        // Cursors that aren't a filled block (a bar, an underline, or any when the pane isn't
        // focused) are drawn over the grid.
        let overlay = (live && cursor.visibility == CursorVisibility::Visible)
            .then(|| {
                let (x, y) = (
                    PAD + cursor.x as f32 * self.cell_w,
                    PAD + cursor.y as f32 * LINE_H,
                );
                let color = hsla(palette.cursor_bg);
                let mark = div().absolute().left(px(x)).top(px(y));
                match (focused, cursor.shape) {
                    (true, shape) if block(shape) => None,
                    (true, CursorShape::BlinkingBar | CursorShape::SteadyBar) => {
                        Some(mark.w(px(2.)).h(px(LINE_H)).bg(color))
                    }
                    (true, _) => Some(
                        mark.top(px(y + LINE_H - 2.))
                            .w(px(self.cell_w))
                            .h(px(2.))
                            .bg(color),
                    ),
                    (false, _) => Some(
                        mark.w(px(self.cell_w))
                            .h(px(LINE_H))
                            .border_1()
                            .border_color(color),
                    ),
                }
            })
            .flatten();

        let entity = cx.entity();
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(default_bg)
            .text_color(hsla(palette.foreground))
            .font_family(FONT)
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE_H))
            .p(px(PAD))
            .track_focus(&self.focus)
            .key_context("terminal")
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Middle, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::on_mouse_up))
            .on_mouse_up(MouseButton::Right, cx.listener(Self::on_mouse_up))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll(cx.listener(Self::on_scroll))
            .children(rows)
            .children(overlay)
            .child(
                canvas(
                    move |bounds, _w, cx| {
                        entity.update(cx, |pane, _| {
                            pane.origin = bounds.origin;
                            pane.resize_to(bounds.size.width, bounds.size.height)
                        });
                    },
                    |_, _, _, _| {},
                )
                // Pinned to the pane's corner: its bounds are the pane's (without top/left an
                // absolute element sits where it would flow, below the rows).
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
    }
}

/// A cursor drawn as a filled cell.
fn block(shape: CursorShape) -> bool {
    matches!(
        shape,
        CursorShape::Default | CursorShape::BlinkingBlock | CursorShape::SteadyBlock
    )
}
