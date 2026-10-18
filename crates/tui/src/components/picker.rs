use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::action::Action;
use crate::components::Component;
use crate::components::widgets::{centered, dim};
use crate::store::AppState;
use crate::store::types::PickerKind;
use crate::theme;

/// The floating fuzzy finder.
pub struct Picker;

impl Component for Picker {
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        let query = &state.picker.query;
        match (key.code, key.modifiers) {
            (KeyCode::Esc, _) => vec![Action::ClosePicker],
            (KeyCode::Enter, _) => vec![Action::PickerAccept],
            (KeyCode::Down, _)
            | (KeyCode::Char('n' | 'j'), KeyModifiers::CONTROL)
            | (KeyCode::Tab, _) => vec![Action::PickerMove(1)],
            (KeyCode::Up, _)
            | (KeyCode::Char('p' | 'k'), KeyModifiers::CONTROL)
            | (KeyCode::BackTab, _) => vec![Action::PickerMove(-1)],
            (KeyCode::Backspace, _) => {
                let mut q = query.clone();
                q.pop();
                vec![Action::PickerQuery(q)]
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => vec![Action::PickerQuery(String::new())],
            (KeyCode::Char(c), m) if !m.contains(KeyModifiers::CONTROL) => {
                vec![Action::PickerQuery(format!("{query}{c}"))]
            }
            _ => vec![],
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let picker = &state.picker;
        dim(frame.buffer_mut(), area);
        let list_height = picker.items.len().clamp(3, 14) as u16;
        let rect = centered(area, 88, list_height + 5);
        frame.render_widget(Clear, rect);
        let title = match picker.kind {
            PickerKind::Pages => " pages ",
            PickerKind::Sites => " sites ",
            PickerKind::Sessions => " sessions ",
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::new().fg(theme::BORDER_STRONG))
            .title(Span::styled(title, theme::text()))
            .style(Style::new().bg(theme::SURFACE));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let width = inner.width as usize;

        let count = format!("{}", picker.items.len());
        let query_line = Line::from(vec![
            Span::styled(" › ", Style::new().fg(theme::ACCENT)),
            Span::styled(picker.query.clone(), theme::text()),
            Span::styled("▏", Style::new().fg(theme::ACCENT)),
            Span::raw(" ".repeat(width.saturating_sub(picker.query.width() + 5 + count.len()))),
            Span::styled(count, theme::faint()),
        ]);
        let mut lines = vec![
            query_line,
            Line::from(Span::styled(
                "─".repeat(width),
                Style::new().fg(theme::BORDER),
            )),
        ];

        let rows = inner.height.saturating_sub(4) as usize;
        let first = picker.selected.saturating_sub(rows.saturating_sub(1));
        for (i, item) in picker.items.iter().enumerate().skip(first).take(rows) {
            let selected = i == picker.selected;
            let bg = if selected {
                theme::RAISED
            } else {
                theme::SURFACE
            };
            let detail_width = width.saturating_sub(2 + 26 + 2 + 1 + 16);
            lines.push(
                Line::from(vec![
                    Span::styled(
                        if selected { "▎ " } else { "  " },
                        Style::new().fg(theme::ACCENT),
                    ),
                    Span::styled(pad(&fit(&item.title, 26), 26), theme::text()),
                    Span::raw("  "),
                    Span::styled(
                        pad(&fit(&item.detail, detail_width), detail_width),
                        theme::muted(),
                    ),
                    Span::raw(" "),
                    Span::styled(pad_left(&fit(&item.right, 16), 16), theme::faint()),
                ])
                .style(Style::new().bg(bg)),
            );
        }
        if picker.items.is_empty() {
            lines.push(Line::from(Span::styled(
                "   nothing matches",
                theme::faint(),
            )));
        }
        frame.render_widget(Paragraph::new(lines), inner);

        let footer = Line::from(vec![
            Span::styled(" enter", theme::muted()),
            Span::styled(" open   ", theme::faint()),
            Span::styled("ctrl-n/p", theme::muted()),
            Span::styled(" move   ", theme::faint()),
            Span::styled("esc", theme::muted()),
            Span::styled(" close", theme::faint()),
        ]);
        let footer_area = Rect {
            y: inner.bottom() - 1,
            height: 1,
            ..inner
        };
        frame.render_widget(
            Paragraph::new(footer).style(Style::new().bg(theme::SURFACE)),
            footer_area,
        );
    }
}

fn fit(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        if out.width() + 2 > width {
            break;
        }
        out.push(c);
    }
    out.push('…');
    out
}

fn pad(text: &str, width: usize) -> String {
    format!("{text}{}", " ".repeat(width.saturating_sub(text.width())))
}

fn pad_left(text: &str, width: usize) -> String {
    format!("{}{text}", " ".repeat(width.saturating_sub(text.width())))
}
