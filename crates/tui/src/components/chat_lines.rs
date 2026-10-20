//! Builds the chat as lines: message blocks with one background per speaker.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::components::widgets::band;
use crate::markdown;
use crate::store::AppState;
use crate::store::types::ChatItem;
use crate::theme;

enum Speaker {
    You,
    Agent,
    System,
}

/// All chat lines for `width` columns (including the 1-column padding inside bands).
pub fn chat_lines(state: &AppState, width: u16) -> Vec<Line<'static>> {
    let inner = width.saturating_sub(2);
    let agent_name = state
        .session
        .current_info()
        .map(|s| s.agent.clone())
        .unwrap_or_else(|| "agent".into());
    let mut blocks: Vec<(Speaker, Vec<Line<'static>>)> = Vec::new();

    let agent_block = |blocks: &mut Vec<(Speaker, Vec<Line<'static>>)>| -> usize {
        if !matches!(blocks.last(), Some((Speaker::Agent, _))) {
            blocks.push((
                Speaker::Agent,
                vec![Line::from(Span::styled(
                    agent_name.clone(),
                    Style::new().fg(theme::INFO),
                ))],
            ));
        }
        blocks.len() - 1
    };

    for item in &state.session.items {
        match item {
            ChatItem::User {
                text,
                source,
                files,
            } => {
                let mut label = vec![Span::styled("you", Style::new().fg(theme::ACCENT))];
                if let Some(source) = source {
                    label.push(Span::styled(format!(" · {source}"), theme::faint()));
                }
                let mut lines = vec![Line::from(label)];
                // Your newlines are meant literally: make them markdown hard breaks.
                let text = text.replace('\n', "  \n");
                if !text.trim().is_empty() {
                    lines.extend(markdown::render(&text, inner, None, theme::text()).lines);
                }
                for file in files {
                    lines.push(Line::from(vec![
                        Span::styled("▪ ", Style::new().fg(theme::ACCENT)),
                        Span::styled(file.clone(), theme::muted()),
                    ]));
                }
                blocks.push((Speaker::You, lines));
            }
            ChatItem::Thinking { text } => {
                let i = agent_block(&mut blocks);
                blocks[i]
                    .1
                    .extend(thinking_lines(text, state.ui.show_thinking, inner, false));
            }
            ChatItem::Text { text } => {
                let i = agent_block(&mut blocks);
                blocks[i]
                    .1
                    .extend(markdown::render(text, inner, None, theme::text()).lines);
            }
            ChatItem::Tool {
                name,
                summary,
                detail,
                nested,
                result,
                is_error,
                ..
            } => {
                let i = agent_block(&mut blocks);
                let status = match (result, is_error) {
                    (None, _) => Span::styled(" …", theme::faint()),
                    (Some(_), true) => Span::styled(" ✗", Style::new().fg(theme::ERR)),
                    (Some(_), false) => Span::raw(""),
                };
                // A sub-agent's calls sit under the call that started it.
                let (marker, bar) = if *nested {
                    ("  ↳ ", "    │ ")
                } else {
                    ("▸ ", "  │ ")
                };
                blocks[i].1.push(Line::from(vec![
                    Span::styled(marker, theme::faint()),
                    // Long names keep a space before the summary.
                    Span::styled(format!("{name:<7} "), Style::new().fg(theme::INFO)),
                    Span::styled(summary.clone(), theme::muted()),
                    status,
                ]));
                if let Some(detail) = detail.as_ref().filter(|_| state.ui.show_tool_details) {
                    for line in detail.lines().take(40) {
                        // The terminal widget drops tabs.
                        let line = line.replace('\t', "  ");
                        let style = match line.chars().next() {
                            Some('+') => Style::new().fg(theme::OK),
                            Some('-') => Style::new().fg(theme::ERR),
                            _ => theme::muted(),
                        };
                        let text: String = line
                            .chars()
                            .take(inner.saturating_sub(bar.len() as u16 + 2) as usize)
                            .collect();
                        blocks[i].1.push(Line::from(vec![
                            Span::styled(bar, theme::faint()),
                            Span::styled(text, style),
                        ]));
                    }
                }
                if let Some(result) = result
                    .as_ref()
                    .filter(|_| state.ui.show_tool_details || *is_error)
                {
                    let style = if *is_error {
                        Style::new().fg(theme::ERR)
                    } else {
                        theme::faint()
                    };
                    for line in result.lines().take(8) {
                        let text: String = line
                            .replace('\t', "  ")
                            .chars()
                            .take(inner.saturating_sub(4) as usize)
                            .collect();
                        blocks[i].1.push(Line::from(vec![
                            Span::styled(bar, theme::faint()),
                            Span::styled(text, style),
                        ]));
                    }
                }
            }
            ChatItem::Compacted { summary, messages } => {
                let marker = if state.ui.show_tool_details {
                    "▾ "
                } else {
                    "▸ "
                };
                let mut lines = vec![Line::from(vec![
                    Span::styled(marker, theme::faint()),
                    Span::styled("compacted", Style::new().fg(theme::WARN)),
                    Span::styled(
                        format!(" · {messages} messages summarized · memory saved first"),
                        theme::faint(),
                    ),
                ])];
                if state.ui.show_tool_details {
                    for line in
                        markdown::render(summary, inner.saturating_sub(2), None, theme::muted())
                            .lines
                    {
                        let mut spans = vec![Span::styled("│ ", theme::faint())];
                        spans.extend(line.spans);
                        lines.push(Line::from(spans));
                    }
                }
                blocks.push((Speaker::System, lines));
            }
            ChatItem::System { text } => {
                blocks.push((
                    Speaker::System,
                    vec![Line::from(Span::styled(
                        text.clone(),
                        Style::new().fg(theme::WARN),
                    ))],
                ));
            }
        }
    }

    if let Some(streaming) = &state.session.streaming {
        let i = agent_block(&mut blocks);
        if !streaming.thinking.is_empty() {
            blocks[i].1.extend(thinking_lines(
                &streaming.thinking,
                state.ui.show_thinking,
                inner,
                streaming.content.is_empty(),
            ));
        }
        if !streaming.content.is_empty() {
            let mut lines = markdown::render(&streaming.content, inner, None, theme::text()).lines;
            if let Some(last) = lines.last_mut() {
                last.spans
                    .push(Span::styled("▍", Style::new().fg(theme::ACCENT)));
            }
            blocks[i].1.extend(lines);
        }
    }

    let mut out = Vec::new();
    for (speaker, lines) in blocks {
        let bg = match speaker {
            Speaker::You => theme::YOU,
            Speaker::Agent => theme::SURFACE,
            Speaker::System => theme::BG,
        };
        out.extend(lines.into_iter().map(|l| band(l, width, bg)));
        out.push(Line::raw(""));
    }
    out
}

/// Collapsed: one `▸ thinking · N words` row. Expanded: dim italic text behind a bar.
fn thinking_lines(text: &str, expanded: bool, width: u16, live: bool) -> Vec<Line<'static>> {
    let words = text.split_whitespace().count();
    let marker = if expanded { "▾ " } else { "▸ " };
    let suffix = if live { " …" } else { "" };
    let mut lines = vec![Line::from(vec![
        Span::styled(marker, theme::faint()),
        Span::styled("thinking", theme::muted()),
        Span::styled(format!(" · {words} words{suffix}"), theme::faint()),
    ])];
    if expanded {
        let style = Style::new().fg(theme::FAINT).add_modifier(Modifier::ITALIC);
        for line in markdown::render(text, width.saturating_sub(2), None, style).lines {
            let mut spans = vec![Span::styled("│ ", theme::faint())];
            spans.extend(line.spans);
            lines.push(Line::from(spans));
        }
    }
    lines
}
