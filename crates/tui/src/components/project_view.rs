use std::cell::Cell;

use composer_protocol::http::ProjectOverview;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::components::Component;
use crate::components::widgets::pane_block;
use crate::store::AppState;
use crate::store::types::Pane;
use crate::theme;

/// One project's overview in the right pane (`gp`): its works across pipelines, branches waiting
/// to merge, its sessions, its memory and switches. Rows open what they show.
pub struct ProjectView {
    /// The cursor, as an index into the rows that open something.
    cursor: usize,
    scroll: Cell<usize>,
}

/// A line of the view, and what `enter` on it does.
struct Row {
    line: Line<'static>,
    open: Vec<Action>,
}

impl ProjectView {
    pub fn new() -> Self {
        Self {
            cursor: 0,
            scroll: Cell::new(0),
        }
    }

    fn rows(state: &AppState) -> Vec<Row> {
        let Some(o) = &state.project_view.overview else {
            return vec![];
        };
        let mut rows = Vec::new();
        let heading = |text: &str| Row {
            line: Line::from(Span::styled(text.to_string(), theme::faint())),
            open: vec![],
        };
        let blank = || Row {
            line: Line::raw(""),
            open: vec![],
        };
        let close = Action::ToggleProjectView;

        let mut head = vec![Span::styled(o.path.clone(), theme::muted())];
        if let Some(branch) = &o.branch {
            head.push(Span::styled(" · ", theme::faint()));
            head.push(Span::styled(branch.clone(), Style::new().fg(theme::ACCENT)));
        }
        if o.uncommitted > 0 {
            head.push(Span::styled(
                format!(" · {} uncommitted", o.uncommitted),
                Style::new().fg(theme::WARN),
            ));
        }
        rows.push(Row {
            line: Line::from(head),
            open: vec![],
        });

        if !o.works.is_empty() {
            rows.push(blank());
            rows.push(heading("WORKS"));
            for w in &o.works {
                let mut spans = vec![Span::styled(format!("{:<10} ", w.pipeline), theme::faint())];
                // One dot per stage: done, review, running, waiting.
                for stage in &w.stages {
                    spans.push(stage_dot(&stage.status));
                }
                if !w.stages.is_empty() {
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(w.title.clone(), theme::text()));
                spans.push(Span::styled(
                    format!("  {}", w.status.clone().unwrap_or_default()),
                    theme::muted(),
                ));
                rows.push(Row {
                    line: Line::from(spans),
                    open: vec![close.clone(), Action::OpenUrl(w.url.clone())],
                });
            }
        }

        if !o.branches.is_empty() {
            rows.push(blank());
            rows.push(heading("BRANCHES TO MERGE"));
            for b in &o.branches {
                // Its worker's changes view has merge and discard; without one, the ticket.
                let worker = state
                    .session
                    .sessions
                    .iter()
                    .find(|s| s.ticket.as_deref() == Some(b.ticket.as_str()));
                let open = match worker {
                    Some(w) => vec![
                        close.clone(),
                        Action::AttachSession(w.id.clone()),
                        Action::ToggleChanges,
                    ],
                    None => vec![
                        close.clone(),
                        Action::OpenUrl(format!("site://orchestration/tickets/{}", b.ticket)),
                    ],
                };
                rows.push(Row {
                    line: Line::from(vec![
                        Span::styled(format!("{} ", b.branch), Style::new().fg(theme::ACCENT)),
                        Span::styled(b.title.clone(), theme::text()),
                        Span::styled(
                            match b.commits {
                                0 => "  nothing to merge".to_string(),
                                1 => "  1 commit".to_string(),
                                n => format!("  {n} commits"),
                            },
                            theme::faint(),
                        ),
                    ]),
                    open,
                });
            }
        }

        rows.push(blank());
        rows.push(heading("SESSIONS"));
        let sessions = &state.session.sessions;
        let mine: Vec<_> = sessions
            .iter()
            .filter(|s| {
                s.ticket.is_none() && state.session.project_of(s).as_deref() == Some(&o.folder)
            })
            .collect();
        if mine.is_empty() {
            rows.push(Row {
                line: Line::from(Span::styled(
                    "no chats here yet: n starts one",
                    theme::faint(),
                )),
                open: vec![],
            });
        }
        for s in mine {
            rows.push(session_row(s, "", state));
            for w in sessions
                .iter()
                .filter(|w| w.parent.as_deref() == Some(&s.id))
            {
                rows.push(session_row(w, "  ", state));
            }
        }

        if let Some(memory) = &o.memory {
            rows.push(blank());
            rows.push(heading("MEMORY"));
            rows.push(Row {
                line: Line::from(Span::styled(memory.excerpt.clone(), theme::muted())),
                open: vec![close.clone(), Action::OpenUrl(memory.url.clone())],
            });
        }
        let switches: Vec<_> = o
            .harnesses
            .iter()
            .filter(|h| h.allowed && !h.entry.contains(':'))
            .collect();
        if !switches.is_empty() {
            rows.push(blank());
            rows.push(heading("SWITCHES"));
            for h in switches {
                rows.push(Row {
                    line: Line::from(vec![
                        Span::styled(format!("{} ", h.entry), theme::text()),
                        if h.on {
                            Span::styled("on", Style::new().fg(theme::OK))
                        } else {
                            Span::styled("off", theme::faint())
                        },
                        Span::styled(
                            if o.tool_rules > 0 {
                                format!(" · {} tool rules", o.tool_rules)
                            } else {
                                String::new()
                            },
                            theme::faint(),
                        ),
                    ]),
                    open: vec![],
                });
            }
        }
        rows
    }
}

