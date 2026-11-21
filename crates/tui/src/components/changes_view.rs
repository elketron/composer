use std::cell::Cell;

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
}

impl ChangesView {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll: Cell::new(0),
            height: Cell::new(20),
            pending_g: false,
        }
    }
}

impl Component for ChangesView {
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        let count = state.project.lines.len();
        let page = self.height.get().max(2) / 2;
        let g = std::mem::take(&mut self.pending_g);
        match (key.code, key.modifiers) {
            (KeyCode::Char('v'), _) if state.project.has_semantic() => {
                return vec![Action::DiffSemantic];
            }
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
            (KeyCode::Esc | KeyCode::Char('q'), _) => return vec![Action::CloseTab],
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
        let block = pane_block(Line::from(title), state.ui.focus == Pane::Stage);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let inner = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        let width = inner.width as usize;

        // The diff shown is the session's project's diff. A changes tab that names another
        // project (a desk carried over from another session) has nothing of its own to show.
        if let Some(other) = state.changes_elsewhere() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    format!(
                        "nothing to show: this tab is the changes of `{other}`, and this 
  session's project is another one"
                    ),
                    theme::faint(),
                )),
                inner,
            );
            return;
        }

        let message = match (&project.diff, &project.error) {
            (_, Some(error)) => Some(Span::styled(error.clone(), Style::new().fg(theme::ERR))),
            (None, None) => Some(Span::styled("loading…", theme::faint())),
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

        // Changed files, then the diff (the cursor moves through the diff).
        let mut header: Vec<Line> = Vec::new();
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

#[cfg(test)]
mod tests {
    use composer_protocol::http::{ChangedFile, ProjectDiff};

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

    fn state_with_diff() -> AppState {
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
        });
        state
    }

    #[test]
    fn v_switches_to_the_text_diff() {
        let mut state = state_with_diff();
        let mut view = ChangesView::new();
        assert!(state.project.lines.iter().all(|l| l.kind != DiffKind::Hunk));
        let actions = press(&mut view, &mut state, "v");
        assert_eq!(actions, [Action::DiffSemantic]);
        assert!(state.project.lines.iter().any(|l| l.kind == DiffKind::Hunk));
    }
}
