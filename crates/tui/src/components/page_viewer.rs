use std::cell::RefCell;

use composer_core::{Frontmatter, SiteUrl};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::action::Action;
use crate::components::Component;
use crate::components::widgets::pane_block;
use crate::markdown::{self, Rendered, RenderedLink};
use crate::store::AppState;
use crate::store::types::{Mode, PageView, Pane};
use crate::theme;

const HINT_KEYS: &str = "asdfghjklqwertyuiopzxcvbnm";

/// Shows the current page; `f` labels its visible links (hint mode).
pub struct PageViewer {
    scroll: usize,
    pending_g: bool,
    hint_input: String,
    /// Rendered page, cached by (url, hash, width).
    cache: RefCell<Option<(String, String, u16, Rendered)>>,
    /// Hint labels of the links visible at the last render.
    hints: RefCell<Vec<(String, String)>>,
    height: std::cell::Cell<usize>,
}

impl PageViewer {
    pub fn new() -> Self {
        Self {
            scroll: 0,
            pending_g: false,
            hint_input: String::new(),
            cache: RefCell::new(None),
            hints: RefCell::new(Vec::new()),
            height: std::cell::Cell::new(20),
        }
    }

    /// Resets scrolling when another page is shown.
    pub fn page_changed(&mut self) {
        self.scroll = 0;
    }

    fn rendered(&self, page: &PageView, width: u16) -> Rendered {
        let mut cache = self.cache.borrow_mut();
        if let Some((url, hash, w, rendered)) = cache.as_ref()
            && *url == page.url
            && *hash == page.hash
            && *w == width
        {
            return rendered.clone();
        }
        let rendered = render_page(page, width);
        *cache = Some((page.url.clone(), page.hash.clone(), width, rendered.clone()));
        rendered
    }

    fn handle_hint_key(&mut self, key: KeyEvent) -> Vec<Action> {
        let KeyCode::Char(c) = key.code else {
            self.hint_input.clear();
            return vec![Action::SetMode(Mode::Normal)];
        };
        self.hint_input.push(c);
        let hints = self.hints.borrow();
        if let Some((_, url)) = hints.iter().find(|(label, _)| *label == self.hint_input) {
            let url = url.clone();
            self.hint_input.clear();
            return vec![Action::SetMode(Mode::Normal), Action::OpenUrl(url)];
        }
        if !hints
            .iter()
            .any(|(label, _)| label.starts_with(&self.hint_input))
        {
            self.hint_input.clear();
            return vec![Action::SetMode(Mode::Normal)];
        }
        vec![]
    }
}

impl Component for PageViewer {
    fn handle_key(&mut self, key: KeyEvent, state: &AppState) -> Vec<Action> {
        if state.mode() == Mode::Hint {
            return self.handle_hint_key(key);
        }
        let page = self.height.get().max(2) / 2;
        let g = std::mem::take(&mut self.pending_g);
        match (key.code, key.modifiers) {
            (KeyCode::Char('j') | KeyCode::Down, _) => self.scroll += 1,
            (KeyCode::Char('k') | KeyCode::Up, _) => self.scroll = self.scroll.saturating_sub(1),
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => self.scroll += page,
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                self.scroll = self.scroll.saturating_sub(page)
            }
            (KeyCode::Char('g'), _) if g => self.scroll = 0,
            (KeyCode::Char('g'), _) => self.pending_g = true,
            (KeyCode::Char('G'), _) => self.scroll = usize::MAX / 2,
            _ => {}
        }
        vec![]
    }

    fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let focused = state.ui.focus == Pane::Page;
        let title = match (&state.page.current, &state.page.loading) {
            (_, Some(url)) => Line::from(vec![
                Span::raw(" "),
                Span::styled(format!("{url} …"), theme::muted()),
                Span::raw(" "),
            ]),
            (Some(page), None) => Line::from(vec![
                Span::raw(" "),
                Span::styled(page.url.clone(), theme::text()),
                Span::raw(" "),
            ]),
            (None, None) => Line::from(Span::styled(" page ", theme::muted())),
        };
        let block = pane_block(title, focused);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let inner = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            ..inner
        };
        self.height.set(inner.height as usize);

        let Some(page) = &state.page.current else {
            let text = match &state.page.error {
                Some(error) => Span::styled(error.clone(), Style::new().fg(theme::ERR)),
                None => Span::styled(
                    "no page yet · space p finds one · the agent's pages appear here",
                    theme::faint(),
                ),
            };
            frame.render_widget(Paragraph::new(text), inner);
            self.hints.borrow_mut().clear();
            return;
        };

        let rendered = self.rendered(page, inner.width);
        let visible = inner.height as usize;
        let scroll = self
            .scroll
            .min(rendered.lines.len().saturating_sub(visible));
        let shown: Vec<Line> = rendered
            .lines
            .iter()
            .skip(scroll)
            .take(visible)
            .cloned()
            .collect();
        frame.render_widget(Paragraph::new(shown), inner);

        // Hint labels over the visible links, one label per link (first visible part).
        let mut hints = Vec::new();
        if state.mode() == Mode::Hint {
            let visible_links: Vec<&RenderedLink> = rendered
                .links
                .iter()
                .filter(|l| l.line >= scroll && l.line < scroll + visible)
                .fold(Vec::new(), |mut acc: Vec<&RenderedLink>, l| {
                    if !acc.iter().any(|a| a.id == l.id) {
                        acc.push(l);
                    }
                    acc
                });
            let labels = hint_labels(visible_links.len());
            let buffer = frame.buffer_mut();
            for (link, label) in visible_links.iter().zip(labels) {
                let y = inner.y + (link.line - scroll) as u16;
                let x = inner.x + link.start;
                let style = Style::new()
                    .fg(theme::BG)
                    .bg(theme::WARN)
                    .add_modifier(Modifier::BOLD);
                buffer.set_string(x, y, &label, style);
                hints.push((label, link.url.clone()));
            }
            for y in inner.top()..inner.bottom() {
                for x in inner.left()..inner.right() {
                    let cell = &mut buffer[(x, y)];
                    if cell.bg != theme::WARN && cell.fg != theme::ACCENT {
                        cell.set_fg(theme::FAINT);
                    }
                }
            }
        }
        *self.hints.borrow_mut() = hints;
    }
}

