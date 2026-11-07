//! A terminal pane: a server terminal's bytes go into wezterm-term's model and its screen is
//! drawn with gpui text runs. Keys are encoded by wezterm-term itself (the kitty keyboard protocol
//! too, when the program asks), so the pane only maps gpui keystrokes to its keys.

use std::sync::{Arc, Mutex};

use gpui::{
    App, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, FontStyle, FontWeight,
    HighlightStyle, Hsla, KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Render, Rgba,
    ScrollDelta, ScrollEvent, StyledText, Window, canvas, div, font, prelude::*, px,
};
use tattoy_wezterm_term as wt;
use wt::{KeyModifiers, Terminal, TerminalSize, color::ColorPalette};

use crate::key_map::{Held, terminal_key};
use crate::terminal_link::{LinkWriter, TerminalLink};

/// The bundled grid font (assets/fonts).
pub const FONT: &str = "JetBrainsMono Nerd Font Mono";
const FONT_SIZE: f32 = 14.0;
const LINE_H: f32 = 18.0;

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
        5000
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
    ended: bool,
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
            ended: false,
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

    pub fn set_ended(&mut self, cx: &mut Context<Self>) {
        self.ended = true;
        self.term.advance_bytes(
            b"\r\n\x1b[2m[the terminal ended: click or press Enter for a new one here]\x1b[0m\r\n",
        );
        cx.notify();
    }

    fn resize_to(&mut self, width: Pixels, height: Pixels) {
        let cols = ((f32::from(width) - 8.0) / self.cell_w).floor().max(10.0) as usize;
        let rows = ((f32::from(height) - 8.0) / LINE_H).floor().max(4.0) as usize;
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
        // Paste: ctrl+shift+v, bracketed when the program asked for it.
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
        let _ = self.term.key_down(code, mods);
        cx.stop_propagation();
    }

    fn on_scroll(&mut self, ev: &ScrollEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let lines = match ev.delta {
            ScrollDelta::Lines(l) => l.y,
            ScrollDelta::Pixels(p) => f32::from(p.y) / LINE_H,
        };
        let button = if lines > 0.0 {
            wt::MouseButton::WheelUp(1)
        } else {
            wt::MouseButton::WheelDown(1)
        };
        let _ = self.term.mouse_event(wt::MouseEvent {
            kind: wt::MouseEventKind::Press,
            x: 0,
            y: 0,
            x_pixel_offset: 0,
            y_pixel_offset: 0,
            button,
            modifiers: KeyModifiers::NONE,
        });
        cx.notify();
    }
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
        let cursor = self.term.cursor_pos();
        let focused = self.focus.is_focused(window);
        let lines = {
            let screen = self.term.screen();
            let range = screen.phys_range(&(0..screen.physical_rows as wt::VisibleRowIndex));
            screen.lines_in_phys_range(range)
        };

        let mut rows = Vec::with_capacity(lines.len());
        for (y, line) in lines.iter().enumerate() {
            let mut text = String::new();
            let mut runs: Vec<(std::ops::Range<usize>, HighlightStyle)> = Vec::new();
            let mut col = 0usize;
            for cell in line.visible_cells() {
                // Pad gaps (cells that are blank defaults may be elided).
                while col < cell.cell_index() {
                    text.push(' ');
                    col += 1;
                }
                let start = text.len();
                let s = cell.str();
                text.push_str(if s.is_empty() { " " } else { s });
                let attrs = cell.attrs();
                let (mut fg, mut bg) = (
                    palette.resolve_fg(attrs.foreground()),
                    palette.resolve_bg(attrs.background()),
                );
                if attrs.reverse() {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let is_cursor = focused
                    && cursor.visibility == wezterm_surface::CursorVisibility::Visible
                    && cursor.y as usize == y
                    && cursor.x == col;
                if is_cursor {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let mut style = HighlightStyle {
                    color: Some(hsla(fg)),
                    ..Default::default()
                };
                if is_cursor || hsla(bg) != default_bg {
                    style.background_color = Some(hsla(bg));
                }
                if attrs.intensity() == wt::Intensity::Bold {
                    style.font_weight = Some(FontWeight::BOLD);
                }
                if attrs.italic() {
                    style.font_style = Some(FontStyle::Italic);
                }
                runs.push((start..text.len(), style));
                col += cell.width().max(1);
            }
            // Cursor past the end of the line's content.
            if focused && cursor.y as usize == y && cursor.x >= col {
                while col < cursor.x {
                    text.push(' ');
                    col += 1;
                }
                let start = text.len();
                text.push(' ');
                runs.push((
                    start..text.len(),
                    HighlightStyle {
                        background_color: Some(hsla(palette.cursor_bg)),
                        ..Default::default()
                    },
                ));
            }
            rows.push(
                div()
                    .h(px(LINE_H))
                    .whitespace_nowrap()
                    .child(StyledText::new(text).with_highlights(runs)),
            );
        }

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
            .p_1()
            .track_focus(&self.focus)
            .key_context("terminal")
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    if this.ended {
                        cx.emit(PaneEvent::Reopen);
                    }
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(Self::on_key_down))
            .on_scroll(cx.listener(Self::on_scroll))
            .children(rows)
            .child(
                canvas(
                    move |bounds, _w, cx| {
                        entity.update(cx, |pane, _| {
                            pane.resize_to(bounds.size.width, bounds.size.height)
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
}
