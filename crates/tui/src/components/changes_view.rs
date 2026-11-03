use std::cell::Cell;

use composer_protocol::http::{DiffComment, TicketReview};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::components::Component;
use crate::components::widgets::{fit, pane_block, short_path};
use crate::store::AppState;
use crate::store::types::{DiffKind, DiffLine, Mode, Pane};
use crate::theme;

/// A comment being typed: on a diff line, or an edit of a draft.
struct CommentInput {
    file: String,
    line: u32,
    quote: String,
    text: String,
    editing: Option<usize>,
}

/// Footer rows the comments may take at most.
const FOOTER_MAX: usize = 9;

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
    /// The comment being typed (Comment mode).
    input: Option<CommentInput>,
    /// The draft selected in the comments list (`C`).
    list: Option<usize>,
}

impl ChangesView {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll: Cell::new(0),
            height: Cell::new(20),
            pending_g: false,
            pending_discard: false,
            input: None,
            list: None,
        }
    }
}

impl ChangesView {
    /// What a comment on this line anchors to; `None` where there is nothing to comment on.
    fn anchor(line: &DiffLine) -> Option<(String, u32, String)> {
        let file = line.file.clone()?;
        let number = line.line?;
        let quote = match line.kind {
            DiffKind::Add | DiffKind::Context => line.text.trim().to_string(),
            DiffKind::Del => format!("-{}", line.text.trim()),
            DiffKind::Hunk => line.text.clone(),
            DiffKind::File | DiffKind::Note => return None,
        };
        Some((file, number, quote))
    }

    /// Keys while typing a comment.
    fn comment_key(&mut self, key: KeyEvent) -> Vec<Action> {
        let Some(input) = self.input.as_mut() else {
            return vec![Action::SetMode(Mode::Normal)];
        };
        match key.code {
            KeyCode::Esc => {
                self.input = None;
                vec![Action::SetMode(Mode::Normal)]
            }
            KeyCode::Enter => {
                let input = self.input.take().expect("typing");
                let message = input.text.trim().to_string();
                let mut actions = vec![Action::SetMode(Mode::Normal)];
                if !message.is_empty() {
                    actions.push(match input.editing {
                        Some(index) => Action::CommentEdit { index, message },
                        None => Action::CommentAdd(DiffComment {
                            file: input.file,
                            line: input.line,
                            message,
                            quote: Some(input.quote).filter(|q| !q.is_empty()),
                        }),
                    });
                }
                actions
            }
            KeyCode::Backspace => {
                input.text.pop();
                vec![]
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.text.push(c);
                vec![]
            }
            _ => vec![],
        }
    }

