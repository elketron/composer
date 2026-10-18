use std::cell::Cell;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::action::Action;
use crate::commands::COMMANDS;
use crate::components::Component;
use crate::components::widgets::{centered, dim};
use crate::store::AppState;
use crate::theme;

const MODES: [(&str, &str); 4] = [
    (
        "NORMAL",
        "move around and use the keys below; esc gets you back here",
    ),
    (
        "INSERT",
        "write a message: enter is a newline; esc, then enter, sends it",
    ),
    (
        "COMMAND",
        "a : command; tab completes (tab / shift-tab cycle), enter runs it, esc cancels",
    ),
    (
        "HINT",
        "links on the page get labels; type one to follow it, esc cancels",
    ),
];

/// Keys that only mean something in one place, handled by that component.
const PLACES: [(&str, &[(&str, &str)]); 6] = [
    (
        "Writing (Insert mode)",
        &[
            ("enter", "new line"),
            (
                "esc, enter",
                "send the message (queued if the agent is busy)",
            ),
            ("ctrl-u / ctrl-w", "delete to the start / the previous word"),
            ("ctrl-a / ctrl-e", "start / end of the message"),
            ("ctrl-c", "cancel the agent's turn"),
        ],
    ),
    (
        "Chat and page panes",
        &[
            ("j / k", "scroll"),
            ("ctrl-d / ctrl-u", "half a page down / up"),
            ("gg / G", "top / bottom"),
            ("enter", "chat: start writing; with a draft: send it"),
        ],
    ),
    (
        "Changes view (gd)",
        &[
            ("j / k", "move the cursor through the diff"),
            ("{ / }", "previous / next file"),
            ("enter", "open the file in $EDITOR at the cursor's line"),
            ("q / esc", "close"),
        ],
    ),
    (
        "Pickers (space p, space s, space w)",
        &[
            ("type", "filter"),
            ("ctrl-n / ctrl-p", "next / previous (or down / up)"),
            ("enter", "open"),
            ("esc", "close"),
        ],
    ),
    (
        "Recipe approval",
        &[
            ("y", "approve this version of the recipe"),
            ("n", "deny (the agent is told to ask you why)"),
            ("e", "read the whole recipe in $EDITOR"),
            ("ctrl-c", "cancel the agent's turn"),
        ],
    ),
    (
        "Help",
        &[
            ("j / k, ctrl-d / ctrl-u", "scroll"),
            ("? / q / esc", "close"),
        ],
    ),
];

/// The help overlay (`?`, `:help`): modes, every key binding, and the `:` commands.
pub struct Help {
    /// Normal-mode bindings from the keymap: (keys, description).
    bindings: Vec<(String, &'static str)>,
    scroll: Cell<usize>,
    height: Cell<usize>,
}

impl Help {
    pub fn new(bindings: Vec<(String, &'static str)>) -> Self {
        Self {
            bindings,
            scroll: Cell::new(0),
            height: Cell::new(20),
        }
    }

    fn lines(&self) -> Vec<Line<'static>> {
        let heading = |text: &str| Line::from(Span::styled(text.to_string(), theme::bold()));
        let row = |keys: &str, description: &str, key_style: Style| {
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{keys:<24}"), key_style),
                Span::styled(description.to_string(), theme::muted()),
            ])
        };
        let mut lines = vec![heading("Modes")];
        for (mode, description) in MODES {
            lines.push(row(mode, description, Style::new().fg(theme::ACCENT)));
        }
        lines.push(Line::raw(""));
        lines.push(heading("Keys (Normal mode)"));
        for (keys, description) in &self.bindings {
            lines.push(row(keys, description, theme::text()));
        }
        for (place, keys) in PLACES {
            lines.push(Line::raw(""));
            lines.push(heading(place));
            for (key, description) in keys {
                lines.push(row(key, description, theme::text()));
            }
        }
        lines.push(Line::raw(""));
        lines.push(heading("Commands"));
        for (usage, description) in COMMANDS {
            lines.push(row(usage, description, Style::new().fg(theme::INFO)));
        }
        lines
    }
}

impl Component for Help {
    fn handle_key(&mut self, key: KeyEvent, _state: &AppState) -> Vec<Action> {
        let page = self.height.get().max(2) / 2;
        let scroll = self.scroll.get();
        match (key.code, key.modifiers) {
            (KeyCode::Char('?' | 'q') | KeyCode::Esc, _) => {
                self.scroll.set(0);
                return vec![Action::ToggleHelp];
            }
            (KeyCode::Char('j') | KeyCode::Down, _) => self.scroll.set(scroll + 1),
            (KeyCode::Char('k') | KeyCode::Up, _) => self.scroll.set(scroll.saturating_sub(1)),
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => self.scroll.set(scroll + page),
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.scroll.set(scroll.saturating_sub(page))
            }
            _ => {}
        }
        vec![]
    }

    fn render(&self, frame: &mut Frame, area: Rect, _state: &AppState) {
        dim(frame.buffer_mut(), area);
        let lines = self.lines();
        let rect = centered(area, 96, area.height.saturating_sub(2));
        frame.render_widget(Clear, rect);
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::new().fg(theme::BORDER_STRONG))
            .title(Span::styled(
                " help ",
                theme::text().add_modifier(Modifier::BOLD),
            ))
            .title_bottom(
                Line::from(Span::styled(
                    " j/k scroll · ? or esc closes ",
                    theme::faint(),
                ))
                .right_aligned(),
            )
            .style(Style::new().bg(theme::SURFACE));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let inner = Rect {
            x: inner.x + 1,
            y: inner.y + 1,
            width: inner.width.saturating_sub(2),
            height: inner.height.saturating_sub(1),
        };

        let visible = inner.height as usize;
        self.height.set(visible);
        let scroll = self.scroll.get().min(lines.len().saturating_sub(visible));
        self.scroll.set(scroll);
        let shown: Vec<Line> = lines.into_iter().skip(scroll).take(visible).collect();
        frame.render_widget(Paragraph::new(shown), inner);
    }
}
