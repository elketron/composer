//! A small CommonMark renderer for the terminal. Headings keep their `#` markers
//! (dimmed), lists get bullets, code blocks are unwrapped, tables are aligned.

use composer_core::SiteUrl;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use crate::markdown::types::Seg;
use crate::markdown::wrap::{truncate, wrap};
use crate::markdown::{Rendered, RenderedLink};
use crate::theme;

/// Renders `markdown` to lines of at most `width` columns.
/// `base` resolves relative links; `style` is the base text style.
pub fn render(markdown: &str, width: u16, base: Option<&SiteUrl>, style: Style) -> Rendered {
    let width = width.max(10) as usize;
    let mut r = Renderer {
        width,
        base,
        base_style: style,
        ..Renderer::default()
    };
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    for event in Parser::new_ext(markdown, options) {
        r.event(event);
    }
    r.flush();
    while r
        .out
        .last()
        .is_some_and(|l| l.iter().all(|s| s.text.trim().is_empty()))
    {
        r.out.pop();
    }
    r.finish()
}

#[derive(Default)]
struct Renderer<'a> {
    width: usize,
    base: Option<&'a SiteUrl>,
    base_style: Style,
    out: Vec<Vec<Seg>>,
    current: Vec<Seg>,
    first_prefix: Vec<Seg>,
    rest_prefix: Vec<Seg>,
    styles: Vec<Style>,
    links: Vec<String>,
    link: Option<usize>,
    lists: Vec<Option<u64>>,
    quote_depth: usize,
    code: Option<String>,
    table: Option<Vec<Vec<String>>>,
    heading: bool,
}