    /// Keys in the comments list: pick, edit, remove.
    fn list_key(&mut self, key: KeyEvent, state: &AppState, selected: usize) -> Vec<Action> {
        let drafts = &state.project.drafts;
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.list = Some((selected + 1).min(drafts.len().saturating_sub(1)))
            }
            KeyCode::Char('k') | KeyCode::Up => self.list = Some(selected.saturating_sub(1)),
            KeyCode::Char('x') if selected < drafts.len() => {
                self.list = (drafts.len() > 1).then(|| selected.min(drafts.len() - 2));
                return vec![Action::CommentRemove(selected)];
            }
            KeyCode::Enter if selected < drafts.len() => {
                let draft = &drafts[selected];
                self.input = Some(CommentInput {
                    file: draft.file.clone(),
                    line: draft.line,
                    quote: draft.quote.clone().unwrap_or_default(),
                    text: draft.message.clone(),
                    editing: Some(selected),
                });
                return vec![Action::SetMode(Mode::Comment)];
            }
            KeyCode::Char('S') => return vec![Action::CommentsSend],
            KeyCode::Esc | KeyCode::Char('C') | KeyCode::Char('q') => self.list = None,
            _ => {}
        }
        vec![]
    }

    /// The comments at the bottom: the one being typed, the drafts, the lines on the ticket.
    fn footer_lines(&self, state: &AppState, width: usize) -> Vec<Line<'static>> {
        let project = &state.project;
        let mut lines = Vec::new();
        if let Some(input) = &self.input {
            lines.push(Line::from(vec![
                Span::styled(
                    format!(
                        "{} {}:{} ",
                        if input.editing.is_some() {
                            "edit"
                        } else {
                            "comment on"
                        },
                        input.file,
                        input.line
                    ),
                    Style::new().fg(theme::ACCENT),
                ),
                Span::styled(input.quote.clone(), theme::faint()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("> ", Style::new().fg(theme::WARN)),
                Span::styled(format!("{}▏", input.text), theme::text()),
                Span::styled("   Enter saves · Esc cancels", theme::faint()),
            ]));
        }
        for (i, draft) in project.drafts.iter().enumerate() {
            let marker = if self.list == Some(i) { "▸" } else { " " };
            let text = format!("{marker} {}:{} — {}", draft.file, draft.line, draft.message);
            let style = if self.list == Some(i) {
                theme::bold()
            } else {
                theme::text()
            };
            lines.push(Line::from(Span::styled(fit(&text, width), style)));
        }
        for line in &project.comments {
            lines.push(Line::from(Span::styled(fit(line, width), theme::faint())));
        }
        let can_comment = project.diff.as_ref().is_some_and(|d| d.ticket.is_some());
        if can_comment && self.input.is_none() {
            let hint = if self.list.is_some() {
                "j/k pick · Enter edit · x remove · S send · Esc back"
            } else if project.drafts.is_empty() {
                "c comment on the line · v semantic/text"
            } else {
                "c comment · C edit comments · S send them · v semantic/text"
            };
            lines.push(Line::from(Span::styled(hint, theme::faint())));
        }
        let keep = lines.len().min(FOOTER_MAX);
        lines.drain(..lines.len() - keep);
        lines
    }

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
        if state.mode() == Mode::Comment {
            return self.comment_key(key);
        }
        if let Some(selected) = self.list {
            return self.list_key(key, state, selected);
        }
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
            (KeyCode::Char('c'), _) if ticket.is_some() => {
                let anchor = state.project.lines.get(self.cursor).and_then(Self::anchor);
                match anchor {
                    Some((file, line, quote)) => {
                        self.input = Some(CommentInput {
                            file,
                            line,
                            quote,
                            text: String::new(),
                            editing: None,
                        });
                        return vec![Action::SetMode(Mode::Comment)];
                    }
                    None => {
                        return vec![Action::Flash(
                            "nothing to comment on here: move to a diff line".into(),
                            crate::store::types::FlashLevel::Warn,
                        )];
                    }
                }
            }
            (KeyCode::Char('C'), _) if !state.project.drafts.is_empty() => self.list = Some(0),
            (KeyCode::Char('S'), _) if ticket.is_some() => return vec![Action::CommentsSend],
            (KeyCode::Char('v'), _) if state.project.has_semantic() => {
                return vec![Action::DiffSemantic];
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

        let footer = self.footer_lines(state, width);
        let footer_height = footer.len().min(inner.height as usize / 2);
        let body_height = (inner.height as usize)
            .saturating_sub(header_height)
            .saturating_sub(footer_height);
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
        if footer_height > 0 {
            let top = inner.height - footer_height as u16;
            let body = Rect {
                height: top,
                ..inner
            };
            let bottom = Rect {
                y: inner.y + top,
                height: footer_height as u16,
                ..inner
            };
            frame.render_widget(Paragraph::new(lines), body);
            frame.render_widget(Paragraph::new(footer), bottom);
        } else {
            frame.render_widget(Paragraph::new(lines), inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use composer_protocol::http::{ChangedFile, ProjectDiff, TicketReview};

    use super::*;
    use crate::store::types::Startup;

    fn press(view: &mut ChangesView, state: &mut AppState, keys: &str) -> Vec<Action> {
        let mut all = Vec::new();
        for key in crate::headless::parse_keys(keys).unwrap() {
            let actions = view.handle_key(key, state);
            for action in &actions {
                state.reduce(action.clone());
            }
            all.extend(actions);
        }
        all
    }

    fn state_with_ticket_diff() -> AppState {
        let mut state = AppState::new(Startup::default(), 160_000);
        state.project.loaded(ProjectDiff {
            path: "/wt".into(),
            branch: None,
            files: vec![ChangedFile {
                path: "a.rs".into(),
                status: "M".into(),
                added: 1,
                removed: 1,
                semantic: Some("a.rs --- Rust\n1    fn a() {\n   2     let x = 2;\n".into()),
            }],
            diff: "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n fn a() {\n-    let x = 1;\n+    let x = 2;\n".into(),
            ticket: Some(TicketReview {
                ticket: "0007".into(),
                branch: "ticket/0007".into(),
                target: Some("main".into()),
                ahead: 1,
                commits: vec![],
                uncommitted: false,
                conflicts: vec![],
            }),
        });
        state
    }

    #[test]
    fn comment_on_a_line_edit_it_and_send() {
        let mut state = state_with_ticket_diff();
        let mut view = ChangesView::new();
        // Semantic rows first: file header, then `fn a() {` (1), `let x = 2;` (2).
        press(&mut view, &mut state, "jj");
        let actions = press(&mut view, &mut state, "c");
        assert_eq!(actions, [Action::SetMode(Mode::Comment)]);
        assert_eq!(state.mode(), Mode::Comment);
        let actions = press(&mut view, &mut state, "why 2?<Enter>");
        assert_eq!(
            actions,
            [
                Action::SetMode(Mode::Normal),
                Action::CommentAdd(DiffComment {
                    file: "a.rs".into(),
                    line: 2,
                    message: "why 2?".into(),
                    quote: Some("let x = 2;".into()),
                })
            ]
        );
        assert_eq!(state.project.drafts.len(), 1);

        // Edit it from the list, then send.
        press(&mut view, &mut state, "C");
        let actions = press(&mut view, &mut state, "<Enter><BS><BS>ok<Enter>");
        assert!(actions.contains(&Action::CommentEdit {
            index: 0,
            message: "why ok".into()
        }));
        assert_eq!(state.project.drafts[0].message, "why ok");
        let actions = press(&mut view, &mut state, "S");
        assert_eq!(actions, [Action::CommentsSend]);

        // Esc cancels a comment being typed; x removes a draft.
        press(&mut view, &mut state, "<Esc>");
        press(&mut view, &mut state, "c");
        press(&mut view, &mut state, "nope<Esc>");
        assert_eq!(state.project.drafts.len(), 1);
        press(&mut view, &mut state, "Cx");
        assert!(state.project.drafts.is_empty());
    }

    #[test]
    fn v_switches_to_the_text_diff() {
        let mut state = state_with_ticket_diff();
        let mut view = ChangesView::new();
        assert!(state.project.lines.iter().all(|l| l.kind != DiffKind::Hunk));
        let actions = press(&mut view, &mut state, "v");
        assert_eq!(actions, [Action::DiffSemantic]);
        assert!(state.project.lines.iter().any(|l| l.kind == DiffKind::Hunk));
    }

    #[test]
    fn a_header_line_has_nothing_to_comment_on() {
        let mut state = state_with_ticket_diff();
        let mut view = ChangesView::new();
        let actions = press(&mut view, &mut state, "c");
        assert!(matches!(actions[..], [Action::Flash(..)]));
        assert_eq!(state.mode(), Mode::Normal);
    }
}
