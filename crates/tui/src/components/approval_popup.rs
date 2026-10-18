use composer_protocol::ApprovalDecision;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::action::Action;
use crate::components::Component;
use crate::components::widgets::{centered, dim};
use crate::store::AppState;
use crate::theme;

/// Asks to run a recipe that wasn't approved (in this version). Modal: it takes
/// every key until answered. Drawn in the warn colour, never the accent.
pub struct ApprovalPopup;

impl Component for ApprovalPopup {
    fn handle_key(&mut self, key: KeyEvent, _state: &AppState) -> Vec<Action> {
        match (key.code, key.modifiers) {
            (KeyCode::Char('y'), _) => vec![Action::ApprovalDecide(ApprovalDecision::Approve)],
            (KeyCode::Char('n'), _) => vec![Action::ApprovalDecide(ApprovalDecision::Deny)],
            (KeyCode::Char('e'), _) => vec![Action::ApprovalView],
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => vec![Action::CancelTurn],
            _ => vec![],
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let Some(approval) = state.approvals.current() else {
            return;
        };
        dim(frame.buffer_mut(), area);

        let body: Vec<&str> = approval.body.lines().collect();
        let max_body = area.height.saturating_sub(12).max(3) as usize;
        let shown = body.len().min(max_body);
        let overflow = u16::from(body.len() > shown);
        // borders 2 + header, blank, blank, hash, keys 5 + body
        let rect = centered(area, 92, shown as u16 + overflow + 7);
        frame.render_widget(Clear, rect);

        let more = state.approvals.count().saturating_sub(1);
        let title = if more > 0 {
            format!(" run recipe? (+{more} more) ")
        } else {
            " run recipe? ".to_string()
        };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(Style::new().fg(theme::WARN))
            .title(Span::styled(title, Style::new().fg(theme::WARN)))
            .style(Style::new().bg(theme::SURFACE));
        let inner = block.inner(rect);
        frame.render_widget(block, rect);
        let inner = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        let width = inner.width as usize;

        let label = |text: &'static str| Span::styled(text, theme::muted());
        let value = |text: String| Span::styled(text, theme::bold());
        let mut header = vec![
            label("site "),
            value(approval.site.clone()),
            Span::raw("   "),
            label("recipe "),
            value(approval.recipe.clone()),
        ];
        if !approval.args.is_empty() {
            header.push(Span::raw("   "));
            header.push(label("args "));
            header.push(value(approval.args.join(" ")));
        }
        header.push(Span::styled(
            format!("   session #{}", approval.session),
            theme::faint(),
        ));

        let code_style = Style::new().fg(theme::TEXT).bg(theme::BG);
        let mut lines = vec![Line::from(header), Line::raw("")];
        for line in body.iter().take(shown) {
            let text: String = line.chars().take(width.saturating_sub(2)).collect();
            let pad = width.saturating_sub(text.chars().count() + 1);
            lines.push(Line::from(Span::styled(
                format!(" {text}{}", " ".repeat(pad)),
                code_style,
            )));
        }
        if body.len() > shown {
            lines.push(Line::from(Span::styled(
                format!(" … {} more lines: e to read all", body.len() - shown),
                theme::faint(),
            )));
        }
        lines.push(Line::raw(""));
        let short_hash: String = approval.hash.chars().take(8).collect();
        lines.push(Line::from(Span::styled(
            format!(
                "sha256 {short_hash} · new or changed · approval lasts until the recipe changes"
            ),
            theme::faint(),
        )));
        lines.push(Line::from(vec![
            Span::styled("y", Style::new().fg(theme::OK)),
            Span::styled(" approve   ", theme::faint()),
            Span::styled("n", Style::new().fg(theme::ERR)),
            Span::styled(" deny   ", theme::faint()),
            Span::styled("e", theme::muted()),
            Span::styled(" view in nvim   ", theme::faint()),
            Span::styled("ctrl-c", theme::muted()),
            Span::styled(" cancel the turn", theme::faint()),
        ]));
        frame.render_widget(Paragraph::new(lines), inner);
    }
}
