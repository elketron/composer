//! Markdown for the chat: a line-based block model with styled runs, byte ranges and resolved
//! links. Pure on purpose — nothing here knows about gpui, so it is testable with no window.
//!
//! A block's `text` is what the chat shows for it (GPUI does the wrapping); a run marks the parts
//! of it that look different, and its range is a byte range in that text — which is what gpui's
//! `TextRun::len` counts (UTF-8 bytes), so the ranges map straight through to `with_highlights`
//! and `with_font_family_overrides`. They land on character boundaries, and they are sorted and
//! non-overlapping, because every run is a whole piece of text as the parser hands it. Fenced
//! code keeps `prose`'s split, so an open fence is code so far.

use std::ops::Range;

use composer_core::{LinkTarget, SiteUrl};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::prose;

/// A block of a message, as the chat should draw it.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub kind: Kind,
    /// How deep in lists this block sits (0 outside a list).
    pub depth: usize,
    /// How many block quotes it sits inside.
    pub quote: usize,
    /// The text to show; run and link ranges are byte ranges in it.
    pub text: String,
    /// The parts that look different, in order; what they do not cover shows plain.
    pub runs: Vec<Run>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Heading(u8),
    Paragraph,
    /// A list item with its marker.
    Item(Marker),
    /// One row of a table, its columns padded so they line up; the header row is the first.
    TableRow { header: bool },
    Rule,
    Code,
}

/// A list marker, with the same glyphs as the TUI's renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    Bullet,
    Number(u64),
    Task(bool),
}

impl Marker {
    /// The marker as the chat draws it, with the space after it.
    pub fn glyph(&self) -> String {
        match self {
            Marker::Bullet => "• ".to_string(),
            Marker::Number(n) => format!("{n}. "),
            Marker::Task(true) => "[x] ".to_string(),
            Marker::Task(false) => "[ ] ".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Style {
    pub strong: bool,
    pub emphasis: bool,
    pub strike: bool,
    /// Inline code: the mono font.
    pub mono: bool,
    /// Index into the block's `links`.
    pub link: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    pub range: Range<usize>,
    pub style: Style,
}

/// A link: its ranges in the block's text (one per styled part, so a link can hold bold words)
/// and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub text: String,
    pub ranges: Vec<Range<usize>>,
    pub target: LinkTarget,
}

/// `markdown` as blocks. `base` is the page the message belongs to (the chat has none), and is
/// what relative links are resolved against.
pub fn blocks(text: &str, base: Option<&SiteUrl>) -> Vec<Block> {
    let mut out = Vec::new();
    for chunk in prose::blocks(text) {
        match chunk {
            prose::Block::Code(code) => out.push(Block {
                kind: Kind::Code,
                depth: 0,
                quote: 0,
                text: code,
                runs: Vec::new(),
                links: Vec::new(),
            }),
            prose::Block::Prose(prose) => parse(&prose, base, &mut out),
        }
    }
    out
}

/// The options the TUI's renderer uses.
fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

/// One prose chunk (no fences in it: `prose` took those out).
fn parse(chunk: &str, base: Option<&SiteUrl>, out: &mut Vec<Block>) {
    let mut doc = Doc {
        base,
        out,
        cur: None,
        styles: Vec::new(),
        lists: Vec::new(),
        quote: 0,
        table: None,
        image: None,
        alt: String::new(),
    };
    for event in Parser::new_ext(chunk, options()) {
        doc.event(event);
    }
    doc.flush();
}

struct Doc<'a> {
    base: Option<&'a SiteUrl>,
    out: &'a mut Vec<Block>,
    cur: Option<Block>,
    /// The inline styles in force, innermost last; a run takes a copy of the top.
    styles: Vec<Style>,
    /// Open lists, innermost last: the next number to use, for an ordered list.
    lists: Vec<Option<u64>>,
    quote: usize,
    /// The table being gathered: its rows, each a cell per column.
    table: Option<Vec<Vec<String>>>,
    /// Inside an image: its destination, and the alt text gathered so far.
    image: Option<String>,
    alt: String,
}

impl Doc<'_> {
    fn style(&self) -> Style {
        self.styles.last().cloned().unwrap_or_default()
    }

