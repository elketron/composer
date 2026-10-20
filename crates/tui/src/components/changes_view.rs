use std::cell::Cell;

use composer_protocol::http::TicketReview;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::components::Component;
use crate::components::widgets::{pane_block, short_path};
use crate::store::AppState;
use crate::store::types::{DiffKind, Pane};
use crate::theme;

/// Git changes of the active project (`gd`): changed files, then the diff with a
/// cursor; `Enter` opens the file in the editor at the cursor's line.
pub struct ChangesView {
    /// Cursor, as an index into the diff lines.
    cursor: usize,
    scroll: Cell<usize>,
    height: Cell<usize>,
    pending_g: bool,
    /// `X` was pressed once: a second one discards the ticket.
    pending_discard: bool,
}

impl ChangesView {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll: Cell::new(0),
            height: Cell::new(20),
            pending_g: false,
            pending_discard: false,
        }
    }
}

impl ChangesView {
    /// `ticket/0007 → main · 2 commits · uncommitted`, conflicts, and the keys.
    fn review_lines(&self, review: &TicketReview) -> Vec<Line<'static>> {
        let sep = || Span::styled(" · ", theme::faint());
        let target = review
            .target
            .clone()
            .unwrap_or_else(|| "(detached HEAD)".into());
        let mut first = vec![
            Span::styled(review.branch.clone(), Style::new().fg(theme::ACCENT)),
            Span::styled(" → ", theme::faint()),
            Span::styled(target.clone(), theme::bold()),
            sep(),
            Span::styled(format!("{} commits", review.commits.len()), theme::muted()),
        ];
        if review.uncommitted {
            first.push(sep());
            first.push(Span::styled(
                "uncommitted changes",
                Style::new().fg(theme::WARN),
            ));
        }
        let mut lines = vec![Line::from(first)];
        for commit in review.commits.iter().take(5) {
            lines.push(Line::from(Span::styled(
                format!("  {commit}"),
                theme::faint(),
            )));
        }
        if review.commits.len() > 5 {
            lines.push(Line::from(Span::styled(
                format!("  … {} more", review.commits.len() - 5),
                theme::faint(),
            )));
        }
        if !review.conflicts.is_empty() {
            lines.push(Line::from(Span::styled(
                format!(
                    "conflicts with {target} in {}: ask the worker to merge {target} and resolve them",
                    review.conflicts.join(", ")
                ),
                Style::new().fg(theme::ERR),
            )));
        }
        let mut keys = vec![
            Span::styled("m", Style::new().fg(theme::OK)),
            Span::styled(format!(" merge into {target}   "), theme::faint()),
            Span::styled("X X", Style::new().fg(theme::ERR)),
            Span::styled(" discard", theme::faint()),
        ];
        if self.pending_discard {
            keys = vec![Span::styled(
                "X again discards the branch and worktree; any other key keeps them",
                Style::new().fg(theme::ERR),
            )];
        }
        lines.push(Line::from(keys));
        lines.push(Line::raw(""));
        lines
    }
}