impl Renderer<'_> {
    fn style(&self) -> Style {
        self.styles.last().copied().unwrap_or(self.base_style)
    }

    fn push_style(&mut self, f: impl FnOnce(Style) -> Style) {
        let style = f(self.style());
        self.styles.push(style);
    }

    fn text(&mut self, text: &str, style: Style) {
        if let Some(table) = &mut self.table {
            if let Some(cell) = table.last_mut().and_then(|row| row.last_mut()) {
                cell.push_str(text);
            }
            return;
        }
        self.current.push(Seg {
            text: text.to_string(),
            style,
            link: self.link,
        });
    }

    fn event(&mut self, event: Event<'_>) {
        if let Some(code) = &mut self.code {
            match event {
                Event::Text(text) => code.push_str(&text),
                Event::End(TagEnd::CodeBlock) => self.end_code(),
                _ => {}
            }
            return;
        }
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                let style = self.style();
                self.text(&text, style);
            }
            Event::Code(text) => {
                let style = self.style().fg(theme::MUTED);
                self.text(&format!("`{text}`"), style);
            }
            Event::SoftBreak => self.text(" ", self.style()),
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.flush();
                self.out.push(vec![Seg {
                    text: "─".repeat(self.width),
                    style: theme::faint(),
                    link: None,
                }]);
                self.blank();
            }
            Event::TaskListMarker(done) => {
                let (mark, style) = if done {
                    ("[x] ", Style::new().fg(theme::OK))
                } else {
                    ("[ ] ", theme::faint())
                };
                self.text(mark, style);
            }
            Event::Html(html) | Event::InlineHtml(html) => self.text(&html, theme::faint()),
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.heading = true;
                let marks = "#".repeat(level as usize);
                self.current.push(Seg {
                    text: format!("{marks} "),
                    style: theme::faint(),
                    link: None,
                });
                self.push_style(|_| theme::bold());
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote_depth += 1;
                self.push_style(|s| s.fg(theme::MUTED));
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
            }
            Tag::List(start) => {
                self.flush();
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                let depth = self.lists.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => "• ".to_string(),
                };
                let pad = " ".repeat(marker.width());
                self.first_prefix = vec![Seg {
                    text: format!("  {indent}{marker}"),
                    style: theme::muted(),
                    link: None,
                }];
                self.rest_prefix = vec![Seg {
                    text: format!("  {indent}{pad}"),
                    style: theme::muted(),
                    link: None,
                }];
            }
            Tag::Emphasis => self.push_style(|s| s.add_modifier(Modifier::ITALIC)),
            Tag::Strong => self.push_style(|s| s.add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => self.push_style(|s| s.add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { dest_url, .. } => {
                self.links.push(self.resolve(&dest_url));
                self.link = Some(self.links.len() - 1);
                self.push_style(|_| theme::link());
            }
            Tag::Table(_) => {
                self.flush();
                self.table = Some(Vec::new());
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(t) = &mut self.table {
                    t.push(Vec::new());
                }
            }
            Tag::TableCell => {
                if let Some(row) = self.table.as_mut().and_then(|t| t.last_mut()) {
                    row.push(String::new());
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                self.flush();
                if self.lists.is_empty() {
                    self.blank();
                }
            }
            TagEnd::Heading(_) => {
                self.styles.pop();
                self.heading = false;
                self.flush();
                self.blank();
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.styles.pop();
                self.quote_depth -= 1;
            }
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
                if self.lists.is_empty() {
                    self.blank();
                }
            }
            TagEnd::Item => {
                self.flush();
                self.first_prefix.clear();
                self.rest_prefix.clear();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.styles.pop();
                self.link = None;
            }
            TagEnd::Table => self.end_table(),
            _ => {}
        }
    }

    fn end_code(&mut self) {
        let code = self.code.take().unwrap_or_default();
        let style = Style::new().fg(theme::TEXT).bg(theme::RAISED);
        let inner = self.width.saturating_sub(4);
        for line in code.trim_end_matches('\n').lines() {
            let text: String = truncate(line, inner);
            let pad = inner.saturating_sub(text.width());
            self.out.push(vec![
                Seg {
                    text: "  ".into(),
                    style: self.base_style,
                    link: None,
                },
                Seg {
                    text: format!(" {text}{} ", " ".repeat(pad)),
                    style,
                    link: None,
                },
            ]);
        }
        self.blank();
    }

    fn end_table(&mut self) {
        let Some(rows) = self.table.take() else {
            return;
        };
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0; columns];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.trim().width());
            }
        }
        for (r, row) in rows.iter().enumerate() {
            let style = if r == 0 {
                theme::bold()
            } else {
                self.base_style
            };
            let mut segs = Vec::new();
            for (i, cell) in row.iter().enumerate() {
                let cell = cell.trim();
                let pad = widths[i].saturating_sub(cell.width());
                segs.push(Seg {
                    text: format!("{cell}{}  ", " ".repeat(pad)),
                    style,
                    link: None,
                });
            }
            let line: String = segs.iter().map(|s| s.text.as_str()).collect();
            self.out.push(vec![Seg {
                text: truncate(line.trim_end(), self.width),
                style,
                link: None,
            }]);
            if r == 0 {
                let rule: usize = widths
                    .iter()
                    .map(|w| w + 2)
                    .sum::<usize>()
                    .saturating_sub(2);
                self.out.push(vec![Seg {
                    text: "─".repeat(rule.min(self.width)),
                    style: theme::faint(),
                    link: None,
                }]);
            }
        }
        self.blank();
    }

    fn blank(&mut self) {
        if self.out.last().is_some_and(|l| !l.is_empty()) {
            self.out.push(Vec::new());
        }
    }

    /// Wraps the current paragraph into output lines.
    fn flush(&mut self) {
        if self.current.iter().all(|s| s.text.trim().is_empty()) {
            self.current.clear();
            return;
        }
        let quote: Vec<Seg> = (0..self.quote_depth)
            .map(|_| Seg {
                text: "│ ".into(),
                style: theme::faint(),
                link: None,
            })
            .collect();
        let first: Vec<Seg> = quote
            .iter()
            .chain(self.first_prefix.iter())
            .cloned()
            .collect();
        let rest: Vec<Seg> = quote
            .iter()
            .chain(self.rest_prefix.iter())
            .cloned()
            .collect();
        let segs = std::mem::take(&mut self.current);
        let lines = wrap(segs, self.width, &first, &rest);
        self.out.extend(lines);
        // Further paragraphs of the same list item use the continuation indent.
        self.first_prefix = self.rest_prefix.clone();
    }

    fn resolve(&self, href: &str) -> String {
        if href.contains("://") && !href.starts_with("site://") || href.starts_with("mailto:") {
            return href.to_string();
        }
        match self.base {
            Some(base) => base
                .join(href)
                .map(|u| u.to_string())
                .unwrap_or_else(|_| href.to_string()),
            None => href.to_string(),
        }
    }

    fn finish(self) -> Rendered {
        let mut rendered = Rendered::default();
        for (line_no, segs) in self.out.into_iter().enumerate() {
            let mut col = 0u16;
            let mut spans = Vec::new();
            for seg in segs {
                let w = seg.text.width() as u16;
                if let Some(id) = seg.link {
                    match rendered.links.last_mut() {
                        Some(last) if last.id == id && last.line == line_no && last.end == col => {
                            last.end += w
                        }
                        _ => rendered.links.push(RenderedLink {
                            id,
                            line: line_no,
                            start: col,
                            end: col + w,
                            url: self.links[id].clone(),
                        }),
                    }
                }
                col += w;
                spans.push(Span::styled(seg.text, seg.style));
            }
            rendered.lines.push(Line::from(spans));
        }
        rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(r: &Rendered) -> Vec<String> {
        r.lines
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect()
    }

    #[test]
    fn headings_lists_and_wrapping() {
        let md = "# Title\n\nSome words that will need wrapping here.\n\n- one\n- two\n";
        let r = render(md, 20, None, Style::new());
        assert_eq!(
            plain(&r),
            [
                "# Title",
                "",
                "Some words that will",
                "need wrapping here.",
                "",
                "  • one",
                "  • two"
            ]
        );
    }

    #[test]
    fn links_resolve_and_track_position() {
        let base = SiteUrl::parse("site://memory/index.md").unwrap();
        let r = render("See [the user](user.md) now", 40, Some(&base), Style::new());
        assert_eq!(r.links.len(), 1);
        let link = &r.links[0];
        assert_eq!((link.line, link.start, link.end), (0, 4, 12));
        assert_eq!(link.url, "site://memory/user.md");
    }

    #[test]
    fn wrapped_link_is_split_per_line() {
        let r = render("aaaa [bb cc dd](site://x/y)", 10, None, Style::new());
        assert!(r.links.len() >= 2);
        assert!(r.links.iter().all(|l| l.id == 0));
    }

    #[test]
    fn tables_align() {
        let r = render(
            "| a | bb |\n|---|---|\n| ccc | d |\n",
            40,
            None,
            Style::new(),
        );
        assert_eq!(plain(&r)[0], "a    bb");
        assert_eq!(plain(&r)[2], "ccc  d");
    }

    #[test]
    fn long_words_break() {
        let r = render("abcdefghijklmnop", 10, None, Style::new());
        assert_eq!(plain(&r), ["abcdefghij", "klmnop"]);
    }
}