    fn push_style(&mut self, change: impl FnOnce(&mut Style)) {
        let mut style = self.style();
        change(&mut style);
        self.styles.push(style);
    }

    fn event(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if self.image.is_some() {
                    self.alt.push_str(&text);
                } else {
                    self.push(&text);
                }
            }
            Event::Code(text) => {
                if self.image.is_some() {
                    self.alt.push_str(&text);
                } else {
                    self.push_style(|s| s.mono = true);
                    self.push(&text);
                    self.styles.pop();
                }
            }
            Event::SoftBreak => self.push(" "),
            Event::HardBreak => self.push("\n"),
            Event::Html(html) | Event::InlineHtml(html) => self.push(html.trim_end()),
            Event::Rule => {
                self.flush();
                self.out.push(Block {
                    kind: Kind::Rule,
                    depth: 0,
                    quote: self.quote,
                    text: String::new(),
                    runs: Vec::new(),
                    links: Vec::new(),
                });
            }
            Event::TaskListMarker(done) => {
                if let Some(cur) = &mut self.cur {
                    cur.kind = Kind::Item(Marker::Task(done));
                }
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.open(Kind::Heading(level as u8));
            }
            Tag::Paragraph => {
                // Inside a list the paragraph is the item's text: the item's block keeps its
                // marker and the text goes into it.
                if self.lists.is_empty() {
                    self.flush();
                    self.open(Kind::Paragraph);
                }
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.quote += 1;
            }
            Tag::CodeBlock(_) => {
                self.flush();
                self.open(Kind::Code);
            }
            Tag::List(start) => {
                self.flush();
                self.lists.push(start);
            }
            Tag::Item => {
                self.flush();
                let marker = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let marker = Marker::Number(*n);
                        *n += 1;
                        marker
                    }
                    _ => Marker::Bullet,
                };
                self.open(Kind::Item(marker));
            }
            Tag::Emphasis => self.push_style(|s| s.emphasis = true),
            Tag::Strong => self.push_style(|s| s.strong = true),
            Tag::Strikethrough => self.push_style(|s| s.strike = true),
            Tag::Link { dest_url, .. } => {
                let target = composer_core::resolve_link(&dest_url, self.base);
                let id = self.link(&target);
                self.push_style(|s| s.link = Some(id));
            }
            // Images stay text for now: the alt and where it points (the phone draws them).
            Tag::Image { dest_url, .. } => {
                self.image = Some(dest_url.to_string());
                self.alt.clear();
            }
            Tag::Table(_) => {
                self.flush();
                self.table = Some(Vec::new());
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(table) = &mut self.table {
                    table.push(Vec::new());
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
            TagEnd::Heading(_) | TagEnd::Paragraph | TagEnd::Item => self.flush(),
            TagEnd::BlockQuote(_) => {
                self.flush();
                self.quote -= 1;
            }
            TagEnd::CodeBlock => self.flush(),
            TagEnd::List(_) => {
                self.flush();
                self.lists.pop();
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.styles.pop();
            }
            TagEnd::Image => {
                if let Some(dest) = self.image.take() {
                    let alt = std::mem::take(&mut self.alt);
                    let target = composer_core::resolve_link(&dest, self.base);
                    let shown = shown_target(&target);
                    let shown = if alt.is_empty() {
                        shown
                    } else {
                        format!("{alt} ({shown})")
                    };
                    let id = self.link(&target);
                    self.push_style(|s| s.link = Some(id));
                    self.push(&shown);
                    self.styles.pop();
                }
            }
            TagEnd::Table => self.end_table(),
            _ => {}
        }
    }

    /// The table is finished: every row on one line, its cells padded to their column's width.
    fn end_table(&mut self) {
        let Some(rows) = self.table.take() else { return };
        if rows.is_empty() {
            return;
        }
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0usize; columns];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.trim().chars().count());
            }
        }
        for (n, row) in rows.iter().enumerate() {
            let header = n == 0;
            let mut line = String::new();
            for (i, cell) in row.iter().enumerate() {
                let cell = cell.trim();
                line.push_str(cell);
                if i + 1 < row.len() {
                    let pad = widths[i].saturating_sub(cell.chars().count());
                    line.push_str(&" ".repeat(pad + 2));
                }
            }
            let text = line.trim_end().to_string();
            let runs = if header && !text.is_empty() {
                vec![Run {
                    range: 0..text.len(),
                    style: Style {
                        strong: true,
                        ..Style::default()
                    },
                }]
            } else {
                Vec::new()
            };
            self.out.push(Block {
                kind: Kind::TableRow { header },
                depth: 0,
                quote: self.quote,
                text,
                runs,
                links: Vec::new(),
            });
        }
    }

    /// Start a block at the current list depth and quote depth.
    fn open(&mut self, kind: Kind) {
        self.cur = Some(Block {
            kind,
            depth: self.lists.len().saturating_sub(1),
            quote: self.quote,
            text: String::new(),
            runs: Vec::new(),
            links: Vec::new(),
        });
    }

    /// Register a link for the block being built and return its id.
    fn link(&mut self, target: &LinkTarget) -> usize {
        let cur = self.cur.as_mut().expect("a block to write into");
        cur.links.push(Link {
            text: String::new(),
            ranges: Vec::new(),
            target: target.clone(),
        });
        cur.links.len() - 1
    }

    /// Add text where it belongs: into a table cell, or into the block being built (where it
    /// becomes a run only if it looks different from the block's plain text).
    fn push(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(cell) = self
            .table
            .as_mut()
            .and_then(|t| t.last_mut())
            .and_then(|row| row.last_mut())
        {
            cell.push_str(text);
            return;
        }
        let style = self.style();
        if self.cur.is_none() {
            self.open(Kind::Paragraph);
        }
        let cur = self.cur.as_mut().expect("a block to write into");
        let start = cur.text.len();
        cur.text.push_str(text);
        if style != Style::default() {
            cur.runs.push(Run {
                range: start..cur.text.len(),
                style,
            });
        }
    }    /// Finish the block being built: its links hold the ranges that pointed at them.
    fn flush(&mut self) {
        let Some(mut block) = self.cur.take() else { return };
        if block.text.is_empty() && block.links.is_empty() {
            return;
        }
        for run in &block.runs {
            if let Some(id) = run.style.link {
                block.links[id].ranges.push(run.range.clone());
            }
        }
        for n in 0..block.links.len() {
            let ranges = block.links[n].ranges.clone();
            let text = text_of(&block, &ranges);
            block.links[n].text = text;
        }
        self.out.push(block);
    }
}