impl Component for ChangesView {
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        let count = state.project.lines.len();
        let page = self.height.get().max(2) / 2;
        let g = std::mem::take(&mut self.pending_g);
        let discard = std::mem::take(&mut self.pending_discard);
        let ticket = state
            .project
            .diff
            .as_ref()
            .and_then(|d| d.ticket.as_ref())
            .map(|t| t.ticket.clone());
        match (key.code, key.modifiers) {
            (KeyCode::Char('m'), _) if ticket.is_some() => {
                return vec![Action::TicketMerge(ticket)];
            }
            (KeyCode::Char('X'), _) if discard => {
                return ticket.map(Action::TicketDiscard).into_iter().collect();
            }
            (KeyCode::Char('X'), _) if ticket.is_some() => self.pending_discard = true,
            (KeyCode::Char('j') | KeyCode::Down, _) => self.cursor += 1,
            (KeyCode::Char('k') | KeyCode::Up, _) => self.cursor = self.cursor.saturating_sub(1),
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => self.cursor += page,
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.cursor = self.cursor.saturating_sub(page)
            }
            (KeyCode::Char('g'), _) if g => self.cursor = 0,
            (KeyCode::Char('g'), _) => self.pending_g = true,
            (KeyCode::Char('G'), _) => self.cursor = count.saturating_sub(1),
            // Next / previous file.
            (KeyCode::Char('}'), _) => {
                if let Some(i) = state
                    .project
                    .lines
                    .iter()
                    .skip(self.cursor + 1)
                    .position(|l| l.kind == DiffKind::File)
                {
                    self.cursor += i + 1;
                }
            }
            (KeyCode::Char('{'), _) => {
                if let Some(i) = state.project.lines[..self.cursor.min(count)]
                    .iter()
                    .rposition(|l| l.kind == DiffKind::File)
                {
                    self.cursor = i;
                }
            }
            (KeyCode::Enter, _) => {
                if let Some(line) = state.project.lines.get(self.cursor)
                    && let Some(file) = &line.file
                {
                    return vec![Action::OpenFileAt {
                        path: file.clone(),
                        line: line.line.unwrap_or(1),
                    }];
                }
            }
            (KeyCode::Esc | KeyCode::Char('q'), _) => return vec![Action::ToggleChanges],
            _ => {}
        }
        self.cursor = self.cursor.min(count.saturating_sub(1));
        vec![]
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let project = &state.project;
        let sep = || Span::styled(" · ", theme::faint());
        let mut title = vec![Span::raw(" "), Span::styled("changes", theme::text())];
        if let Some(diff) = &project.diff {
            title.push(sep());
            title.push(Span::styled(short_path(&diff.path), theme::muted()));
            if let Some(branch) = &diff.branch {
                title.push(sep());
                title.push(Span::styled(branch.clone(), Style::new().fg(theme::ACCENT)));
            }
            let (added, removed) = diff
                .files
                .iter()
                .fold((0, 0), |(a, r), f| (a + f.added, r + f.removed));
            title.push(sep());
            title.push(Span::styled(
                format!("{} files ", diff.files.len()),
                theme::muted(),
            ));
            title.push(Span::styled(
                format!("+{added} "),
                Style::new().fg(theme::OK),
            ));
            title.push(Span::styled(
                format!("-{removed}"),
                Style::new().fg(theme::ERR),
            ));
        }
        title.push(Span::raw(" "));
        let block = pane_block(Line::from(title), state.ui.focus == Pane::Page);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let inner = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        let width = inner.width as usize;

        let message = match (&project.diff, &project.error) {
            (_, Some(error)) => Some(Span::styled(error.clone(), Style::new().fg(theme::ERR))),
            (None, None) => Some(Span::styled("loading…", theme::faint())),
            (Some(diff), None) if diff.files.is_empty() && diff.ticket.is_some() => Some(
                Span::styled("no changes on the ticket's branch yet", theme::faint()),
            ),
            (Some(diff), None) if diff.files.is_empty() => Some(Span::styled(
                "no changes: the working tree matches HEAD",
                theme::faint(),
            )),
            _ => None,
        };
        if let Some(message) = message {
            frame.render_widget(Paragraph::new(message), inner);
            return;
        }
        let Some(diff) = &project.diff else { return };

        // A ticket's branch first: where it goes and what merging it does.
        let mut header: Vec<Line> = diff
            .ticket
            .as_ref()
            .map(|review| self.review_lines(review))
            .unwrap_or_default();
        // Changed files, then the diff (the cursor moves through the diff).
        header.extend(diff.files.iter().map(|f| {
            let color = match f.status.as_str() {
                "A" | "?" => theme::OK,
                "D" => theme::ERR,
                _ => theme::WARN,
            };
            let counts = format!("+{} -{}", f.added, f.removed);
            let path: String = f
                .path
                .chars()
                .take(width.saturating_sub(counts.len() + 4))
                .collect();
            let pad = width.saturating_sub(path.chars().count() + counts.len() + 3);
            Line::from(vec![
                Span::styled(format!("{} ", f.status), Style::new().fg(color)),
                Span::styled(path, theme::text()),
                Span::raw(" ".repeat(pad)),
                Span::styled(format!("+{}", f.added), Style::new().fg(theme::OK)),
                Span::styled(format!(" -{}", f.removed), Style::new().fg(theme::ERR)),
            ])
        }));
        header.push(Line::raw(""));
        let header_height = header.len().min(inner.height as usize / 3);

        let body_height = (inner.height as usize).saturating_sub(header_height);
        self.height.set(body_height);
        // Keep the cursor visible.
        let mut scroll = self.scroll.get();
        if self.cursor < scroll {
            scroll = self.cursor;
        } else if self.cursor >= scroll + body_height {
            scroll = self.cursor + 1 - body_height;
        }
        self.scroll.set(scroll);

        let mut lines: Vec<Line> = header.into_iter().take(header_height).collect();
        for (i, line) in project
            .lines
            .iter()
            .enumerate()
            .skip(scroll)
            .take(body_height)
        {
            let number = |n: Option<u32>| {
                Span::styled(
                    format!("{:>4} ", n.map(|n| n.to_string()).unwrap_or_default()),
                    theme::faint(),
                )
            };
            let mut spans = match line.kind {
                DiffKind::File => vec![Span::styled(format!("── {} ", line.text), theme::bold())],
                DiffKind::Hunk => vec![Span::styled(
                    line.text.clone(),
                    Style::new().fg(theme::INFO),
                )],
                DiffKind::Add => vec![
                    number(line.line),
                    Span::styled(format!("+{}", line.text), Style::new().fg(theme::OK)),
                ],
                DiffKind::Del => vec![
                    number(None),
                    Span::styled(format!("-{}", line.text), Style::new().fg(theme::ERR)),
                ],
                DiffKind::Context => vec![
                    number(line.line),
                    Span::styled(format!(" {}", line.text), theme::muted()),
                ],
                DiffKind::Note => vec![Span::styled(line.text.clone(), theme::faint())],
            };
            if i == self.cursor {
                spans.insert(0, Span::styled("▎", Style::new().fg(theme::ACCENT)));
                let used: usize = spans.iter().map(|s| s.width()).sum();
                spans.push(Span::raw(" ".repeat(width.saturating_sub(used))));
                lines.push(
                    Line::from(spans).style(
                        Style::new()
                            .bg(theme::RAISED)
                            .add_modifier(Modifier::empty()),
                    ),
                );
            } else {
                spans.insert(0, Span::raw(" "));
                lines.push(Line::from(spans));
            }
        }
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
