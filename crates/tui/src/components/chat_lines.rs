//! Builds the chat as lines: message blocks with one background per speaker.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

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
    let agent_name = "agent".to_string();
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
                // Long names keep a space before the summary.
                let name = format!("{name:<7} ");
                // A long summary wraps under itself, or under the name when that leaves too little.
                let hang = marker.width() + name.width();
                let hang = if hang * 3 <= inner as usize {
                    hang
                } else {
                    marker.width()
                };
                blocks[i].1.extend(markdown::wrap_spans(
                    vec![
                        Span::styled(marker, theme::faint()),
                        Span::styled(name, Style::new().fg(theme::INFO)),
                        Span::styled(summary.clone(), theme::muted()),
                        status,
                    ],
                    inner as usize,
                    vec![Span::raw(" ".repeat(hang))],
                ));
                if let Some(detail) = detail.as_ref().filter(|_| state.ui.show_tool_details) {
                    for line in detail.lines().take(40) {
                        // The terminal widget drops tabs.
                        let line = line.replace('\t', "  ");
                        let style = match line.chars().next() {
                            Some('+') => Style::new().fg(theme::OK),
                            Some('-') => Style::new().fg(theme::ERR),
                            _ => theme::muted(),
                        };
                        blocks[i]
                            .1
                            .extend(barred(&line, bar, inner as usize, style));
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
                        let line = line.replace('\t', "  ");
                        blocks[i]
                            .1
                            .extend(barred(&line, bar, inner as usize, style));
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
            ChatItem::ProcessEnded {
                terminal,
                command,
                text,
            } => {
                // "[background process t1 (`cmd`) exited with code 1]": the part after the command.
                let how = text
                    .lines()
                    .next()
                    .and_then(|line| line.trim_end_matches(']').rsplit_once("`)"))
                    .map(|(_, how)| how.trim().to_string())
                    .unwrap_or_else(|| "ended".into());
                let marker = if state.ui.show_tool_details {
                    "▾ "
                } else {
                    "▸ "
                };
                let mut lines = markdown::wrap_spans(
                    vec![
                        Span::styled(marker, theme::faint()),
                        Span::styled("process ", Style::new().fg(theme::WARN)),
                        Span::styled(format!("{terminal} `{command}` "), theme::muted()),
                        Span::styled(how, theme::faint()),
                    ],
                    inner as usize,
                    vec![Span::raw("  ")],
                );
                if state.ui.show_tool_details {
                    for line in text.lines().skip(1).take(40) {
                        let line = line.replace('\t', "  ");
                        lines.extend(barred(&line, "│ ", inner as usize, theme::faint()));
                    }
                }
                blocks.push((Speaker::System, lines));
            }
            ChatItem::System1 { text } => {
                let rest = text.strip_prefix("system-1").unwrap_or(text);
                blocks.push((
                    Speaker::System,
                    markdown::wrap_spans(
                        vec![
                            Span::styled("system-1", Style::new().fg(theme::INFO)),
                            Span::styled(rest.to_string(), theme::muted()),
                        ],
                        inner as usize,
                        vec![Span::raw("  ")],
                    ),
                ));
            }
            ChatItem::System { text } => {
                let mut lines = Vec::new();
                for line in text.lines() {
                    lines.extend(markdown::wrap_spans(
                        vec![Span::styled(line.to_string(), Style::new().fg(theme::WARN))],
                        inner as usize,
                        vec![Span::raw("  ")],
                    ));
                }
                blocks.push((Speaker::System, lines));
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

/// One line of tool or process output behind `bar`, broken into rows that fit `width` (all of its
/// text kept: output is not prose, so it breaks by character, not by word).
fn barred(line: &str, bar: &'static str, width: usize, style: Style) -> Vec<Line<'static>> {
    markdown::hard_wrap(line, width.saturating_sub(bar.width()))
        .into_iter()
        .map(|row| {
            Line::from(vec![
                Span::styled(bar, theme::faint()),
                Span::styled(row, style),
            ])
        })
        .collect()
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

#[cfg(test)]
mod wrap_tests {
    use super::*;
    use crate::store::types::Startup;

    fn rows(state: &AppState, width: u16) -> Vec<String> {
        chat_lines(state, width)
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn long_lines_wrap_and_nothing_is_cut_off() {
        let mut state = AppState::new(Startup::default(), 160_000);
        let command = "clear; sed -n '80,261p' docs/rewrite-plan.md; echo ===END";
        state.session.items = vec![
            ChatItem::Tool {
                call_id: "c1".into(),
                name: "terminal_send".into(),
                summary: format!("{{\"text\":\"{command}\",\"id\":\"tfb542b41\"}}"),
                detail: None,
                nested: false,
                result: Some(
                    "site://canvas names no folder, so there is nothing to open there".into(),
                ),
                is_error: true,
            },
            ChatItem::System1 {
                text: format!(
                    "system-1 forwarded `{command}` to you (approve 0.71 below 0.90; reject 0.20, forward 0.09)"
                ),
            },
            ChatItem::System {
                text:
                    "a notice that is longer than the pane is wide, and must wrap onto the next row"
                        .into(),
            },
        ];
        let width = 40;
        let shown = rows(&state, width);
        // Compared without whitespace: a word longer than a row is broken across rows.
        let squash = |s: &str| s.split_whitespace().collect::<String>();
        let all = squash(&shown.concat());
        for part in [
            "tfb542b41",
            "nothing to open there",
            "forward 0.09)",
            "onto the next row",
        ] {
            assert!(
                all.contains(&squash(part)),
                "{part:?} was cut off: {shown:#?}"
            );
        }
        for row in &shown {
            assert!(
                row.width() <= width as usize,
                "row wider than the pane: {row:?}"
            );
        }
    }
}