/// The text of a link, from its ranges (they may not be next to each other).
fn text_of(block: &Block, ranges: &[Range<usize>]) -> String {
    ranges
        .iter()
        .map(|range| &block.text[range.clone()])
        .collect::<Vec<_>>()
        .join("")
}

/// A link target as words, for the alt of an image (which is text here, not a picture).
fn shown_target(target: &LinkTarget) -> String {
    match target {
        LinkTarget::Site(url) => url.to_string(),
        LinkTarget::External(url) => url.clone(),
        LinkTarget::Invalid(href) => href.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> SiteUrl {
        SiteUrl::parse("site://memory/index.md").unwrap()
    }

    /// A block as one line: what it is, then its text with each styled part marked.
    fn shown(block: &Block) -> String {
        let mut out = String::from(&"> ".repeat(block.quote));
        out.push_str(&match block.kind {
            Kind::Heading(level) => format!("h{level} "),
            Kind::Paragraph => "p ".to_string(),
            Kind::Item(marker) => format!("{}{} ", "  ".repeat(block.depth), tag(marker)),
            Kind::TableRow { header: true } => "thead ".to_string(),
            Kind::TableRow { header: false } => "trow ".to_string(),
            Kind::Rule => "rule".to_string(),
            Kind::Code => "code ".to_string(),
        });
        if block.runs.is_empty() {
            return format!("{}{}", out, block.text).trim_end().to_string();
        }
        let mut seen = 0;
        for run in &block.runs {
            if run.range.start > seen {
                out.push_str(&block.text[seen..run.range.start]);
            }
            out.push_str(&mark(&block.text[run.range.clone()], &run.style));
            seen = run.range.end;
        }
        out.push_str(&block.text[seen..]);
        out.trim_end().to_string()
    }

    fn tag(marker: Marker) -> String {
        match marker {
            Marker::Bullet => "-".to_string(),
            Marker::Number(n) => format!("{n}."),
            Marker::Task(true) => "x".to_string(),
            Marker::Task(false) => "o".to_string(),
        }
    }

    fn mark(text: &str, style: &Style) -> String {
        let mut tags = String::new();
        for (on, name) in [
            (style.strong, "b"),
            (style.emphasis, "i"),
            (style.strike, "s"),
            (style.mono, "m"),
            (style.link.is_some(), "a"),
        ] {
            if on {
                tags.push_str(name);
            }
        }
        format!("[{tags} {text}]")
    }

    fn shown_all(md: &str) -> Vec<String> {
        blocks(md, Some(&base())).iter().map(shown).collect()
    }

    /// The one run in this block with this style, as text.
    fn text_of_style(block: &Block, style: impl Fn(&Style) -> bool) -> String {
        let run = block
            .runs
            .iter()
            .find(|r| style(&r.style))
            .expect("a run with this style");
        block.text[run.range.clone()].to_string()
    }

    #[test]
    fn headings_lose_their_markers() {
        assert_eq!(
            shown_all("# One\n\n## Two\n\n### Three *em*\n\n#### Four\n\n##### Five\n\n###### Six\n"),
            [
                "h1 One",
                "h2 Two",
                "h3 Three [i em]",
                "h4 Four",
                "h5 Five",
                "h6 Six",
            ]
        );
        assert_eq!(blocks("# One", Some(&base()))[0].kind, Kind::Heading(1));
    }

    #[test]
    fn bullet_lists_get_markers_and_depths() {
        assert_eq!(
            shown_all("- one\n- two\n    - deep\n        - deeper\n"),
            ["- one", "- two", "  - deep", "    - deeper"]
        );
    }

    #[test]
    fn ordered_lists_count_from_their_number() {
        assert_eq!(shown_all("3. three\n4. four\n"), ["3. three", "4. four"]);
        assert_eq!(shown_all("9. nine\n"), ["9. nine"]);
        // A list of its own inside a list item counts from its own number (a sublist needs a
        // blank line before it, as CommonMark asks).
        assert_eq!(shown_all("- one\n\n  2. two\n"), ["- one", "  2. two"]);
    }

    #[test]
    fn task_lists_keep_their_boxes() {
        assert_eq!(
            shown_all("- [x] done\n- [ ] todo\n    - [x] nested\n"),
            ["x done", "o todo", "  x nested"]
        );
    }

    #[test]
    fn quotes_carry_their_depth() {
        assert_eq!(
            shown_all("> quoted\n>\n> > deeper\n"),
            ["> p quoted", "> > p deeper"]
        );
    }

    #[test]
    fn tables_align_their_columns() {
        assert_eq!(
            shown_all("| a | bb |\n|---|---|\n| ccc | d |\n"),
            ["thead [b a    bb]", "trow ccc  d"]
        );
    }

    #[test]
    fn emphasis_strong_and_strike_are_runs() {
        assert_eq!(
            shown_all("*it* **bold** ~~gone~~ plain\n"),
            ["p [i it] [b bold] [s gone] plain"]
        );
        // A style inside another: the inner run carries both.
        assert_eq!(
            shown_all("**bold *both* inside**\n"),
            ["p [b bold ][bi both][b  inside]"]
        );
    }

    #[test]
    fn inline_code_ranges_are_byte_ranges_in_the_text() {
        assert_eq!(shown_all("use `foo` here"), ["p use [m foo] here"]);
        let block = &blocks("use `foo` here", Some(&base()))[0];
        assert_eq!(block.text, "use foo here");
        let mono = text_of_style(block, |s| s.mono);
        assert_eq!(mono, "foo");
        let run = block.runs.iter().find(|r| r.style.mono).unwrap();
        assert_eq!(run.range, 4..7, "byte ranges, as gpui counts them");
    }

    #[test]
    fn a_run_that_spans_a_multibyte_character_lands_on_a_boundary() {
        // "héllo" is 6 bytes for 5 characters: a range that counted characters would cut the é
        // in half, and slicing this text would panic.
        let block = &blocks("- **héllo** wörld [link](x.md)", Some(&base()))[0];
        for run in &block.runs {
            assert!(
                block.text.is_char_boundary(run.range.start)
                    && block.text.is_char_boundary(run.range.end),
                "run {:?} is not on character boundaries",
                run.range
            );
        }
        assert_eq!(text_of_style(block, |s| s.strong), "héllo");
        assert_eq!(text_of_style(block, |s| s.link == Some(0)), "link");
        let strong = &block.runs[0];
        assert_eq!(strong.range.len(), 6, "byte ranges, not character counts");
        let link = &block.runs[1];
        assert_eq!(block.links[0].ranges, vec![link.range.clone()]);
    }

    #[test]
    fn link_ranges_are_what_the_renderer_clicks() {
        let block = &blocks(
            "See [the user](user.md) and [`code docs`](https://example.com) now",
            Some(&base()),
        )[0];
        assert_eq!(block.links.len(), 2);
        assert_eq!(block.links[0].text, "the user");
        assert_eq!(
            block.links[0].target,
            LinkTarget::Site(SiteUrl::parse("site://memory/user.md").unwrap())
        );
        // A link whose words are styled: still one link, the ranges of both its parts.
        assert_eq!(block.links[1].text, "code docs");
        assert_eq!(
            block.links[1].target,
            LinkTarget::External("https://example.com".to_string())
        );
        let block = &blocks("[**bold** bit](x.md)", Some(&base()))[0];
        assert_eq!(block.links[0].text, "bold bit");
        assert_eq!(block.links[0].ranges.len(), 2, "one range per styled part");
    }

    #[test]
    fn runs_are_sorted_and_do_not_overlap() {
        // What the renderer feeds gpui directly: `with_highlights` and
        // `with_font_family_overrides` both want ranges in order, not overlapping, on character
        // boundaries. A block with several styles at once is where that could break.
        let block = &blocks(
            "- **bold** and `code` and [a *linked* link](x.md) wörld\n\npara *em* ~~gone~~\n",
            Some(&base()),
        )[0];
        assert!(!block.runs.is_empty(), "the block has styled parts");
        let mut prev_end = 0;
        for run in &block.runs {
            assert!(run.range.start >= prev_end, "runs out of order: {run:?}");
            assert!(
                block.text.is_char_boundary(run.range.start)
                    && block.text.is_char_boundary(run.range.end),
                "run {:?} is not on character boundaries",
                run.range
            );
            prev_end = run.range.end;
        }
    }

    #[test]
    fn an_unclosed_marker_while_streaming_stays_text() {
        assert_eq!(shown_all("plain, then **bold"), ["p plain, then **bold"]);
        assert_eq!(shown_all("plain, then **bold**"), ["p plain, then [b bold]"]);
    }

    #[test]
    fn rules_and_fences_are_their_own_blocks() {
        assert_eq!(
            shown_all("a\n\n---\n\nb\n```\ncode so far\n"),
            ["p a", "rule", "p b", "code code so far"]
        );
        // Still streaming: an open fence is code so far (the split `prose` settled on).
        assert_eq!(shown_all("a\n```\nb"), ["p a", "code b"]);
    }

    #[test]
    fn an_image_shows_its_alt_and_where_it_points() {
        assert_eq!(
            shown_all("see ![the map](m.png) here"),
            ["p see [a the map (site://memory/m.png)] here"]
        );
    }

    #[test]
    fn a_relative_link_with_no_page_cannot_be_resolved() {
        // The chat has no page. An absolute address still goes where it says.
        let block = &blocks(
            "see [user](user.md) and [plans](site://plans/x) and [me](mailto:me@example.com)",
            None,
        )[0];
        assert!(matches!(block.links[0].target, LinkTarget::Invalid(_)));
        assert_eq!(
            block.links[1].target,
            LinkTarget::Site(SiteUrl::parse("site://plans/x").unwrap())
        );
        assert_eq!(
            block.links[2].target,
            LinkTarget::External("mailto:me@example.com".to_string())
        );
    }
}