/// A stage's state as a dot: filled when finished, coloured by what it waits on.
fn stage_dot(status: &str) -> Span<'static> {
    match status {
        "done" | "approved" => Span::styled("●", Style::new().fg(theme::OK)),
        "review" => Span::styled("●", Style::new().fg(theme::ACCENT)),
        "running" | "rerun" => Span::styled("●", Style::new().fg(theme::WARN)),
        "stopped" | "failed" => Span::styled("●", Style::new().fg(theme::ERR)),
        _ => Span::styled("○", theme::faint()),
    }
}

fn session_row(s: &composer_protocol::SessionInfo, indent: &str, state: &AppState) -> Row {
    let busy = s.state != composer_protocol::TurnState::Idle;
    let label = match (&s.ticket, &s.worker) {
        (Some(ticket), Some(worker)) => {
            format!("{ticket} {}", format!("{worker:?}").to_lowercase())
        }
        _ => {
            if busy {
                "running".to_string()
            } else {
                String::new()
            }
        }
    };
    Row {
        line: Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(
                format!("{} ", s.agent),
                Style::new().fg(if busy { theme::WARN } else { theme::INFO }),
            ),
            Span::styled(format!("#{} ", s.id), theme::faint()),
            Span::styled(s.preview.clone().unwrap_or_default(), theme::muted()),
            Span::styled(format!("  {label}"), theme::faint()),
        ]),
        open: if state.session.current.as_deref() == Some(&s.id) {
            vec![Action::ToggleProjectView, Action::FocusPane(Pane::Chat)]
        } else {
            vec![
                Action::ToggleProjectView,
                Action::AttachSession(s.id.clone()),
            ]
        },
    }
}

impl Component for ProjectView {
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        let rows = Self::rows(state);
        let openable: Vec<&Row> = rows.iter().filter(|r| !r.open.is_empty()).collect();
        let count = openable.len();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.cursor = (self.cursor + 1).min(count.saturating_sub(1));
            }
            KeyCode::Char('k') | KeyCode::Up => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Enter => {
                if let Some(row) = openable.get(self.cursor) {
                    return row.open.clone();
                }
            }
            KeyCode::Char('n') => {
                if let Some(o) = &state.project_view.overview {
                    return vec![
                        Action::ToggleProjectView,
                        Action::NewSessionIn(o.folder.clone()),
                    ];
                }
            }
            KeyCode::Char('r') => {
                return vec![Action::ProjectView(state.project_view.project.clone())];
            }
            KeyCode::Esc | KeyCode::Char('q') => return vec![Action::ToggleProjectView],
            _ => {}
        }
        vec![]
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let view = &state.project_view;
        let name = view
            .overview
            .as_ref()
            .map(|o: &ProjectOverview| o.name.clone())
            .or_else(|| view.project.clone())
            .unwrap_or_default();
        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled("project", theme::text()),
            Span::styled(" · ", theme::faint()),
            Span::styled(name, Style::new().fg(theme::ACCENT)),
            Span::raw(" "),
        ]);
        let block = pane_block(title, state.ui.focus == Pane::Page);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let inner = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        if view.overview.is_none() {
            let text = if view.loading {
                "loading…"
            } else {
                "no project"
            };
            frame.render_widget(Paragraph::new(Span::styled(text, theme::faint())), inner);
            return;
        }

        let rows = Self::rows(state);
        let cursor = self.cursor.min(
            rows.iter()
                .filter(|r| !r.open.is_empty())
                .count()
                .saturating_sub(1),
        );
        let mut seen = 0;
        let mut at = 0;
        let mut lines: Vec<Line> = Vec::new();
        for (i, row) in rows.into_iter().enumerate() {
            let mut line = row.line;
            if !row.open.is_empty() {
                let on = seen == cursor;
                seen += 1;
                line.spans.insert(
                    0,
                    Span::styled(if on { "▸ " } else { "  " }, Style::new().fg(theme::ACCENT)),
                );
                if on {
                    at = i;
                    line = line.style(Style::new().bg(theme::RAISED));
                }
            } else {
                line.spans.insert(0, Span::raw("  "));
            }
            lines.push(line);
        }
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "  j/k move · enter open · n new chat here · r reload · q close · :work new <pipeline> <name> adds a work",
            theme::faint(),
        )));
        // Keep the cursor's row in view.
        let height = inner.height as usize;
        let mut scroll = self.scroll.get();
        if at < scroll {
            scroll = at;
        } else if at >= scroll + height {
            scroll = at + 1 - height;
        }
        self.scroll.set(scroll);
        frame.render_widget(
            // One line per row, clipped at the edge (enter opens the whole thing).
            Paragraph::new(lines).scroll((scroll as u16, 0)),
            inner,
        );
    }
}
