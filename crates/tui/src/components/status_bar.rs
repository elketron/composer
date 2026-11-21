use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::components::Component;
use crate::store::AppState;
use crate::store::types::{ConnState, FlashLevel, Mode};
use crate::theme;

const FLASH_SECONDS: u64 = 6;

/// Mode, site, context, slots on the left; flash message or follow state on the right.
pub struct StatusBar;

impl Component for StatusBar {
    fn handle_key(&mut self, _key: KeyEvent, _state: &AppState) -> Vec<Action> {
        vec![]
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let (label, color) = match state.mode() {
            Mode::Normal => (" NORMAL ", theme::ACCENT),
            Mode::Insert => (" INSERT ", theme::OK),
            Mode::Command => (" COMMAND ", theme::INFO),
            Mode::Hint => (" HINT ", theme::WARN),
        };
        let sep = || Span::styled(" │ ", theme::faint());
        let mut left = vec![
            Span::styled(
                label,
                Style::new()
                    .fg(theme::BG)
                    .bg(color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ];

        match &state.conn.state {
            ConnState::Connected => {
                let info = state.session.current_info();
                let site = info
                    .and_then(|i| i.site.as_deref())
                    .map_or_else(|| "no site".to_string(), |site| format!("site {site}"));
                left.push(Span::styled(site, theme::muted()));
                left.push(sep());
                let ctx = info.map(|i| i.context_tokens).unwrap_or(0);
                let ctx_style = if ctx > state.conn.context_limit * 9 / 10 {
                    Style::new().fg(theme::WARN)
                } else {
                    theme::muted()
                };
                left.push(Span::styled(
                    format!("ctx {}/{}", kilo(ctx), kilo(state.conn.context_limit)),
                    ctx_style,
                ));
                left.push(sep());
                let mut slots =
                    format!("slots {}/{}", state.conn.slots_used, state.conn.slots_total);
                if state.conn.slots_waiting > 0 {
                    slots.push_str(&format!(" · {} waiting", state.conn.slots_waiting));
                }
                left.push(Span::styled(slots, theme::muted()));
                // The agent's background processes: `:ps` lists them, `:stop` ends one.
                let running = state.terminals.processes.len();
                if running > 0 {
                    left.push(sep());
                    left.push(Span::styled(
                        format!("{running} running (:ps)"),
                        Style::new().fg(theme::WARN),
                    ));
                }
                if let Some(scope) = &state.session.scope {
                    left.push(sep());
                    left.push(Span::styled(
                        format!("scope {scope}"),
                        Style::new().fg(theme::ACCENT),
                    ));
                }
            }
            ConnState::Connecting => {
                left.push(Span::styled("connecting…", Style::new().fg(theme::WARN)))
            }
            ConnState::Disconnected(reason) => left.push(Span::styled(
                format!("disconnected: {reason} · retrying"),
                Style::new().fg(theme::ERR),
            )),
        }

        let right = match &state.ui.flash {
            Some(flash) if flash.at.elapsed().as_secs() < FLASH_SECONDS => {
                let color = match flash.level {
                    FlashLevel::Info => theme::TEXT,
                    FlashLevel::Warn => theme::WARN,
                    FlashLevel::Error => theme::ERR,
                };
                Span::styled(format!("{} ", flash.text), Style::new().fg(color))
            }
            _ if state.page.follow => Span::styled("? help  ● following agent ", theme::muted()),
            _ => Span::styled("? help  ○ not following ", theme::faint()),
        };

        // The right side gets what is left, cut with `…` (long URLs, errors).
        let left_width: usize = left.iter().map(|s| s.width()).sum();
        let room = (area.width as usize).saturating_sub(left_width + 2);
        let right = if right.width() > room {
            let cut: String = right.content.chars().take(room.saturating_sub(1)).collect();
            Span::styled(format!("{cut}…"), right.style)
        } else {
            right
        };
        let gap = (area.width as usize).saturating_sub(left_width + right.width());
        left.push(Span::raw(" ".repeat(gap)));
        left.push(right);
        frame.render_widget(
            Paragraph::new(Line::from(left)).style(Style::new().bg(theme::BG)),
            area,
        );
    }
}

fn kilo(n: u32) -> String {
    if n >= 1000 {
        format!("{}K", n / 1000)
    } else {
        n.to_string()
    }
}

#[cfg(test)]
mod tests {
    use composer_protocol::{SessionInfo, TurnState};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;
    use crate::store::types::Startup;

    fn bar(site: Option<&str>) -> String {
        let mut state = AppState::new(Startup::default(), 160_000);
        state.conn.state = ConnState::Connected;
        state.session.upsert(SessionInfo {
            id: "s1".into(),
            kind: composer_protocol::RunKind::Agentic,
            site: site.map(Into::into),
            folder: None,
            parent: None,
            state: TurnState::Idle,
            last_seq: 0,
            context_tokens: 0,
            model: None,
            preview: None,
            updated: String::new(),
            harness: None,
        });
        state.session.current = Some("s1".into());
        let mut terminal = Terminal::new(TestBackend::new(100, 1)).unwrap();
        terminal
            .draw(|frame| StatusBar.render(frame, frame.area(), &state))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn the_site_shows() {
        let inside = bar(Some("3d"));
        assert!(inside.contains("site 3d │ ctx"), "{inside}");
        let outside = bar(None);
        assert!(outside.contains("no site │ ctx"), "{outside}");
    }
}