/// `a`, `s`, `d`, ... then two-letter labels when there are more links than keys.
fn hint_labels(count: usize) -> Vec<String> {
    let keys: Vec<char> = HINT_KEYS.chars().collect();
    if count <= keys.len() {
        return keys.iter().take(count).map(|c| c.to_string()).collect();
    }
    keys.iter()
        .flat_map(|a| keys.iter().map(move |b| format!("{a}{b}")))
        .take(count)
        .collect()
}

/// Frontmatter header, the markdown body, then backlinks.
fn render_page(page: &PageView, width: u16) -> Rendered {
    let base = SiteUrl::parse(&page.url).ok();
    let (_, body) = Frontmatter::split(&page.raw);
    let mut lines = Vec::new();

    if let Some(map) = page.frontmatter.as_object().filter(|m| !m.is_empty()) {
        let key_width = map.keys().map(|k| k.len()).max().unwrap_or(0).min(12);
        for (key, value) in map {
            let value_text = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => "—".to_string(),
                serde_json::Value::Array(items) => items
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                other => other.to_string(),
            };
            let style = match key.as_str() {
                "type" | "tags" | "kind" => Style::new().fg(theme::INFO),
                "status" => Style::new().fg(theme::WARN),
                _ if value_text.starts_with("site://") => theme::link(),
                _ => theme::text(),
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{key:<key_width$}  "), theme::faint()),
                Span::styled(value_text, style),
            ]));
        }
        lines.push(Line::raw(""));
    }
    if let Some(error) = &page.frontmatter_error {
        lines.push(Line::from(Span::styled(
            format!("invalid frontmatter: {error}"),
            Style::new().fg(theme::ERR),
        )));
        lines.push(Line::raw(""));
    }

    let offset = lines.len();
    let body = markdown::render(body, width, base.as_ref(), theme::text());
    lines.extend(body.lines);
    let mut links: Vec<RenderedLink> = body
        .links
        .into_iter()
        .map(|l| RenderedLink {
            line: l.line + offset,
            ..l
        })
        .collect();

    if !page.backlinks.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::from(Span::styled(
            "─".repeat(width as usize),
            theme::faint(),
        )));
        let next_id = links.iter().map(|l| l.id + 1).max().unwrap_or(0);
        for (i, url) in page.backlinks.iter().enumerate() {
            let label = if i == 0 { "backlinks  " } else { "           " };
            links.push(RenderedLink {
                id: next_id + i,
                line: lines.len(),
                start: 11,
                end: 11 + url.len() as u16,
                url: url.clone(),
            });
            lines.push(Line::from(vec![
                Span::styled(label, theme::faint()),
                Span::styled(url.clone(), theme::link()),
            ]));
        }
    }
    Rendered { lines, links }
}
