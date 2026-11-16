//! The native chat (docs/desktop.md, mockups/next 01): the session's conversation as a compact
//! transcript with speaker labels and rules, tool calls grouped inline, approvals where they
//! arise, and the modal composer pinned below. It holds the chat ([`Chat`], fed the session's
//! events by the shell) and its own view state; what it asks of the server goes out as
//! [`ChatEvent`]s.

use std::collections::HashSet;

use composer_client::{Approval, Chat, ChatItem};
use composer_protocol::{ApprovalDecision, ApprovalKind, Attachment, ServerMessage, TurnState};
use std::ops::Range;

use gpui::{
    AnyElement, Bounds, ClipboardItem, Context, ElementInputHandler, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, FontStyle, FontWeight, HighlightStyle, InteractiveText,
    KeyDownEvent, Pixels, Point, ScrollHandle, SharedString, StrikethroughStyle, StyledText,
    UTF16Selection, UnderlineStyle, Window, canvas, div, prelude::*, px, rgb,
};

use composer_core::{LinkTarget, SiteUrl};

use crate::draft::{Draft, DraftKey, Mode};
use crate::markdown;
use crate::outbox::Outbox;
use crate::prose::{head, thinking_preview};
use crate::term_pane::FONT;
use crate::theme::*;
use crate::uploads::{Upload, Uploads};
use crate::user_review::{VerdictCard, on_pass, on_send, sync_outbox, verdict_card};

/// The chat's prose font (bundled).
pub const PROSE: &str = "Inter";
/// Lines of a tool's result or an approval's body shown when opened.
const SHOWN_LINES: usize = 24;

/// What the chat asks of the window.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatEvent {
    Send {
        text: String,
        attachments: Vec<Attachment>,
        /// The review whose verdict this is, when it went into the outbox: a failed send takes
        /// it out again.
        review: Option<String>,
    },
    Answer {
        request: String,
        decision: ApprovalDecision,
    },
    Cancel,
    /// Older messages, before the first shown.
    LoadOlder,
    /// A link in the chat was clicked: the shell opens it as it opens any other.
    Open {
        target: LinkTarget,
    },
    /// Pick files to attach.
    Attach,
    /// Attach these: an image pasted from the clipboard (named), or copied files.
    PasteImage {
        name: String,
        bytes: Vec<u8>,
    },
    PasteFiles(Vec<std::path::PathBuf>),
}

pub struct ChatPane {
    chat: Option<Chat>,
    /// The session's agent, state and site (from its `session.updated`).
    agent: String,
    state: TurnState,
    site: Option<String>,
    approvals: Vec<Approval>,
    draft: Draft,
    files: Uploads,
    /// Sent and not back yet: the review's verdict, so its card shows it given at once.
    outbox: Outbox,
    /// Tool calls and approvals opened to show their details.
    opened: HashSet<String>,
    loading_older: bool,
    /// Text an input method is composing (in the draft, underlined), until it commits.
    marked: Option<Range<usize>>,
    /// Where the draft is drawn (an input method puts its candidates by it).
    draft_bounds: Bounds<Pixels>,
    scroll: ScrollHandle,
    focus: FocusHandle,
}

impl EventEmitter<ChatEvent> for ChatPane {}

impl Focusable for ChatPane {
    fn focus_handle(&self, _cx: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl ChatPane {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            chat: None,
            agent: String::new(),
            state: TurnState::Idle,
            site: None,
            approvals: Vec::new(),
            draft: Draft::default(),
            files: Uploads::default(),
            outbox: Outbox::default(),
            opened: HashSet::new(),
            loading_older: false,
            marked: None,
            draft_bounds: Bounds::default(),
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }

    /// The session shown now; its chat starts empty until its history arrives. The draft stays.
    pub fn open(&mut self, session: &str, cx: &mut Context<Self>) {
        if self.session() != Some(session) {
            self.chat = Some(Chat::new(session));
            self.opened.clear();
            self.loading_older = false;
            self.files.clear();
            cx.notify();
        }
    }

    pub fn session(&self) -> Option<&str> {
        self.chat.as_ref().map(|c| c.session.as_str())
    }

    pub fn last_seq(&self) -> u64 {
        self.chat.as_ref().map_or(0, |c| c.last_seq)
    }

    pub fn first_seq(&self) -> u64 {
        self.chat.as_ref().map_or(0, |c| c.first_seq)
    }

    /// What the session is: its agent, whether a turn runs, its site.
    pub fn set_info(
        &mut self,
        agent: &str,
        state: TurnState,
        site: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if state == TurnState::Idle
            && let Some(chat) = &self.chat
            && self.outbox.idle(&chat.session)
        {
            cx.notify();
        }
        if self.agent != agent || self.state != state || self.site != site {
            self.agent = agent.to_string();
            self.state = state;
            self.site = site;
            cx.notify();
        }
    }

    /// The approvals waiting in this session.
    pub fn set_approvals(&mut self, approvals: Vec<Approval>, cx: &mut Context<Self>) {
        if self.approvals != approvals {
            let more = approvals.len() > self.approvals.len();
            self.approvals = approvals;
            if more {
                self.scroll.scroll_to_bottom();
            }
            cx.notify();
        }
    }

    /// The session's last messages.
    pub fn history_loaded(
        &mut self,
        session: &str,
        events: &[ServerMessage],
        more: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(chat) = self.chat.as_mut().filter(|c| c.session == session) {
            chat.history_loaded(events, more);
            sync_outbox(&chat.items, session, &mut self.outbox);
            self.scroll.scroll_to_bottom();
            cx.notify();
        }
    }

    /// Older messages, put in front; the view stays on what it showed.
    pub fn older_loaded(
        &mut self,
        session: &str,
        events: &[ServerMessage],
        more: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(chat) = self.chat.as_mut().filter(|c| c.session == session) {
            let before = chat.items.len();
            chat.older_loaded(events, more);
            self.loading_older = false;
            let added = chat.items.len() - before;
            if added > 0 {
                self.scroll.scroll_to_top_of_item(added);
            }
            cx.notify();
        }
    }

    /// A transcript event; the view follows the end when it was there.
    pub fn apply(&mut self, message: &ServerMessage, cx: &mut Context<Self>) {
        let at_end = self.at_end();
        if let Some(chat) = self.chat.as_mut()
            && chat.apply(message)
        {
            sync_outbox(&chat.items, &chat.session, &mut self.outbox);
            if at_end {
                self.scroll.scroll_to_bottom();
            }
            cx.notify();
        }
    }

    /// Files from the picker or a drop, as they upload.
    pub fn uploading(&mut self, session: &str, name: String, cx: &mut Context<Self>) -> u64 {
        let id = self.files.start(session, name);
        cx.notify();
        id
    }

    pub fn uploaded(
        &mut self,
        session: &str,
        id: u64,
        result: Result<Attachment, String>,
        cx: &mut Context<Self>,
    ) {
        if self.files.complete(session, id, result) {
            cx.notify();
        }
    }

    /// Text put into the draft (writing goes on after it).
    pub fn paste(&mut self, text: &str, cx: &mut Context<Self>) {
        self.draft.mode = Mode::Insert;
        self.draft.insert(text);
        cx.notify();
    }

    /// A paste: an image or copied files become attachments, text goes into the draft.
    fn paste_clipboard(&mut self, clipboard: ClipboardItem, cx: &mut Context<Self>) {
        for entry in clipboard.entries() {
            match entry {
                gpui::ClipboardEntry::Image(image) => {
                    let ext = match image.format {
                        gpui::ImageFormat::Jpeg => "jpg",
                        gpui::ImageFormat::Webp => "webp",
                        gpui::ImageFormat::Gif => "gif",
                        gpui::ImageFormat::Svg => "svg",
                        gpui::ImageFormat::Bmp => "bmp",
                        gpui::ImageFormat::Tiff => "tiff",
                        _ => "png",
                    };
                    let n = self.files.len() + 1;
                    cx.emit(ChatEvent::PasteImage {
                        name: format!("pasted-{n}.{ext}"),
                        bytes: image.bytes.clone(),
                    });
                    return;
                }
                gpui::ClipboardEntry::ExternalPaths(paths) => {
                    cx.emit(ChatEvent::PasteFiles(paths.paths().to_vec()));
                    return;
                }
                gpui::ClipboardEntry::String(_) => {}
            }
        }
        if let Some(text) = clipboard.text() {
            self.paste(&text, cx);
        }
    }

    fn at_end(&self) -> bool {
        let max = self.scroll.max_offset().y;
        let offset = -self.scroll.offset().y;
        max - offset < px(40.)
    }

    fn busy(&self) -> bool {
        self.state != TurnState::Idle
    }

    /// The verdict sent for `review` in `session` didn't go: the card offers its buttons again.
    pub fn send_failed(&mut self, session: &str, review: &str, cx: &mut Context<Self>) {
        if self.outbox.failed(session, review) {
            cx.notify();
        }
    }

    /// The connection is back: what was sent before may have been lost with the server's inbox.
    pub fn reconnected(&mut self) {
        self.outbox.reconnected();
    }

    /// Pass on the verdict card: into the outbox and sent, once. A second click finds it there.
    fn pass(&mut self, cx: &mut Context<Self>) {
        if !self.files.ready() {
            return;
        }
        let Some(chat) = &self.chat else {
            return;
        };
        if let Some(verdict) = on_pass(&chat.items, &chat.session, &mut self.outbox) {
            self.emit_send(verdict.text, Some(verdict.review), cx);
            cx.notify();
        }
    }

    fn send(&mut self, text: String, cx: &mut Context<Self>) {
        if !self.files.ready() {
            return;
        }
        // A verdict written in the composer (Changes… starts `fail: `) shows on the card too.
        let review = self
            .chat
            .as_ref()
            .and_then(|chat| on_send(&chat.items, &chat.session, &text, &mut self.outbox));
        self.emit_send(text, review, cx);
    }

    fn emit_send(&mut self, text: String, review: Option<String>, cx: &mut Context<Self>) {
        let Some(attachments) = self.files.take() else {
            return;
        };
        self.scroll.scroll_to_bottom();
        cx.emit(ChatEvent::Send {
            text,
            attachments,
            review,
        });
    }

    fn send_draft(&mut self, cx: &mut Context<Self>) {
        if self.files.ready()
            && let DraftKey::Send(text) = self.draft.send()
        {
            self.send(text, cx);
            cx.notify();
        }
    }

    fn on_key_down(&mut self, ev: &KeyDownEvent, _w: &mut Window, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let m = &ks.modifiers;
        if m.alt || (m.control && m.shift) {
            return; // the window's shortcuts
        }
        let control = m.control || m.platform;
        if ks.key == "enter" && (control || self.draft.mode == Mode::Normal) && !self.files.ready()
        {
            cx.stop_propagation();
            return; // Do not let Draft::send clear text while attachments are unfinished.
        }
        if control && ks.key == "v" {
            if let Some(clipboard) = cx.read_from_clipboard() {
                self.paste_clipboard(clipboard, cx);
            }
            cx.stop_propagation();
            return;
        }
        if control && ks.key == "c" && self.busy() && self.draft.mode == Mode::Normal {
            cx.emit(ChatEvent::Cancel);
            cx.stop_propagation();
            return;
        }
        if self.draft.mode == Mode::Normal && !control {
            match ks.key_char.as_deref().unwrap_or(&ks.key) {
                "G" => {
                    self.scroll.scroll_to_bottom();
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                "g" => {
                    self.scroll.scroll_to_item(0);
                    cx.notify();
                    cx.stop_propagation();
                    return;
                }
                _ => {}
            }
        }
        // Typing in INSERT comes as text input (with an input method's composition), not keys.
        let typed = ks
            .key_char
            .as_deref()
            .is_some_and(|t| !t.is_empty() && !t.chars().any(char::is_control));
        if self.draft.mode == Mode::Insert && !control && (typed || self.marked.is_some()) {
            return;
        }
        match self.draft.key(&ks.key, ks.key_char.as_deref(), control) {
            DraftKey::Unhandled => return,
            DraftKey::Changed => {}
            DraftKey::Send(text) => self.send(text, cx),
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn copy(&self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }
}

/// A speaker's label: you in green, the agent in the light indigo.
fn speaker(name: &str, you: bool) -> impl IntoElement {
    div()
        .text_size(px(TEXT_SMALL))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgb(if you { OK } else { ACCENT_SOFT }))
        .mb(px(4.))
        .child(name.to_string())
}

/// A message: the rule on its left (green for you), the label when the speaker changes.
fn message(you: bool, label: Option<String>, body: Vec<AnyElement>) -> AnyElement {
    div()
        .pl(px(16.))
        .border_l_2()
        .border_color(rgb(if you { OK_TEXT } else { ACCENT_BORDER }))
        .flex()
        .flex_col()
        .gap(px(8.))
        .when_some(label, |d, label| d.child(speaker(&label, you)))
        .children(body)
        .into_any_element()
}

fn code(text: &str) -> AnyElement {
    div()
        .font_family(FONT)
        .text_size(px(TEXT_SIZE))
        .line_height(px(21.))
        .text_color(rgb(FG))
        .bg(rgb(BG_DEEP))
        .border_1()
        .border_color(rgb(BORDER))
        .rounded(px(R_CARD))
        .px(px(10.))
        .py(px(8.))
        .child(text.to_string())
        .into_any_element()
}

/// The heading sizes: the body size, stepped up (the mockups type scale), every one of them
/// SemiBold like the mockups heads.
fn heading_size(level: u8) -> f32 {
    match level {
        1 => TEXT_SIZE + 4.,
        2 => TEXT_SIZE + 3.,
        3 => TEXT_SIZE + 2.,
        _ => TEXT_SIZE + 1.,
    }
}

/// A model style as a gpui highlight: the weight and the slant have faces bundled for them.
/// How a run looks. Strong is SemiBold in Inter, but Bold in the mono face (a table, inline
/// code): JetBrains Mono is registered without a SemiBold, and would fall back to Regular.
fn highlight(style: &markdown::Style, mono_block: bool) -> HighlightStyle {
    let strong = if mono_block || style.mono {
        FontWeight::BOLD
    } else {
        FontWeight::SEMIBOLD
    };
    HighlightStyle {
        color: Some(
            rgb(if style.link.is_some() {
                ACCENT_SOFT
            } else {
                FG
            })
            .into(),
        ),
        font_weight: style.strong.then_some(strong),
        font_style: style.emphasis.then_some(FontStyle::Italic),
        strikethrough: style.strike.then_some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        }),
        ..HighlightStyle::default()
    }
}

/// A line across the message: the model rule, in the dim of a rule.
fn rule_line() -> AnyElement {
    div()
        .h(px(1.))
        .bg(rgb(BORDER))
        .my(px(8.))
        .into_any_element()
}

/// The quote wrap: a 2px left rule and a 12px indent, nothing when it is not quoted.
fn quoted(block: &markdown::Block, inner: AnyElement) -> AnyElement {
    if block.quote == 0 {
        return inner;
    }
    div()
        .border_l_2()
        .border_color(rgb(BORDER_2))
        .pl(px(12.))
        .child(inner)
        .into_any_element()
}

/// What the body of a block looks like: the prose font at the body size, tables in mono.
fn block_style(block: &markdown::Block) -> gpui::Div {
    let mono = matches!(block.kind, markdown::Kind::TableRow { .. });
    let size = match block.kind {
        markdown::Kind::Heading(level) => heading_size(level),
        _ => TEXT_SIZE,
    };
    div()
        .font_family(if mono { FONT } else { PROSE })
        .text_size(px(size))
        .line_height(px(23.))
        .text_color(rgb(FG))
        .when(matches!(block.kind, markdown::Kind::Heading(_)), |d| {
            d.font_weight(FontWeight::SEMIBOLD)
        })
}

/// A line between rules: something that happened (`cancelled`, a summary, a system note).
fn event(text: String) -> AnyElement {
    let rule = || div().flex_1().h(px(1.)).bg(rgb(BORDER));
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(12.))
        .text_size(px(TEXT_SMALL))
        .text_color(rgb(FG_FAINT))
        .child(rule())
        .child(
            div()
                .max_w(px(420.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(text),
        )
        .child(rule())
        .into_any_element()
}

fn button(
    id: impl Into<SharedString>,
    label: &'static str,
    filled: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(gpui::ElementId::Name(id.into()))
        .px(px(9.))
        .py(px(3.))
        .rounded(px(R_CONTROL))
        .cursor_pointer()
        .text_size(px(TEXT_SMALL))
        .when(filled, |d| {
            d.bg(rgb(ACCENT))
                .text_color(rgb(FG_BRIGHT))
                .hover(|d| d.bg(rgb(ACCENT_HOVER)))
        })
        .when(!filled, |d| {
            d.bg(rgb(SURFACE_2))
                .text_color(rgb(FG))
                .hover(|d| d.bg(rgb(BORDER_2)))
        })
        .child(label)
}

impl ChatPane {
    /// A message as markdown: its blocks, each drawn at its own type; GPUI does the wrapping.
    /// Links written relative to a page resolve against the session's site.
    fn markdown(&self, text: &str, key: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let base = self
            .site
            .as_deref()
            .and_then(|site| SiteUrl::new(site, "").ok());
        markdown::blocks(text, base.as_ref())
            .iter()
            .enumerate()
            .map(|(n, block)| self.block(block, &format!("{key}-{n}"), cx))
            .collect()
    }

    /// One block of a message.
    fn block(&self, block: &markdown::Block, key: &str, cx: &mut Context<Self>) -> AnyElement {
        match block.kind {
            markdown::Kind::Code => code(&block.text),
            markdown::Kind::Rule => rule_line(),
            markdown::Kind::Item(marker) => {
                // the hanging indent: the marker in its own column, the text beside it
                let dim = if marker == markdown::Marker::Task(true) {
                    OK
                } else {
                    FG_DIM
                };
                quoted(
                    block,
                    div()
                        .flex()
                        .flex_row()
                        .ml(px(14. * block.depth as f32))
                        .child(
                            div()
                                .font_family(PROSE)
                                .text_size(px(TEXT_SIZE))
                                .line_height(px(23.))
                                .text_color(rgb(dim))
                                .child(marker.glyph()),
                        )
                        .child(
                            // min width 0: a flex item is otherwise as wide as its unwrapped
                            // text, and the item runs under the stage instead of wrapping
                            block_style(block)
                                .flex_1()
                                .min_w(px(0.))
                                .child(self.block_text(block, key, cx)),
                        )
                        .into_any_element(),
                )
            }
            _ => quoted(
                block,
                block_style(block)
                    .child(self.block_text(block, key, cx))
                    .into_any_element(),
            ),
        }
    }

    /// A block's text: the model runs as gpui highlights and font overrides (sorted, not
    /// overlapping, on character boundaries), and its link ranges as the click targets gpui
    /// hit-tests through the text layout (it shows the pointer cursor over them itself).
    fn block_text(&self, block: &markdown::Block, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let plain = markdown::Style::default();
        let mono_block = matches!(block.kind, markdown::Kind::TableRow { .. });
        let highlights: Vec<(Range<usize>, HighlightStyle)> = block
            .runs
            .iter()
            .filter(|run| run.style != plain)
            .map(|run| (run.range.clone(), highlight(&run.style, mono_block)))
            .collect();
        let fonts: Vec<(Range<usize>, SharedString)> = block
            .runs
            .iter()
            .filter(|run| run.style.mono)
            .map(|run| (run.range.clone(), FONT.into()))
            .collect();
        let mut styled = StyledText::new(block.text.clone());
        if !highlights.is_empty() {
            styled = styled.with_highlights(highlights);
        }
        if !fonts.is_empty() {
            styled = styled.with_font_family_overrides(fonts);
        }
        if block.links.is_empty() {
            return styled.into_any_element();
        }
        let (ranges, targets): (Vec<Range<usize>>, Vec<LinkTarget>) =
            markdown::click_targets(block).into_iter().unzip();
        let entity = cx.entity();
        InteractiveText::new(gpui::ElementId::Name(key.into()), styled)
            .on_click(ranges, move |ix, _window, cx| {
                let target = targets[ix].clone();
                entity.update(cx, |_, cx| cx.emit(ChatEvent::Open { target }));
            })
            .into_any_element()
    }

    /// Consecutive tool calls, in one box: a row each, opened to show what it changed and said.
    fn tools(&self, calls: &[&ChatItem], cx: &mut Context<Self>) -> AnyElement {
        let mut rows = Vec::new();
        for (n, item) in calls.iter().enumerate() {
            let ChatItem::Tool {
                call_id,
                name,
                summary,
                detail,
                nested,
                result,
                is_error,
            } = item
            else {
                continue;
            };
            let (mark, colour) = match (result, is_error) {
                (None, _) => ("◌", FG_FAINT),
                (Some(_), true) => ("✗", ERR),
                (Some(_), false) => ("✓", OK),
            };
            let open = self.opened.contains(call_id);
            let id = call_id.clone();
            rows.push(
                div()
                    .id(gpui::ElementId::Name(format!("tool-{call_id}").into()))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(7.))
                    .px(px(9.))
                    .py(px(6.))
                    .when(*nested, |d| d.pl(px(25.)))
                    .when(n > 0, |d| d.border_t_1().border_color(rgb(BORDER)))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(PANEL_2)))
                    .on_click(cx.listener(move |pane, _, _, cx| {
                        if !pane.opened.remove(&id) {
                            pane.opened.insert(id.clone());
                        }
                        cx.notify();
                    }))
                    .child(div().text_color(rgb(colour)).child(mark))
                    .child(div().flex_none().text_color(rgb(FG)).child(name.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(rgb(FG_FAINT))
                            .child(summary.clone()),
                    )
                    .child(
                        div()
                            .text_color(rgb(FG_FAINT))
                            .child(if open { "⌄" } else { "›" }),
                    )
                    .into_any_element(),
            );
            if open {
                let mut shown = String::new();
                if let Some(detail) = detail {
                    shown.push_str(detail);
                    shown.push_str("\n\n");
                }
                shown.push_str(result.as_deref().unwrap_or("(running)"));
                let (text, left) = head(shown.trim(), SHOWN_LINES);
                let all = shown.clone();
                rows.push(
                    div()
                        .id(gpui::ElementId::Name(format!("tool-out-{call_id}").into()))
                        .px(px(12.))
                        .py(px(8.))
                        .bg(rgb(BG_DEEP))
                        .border_t_1()
                        .border_color(rgb(BORDER))
                        .font_family(FONT)
                        .text_size(px(TEXT_SMALL))
                        .line_height(px(16.))
                        .text_color(rgb(if *is_error { ERR_TEXT } else { FG_DIM }))
                        .child(text)
                        .when(left > 0, |d| {
                            d.child(
                                div()
                                    .pt(px(6.))
                                    .text_color(rgb(FG_FAINT))
                                    .child(format!("… {left} more lines")),
                            )
                        })
                        .child(
                            div()
                                .id(gpui::ElementId::Name(format!("copy-{call_id}").into()))
                                .pt(px(6.))
                                .text_color(rgb(FG_FAINT))
                                .cursor_pointer()
                                .hover(|d| d.text_color(rgb(FG)))
                                .on_click(
                                    cx.listener(move |pane, _, _, cx| pane.copy(all.clone(), cx)),
                                )
                                .child("copy"),
                        )
                        .into_any_element(),
                );
            }
        }
        div()
            .flex()
            .flex_col()
            .border_1()
            .border_color(rgb(BORDER))
            .rounded(px(R_CARD))
            .overflow_hidden()
            .bg(rgb(PANEL))
            .font_family(FONT)
            .text_size(px(TEXT_SMALL))
            .children(rows)
            .into_any_element()
    }

    /// The user review's verdict (site://coding): Pass sends `pass`; Changes… starts `fail: `
    /// in the composer for the why. The notes themselves stay in Hunk. Once a verdict is in the
    /// outbox the card says so in place of its buttons, until the message comes back.
    fn verdict(&self, card: &VerdictCard, cx: &mut Context<Self>) -> AnyElement {
        let what = if card.ticket.is_empty() {
            "your review".to_string()
        } else {
            format!("your review of {}", card.ticket)
        };
        let hint = match &card.sent {
            None => "Hunk is on the stage: notes with c, then your verdict",
            Some(_) if self.busy() => "sent: the agent reads it when this turn ends",
            Some(_) => "sent: waiting for the agent",
        };
        let row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(9.))
            .px(px(10.))
            .py(px(7.))
            .border_1()
            .border_color(rgb(if card.sent.is_some() {
                BORDER
            } else {
                WARN_DEEP
            }))
            .bg(rgb(PANEL_2))
            .rounded(px(R_CARD))
            .font_family(FONT)
            .text_size(px(TEXT_SMALL))
            .child(
                div()
                    .flex_none()
                    .text_color(rgb(if card.sent.is_some() { FG_DIM } else { WARN }))
                    .child(format!("◆ {what}")),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(rgb(FG_FAINT))
                    .child(hint),
            );
        match &card.sent {
            // Given: what you said, quietly, and nothing to click again.
            Some(sent) => row
                .child(
                    div()
                        .flex_none()
                        .max_w(px(220.))
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_color(rgb(OK))
                        .child(format!("✓ {}", sent.trim())),
                )
                .into_any_element(),
            None => row
                .child(
                    button("verdict-changes", "Changes…", false).on_click(cx.listener(
                        |pane, _, window, cx| {
                            pane.draft = Draft {
                                text: "fail: ".into(),
                                cursor: "fail: ".len(),
                                mode: Mode::Insert,
                            };
                            window.focus(&pane.focus, cx);
                            cx.notify();
                        },
                    )),
                )
                .child(
                    button("verdict-pass", "Pass", true)
                        .on_click(cx.listener(|pane, _, _, cx| pane.pass(cx))),
                )
                .into_any_element(),
        }
    }

    /// One thinking block throughout streaming and completion, opened on a click. Its first
    /// line stays put as new paragraphs arrive; expanded, all the thinking grows in place.
    fn thinking(&self, n: usize, text: &str, live: bool, cx: &mut Context<Self>) -> AnyElement {
        let key = format!("thinking-{n}");
        let open = self.opened.contains(&key);
        let (first, words) = thinking_preview(text);
        let label = format!(
            "{} thought · {words} words{}",
            if open { "⌄" } else { "›" },
            if live { " …" } else { "" },
        );
        let toggle = key.clone();
        div()
            .id(gpui::ElementId::Name(key.into()))
            .flex()
            .flex_row()
            .gap(px(8.))
            .font_family(PROSE)
            .text_size(px(TEXT_SIZE))
            .line_height(px(22.))
            .text_color(rgb(FG_FAINT))
            .cursor_pointer()
            .hover(|d| d.text_color(rgb(FG_DIM)))
            .on_click(cx.listener(move |pane, _, _, cx| {
                if !pane.opened.remove(&toggle) {
                    pane.opened.insert(toggle.clone());
                }
                cx.notify();
            }))
            .child(
                div()
                    .flex_none()
                    .font_family(FONT)
                    .text_size(px(TEXT_SMALL))
                    .child(label),
            )
            .child(if open {
                div().flex_1().min_w(px(0.)).child(text.trim().to_string())
            } else {
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(first.to_string())
            })
            .into_any_element()
    }

    fn approval(&self, approval: &Approval, cx: &mut Context<Self>) -> AnyElement {
        let what = match approval.kind {
            ApprovalKind::Recipe => format!("run {} › {}", approval.site, approval.recipe),
            _ if !approval.title.is_empty() => approval.title.clone(),
            ApprovalKind::Merge => "merge".into(),
            ApprovalKind::Tool | ApprovalKind::Mcp => "use a tool".into(),
            ApprovalKind::Publish => "publish".into(),
        };
        let args = approval.args.join(" ");
        let open = self.opened.contains(&approval.request);
        let request = approval.request.clone();
        let answer = |decision: ApprovalDecision| {
            let request = approval.request.clone();
            cx.listener(move |_, _, _, cx| {
                cx.emit(ChatEvent::Answer {
                    request: request.clone(),
                    decision,
                })
            })
        };
        let (body, left) = head(approval.body.trim(), SHOWN_LINES);
        div()
            .flex()
            .flex_col()
            .border_1()
            .border_color(rgb(ACCENT_BORDER))
            .bg(rgb(ACCENT_DEEP))
            .rounded(px(R_CARD))
            .overflow_hidden()
            .font_family(FONT)
            .text_size(px(TEXT_SMALL))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(9.))
                    .px(px(10.))
                    .py(px(7.))
                    .child(
                        div()
                            .flex_none()
                            .text_color(rgb(ACCENT_SOFT))
                            .child(format!("⚑ {what}")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(rgb(FG_FAINT))
                            .child(args),
                    )
                    .when(!approval.body.trim().is_empty(), |d| {
                        d.child(
                            div()
                                .id(gpui::ElementId::Name(format!("show-{request}").into()))
                                .text_color(rgb(FG_DIM))
                                .cursor_pointer()
                                .hover(|d| d.text_color(rgb(FG)))
                                .on_click(cx.listener(move |pane, _, _, cx| {
                                    if !pane.opened.remove(&request) {
                                        pane.opened.insert(request.clone());
                                    }
                                    cx.notify();
                                }))
                                .child(if open { "hide" } else { "show" }),
                        )
                    })
                    .child(
                        button(format!("deny-{}", approval.request), "Deny", false)
                            .on_click(answer(ApprovalDecision::Deny)),
                    )
                    .when(approval.rule.is_some(), |d| {
                        d.child(
                            button(format!("always-{}", approval.request), "Always", false)
                                .on_click(answer(ApprovalDecision::Always)),
                        )
                    })
                    .child(
                        button(format!("allow-{}", approval.request), "Allow", true)
                            .on_click(answer(ApprovalDecision::Approve)),
                    ),
            )
            .when(open, |d| {
                d.child(
                    div()
                        .px(px(12.))
                        .py(px(8.))
                        .bg(rgb(BG_DEEP))
                        .border_t_1()
                        .border_color(rgb(ACCENT_BORDER))
                        .line_height(px(16.))
                        .text_color(rgb(FG_DIM))
                        .child(body)
                        .when(left > 0, |d| {
                            d.child(
                                div()
                                    .pt(px(6.))
                                    .text_color(rgb(FG_FAINT))
                                    .child(format!("… {left} more lines")),
                            )
                        }),
                )
            })
            .when_some(approval.rule.clone(), |d, rule| {
                d.child(
                    div()
                        .px(px(10.))
                        .pb(px(7.))
                        .text_color(rgb(FG_FAINT))
                        .child(format!("always allows {rule} in this project")),
                )
            })
            .into_any_element()
    }

    /// The transcript's rows: messages with their tool boxes, events, the streaming reply, the
    /// approvals waiting.
    fn transcript(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(chat) = &self.chat else {
            return vec![];
        };
        let agent = if self.agent.is_empty() {
            "agent"
        } else {
            &self.agent
        };
        let mut rows: Vec<AnyElement> = Vec::new();
        if chat.more {
            rows.push(
                div()
                    .id("older")
                    .flex()
                    .justify_center()
                    .text_size(px(TEXT_SMALL))
                    .text_color(rgb(FG_DIM))
                    .cursor_pointer()
                    .hover(|d| d.text_color(rgb(FG)))
                    .on_click(cx.listener(|pane, _, _, cx| {
                        if !pane.loading_older {
                            pane.loading_older = true;
                            cx.emit(ChatEvent::LoadOlder);
                            cx.notify();
                        }
                    }))
                    .child(if self.loading_older {
                        "loading…"
                    } else {
                        "Earlier messages ↑"
                    })
                    .into_any_element(),
            );
        }
        // Who spoke last: a message by the same speaker continues without its label.
        let mut last: Option<bool> = None;
        // The agent's message being gathered: its text and tool boxes.
        let mut body: Vec<AnyElement> = Vec::new();
        let mut body_label: Option<String> = None;
        let mut tools: Vec<&ChatItem> = Vec::new();
        let flush_tools = |tools: &mut Vec<&ChatItem>,
                           body: &mut Vec<AnyElement>,
                           pane: &Self,
                           cx: &mut Context<Self>| {
            if !tools.is_empty() {
                body.push(pane.tools(tools, cx));
                tools.clear();
            }
        };
        let flush =
            |body: &mut Vec<AnyElement>, label: &mut Option<String>, rows: &mut Vec<AnyElement>| {
                if !body.is_empty() {
                    rows.push(message(false, label.take(), std::mem::take(body)));
                }
            };
        for (n, item) in chat.items.iter().enumerate() {
            match item {
                ChatItem::Tool { .. } => {
                    if last != Some(false) {
                        body_label = Some(agent.to_string());
                        last = Some(false);
                    }
                    tools.push(item);
                }
                ChatItem::Text { text } | ChatItem::Thinking { text } => {
                    flush_tools(&mut tools, &mut body, self, cx);
                    if last != Some(false) {
                        body_label = Some(agent.to_string());
                        last = Some(false);
                    }
                    if let ChatItem::Thinking { .. } = item {
                        body.push(self.thinking(n, text, false, cx));
                    } else {
                        body.extend(self.markdown(text, &format!("md-{n}"), cx));
                    }
                }
                ChatItem::User {
                    text,
                    source,
                    files,
                } => {
                    flush_tools(&mut tools, &mut body, self, cx);
                    flush(&mut body, &mut body_label, &mut rows);
                    let label = source.clone().unwrap_or_else(|| "you".into());
                    // Your messages are markdown too, like the agent's (a desktop choice: the
                    // phone shows them plain).
                    let mut parts = self.markdown(text, &format!("you-{n}"), cx);
                    if !files.is_empty() {
                        parts.push(
                            div()
                                .text_size(px(TEXT_SMALL))
                                .text_color(rgb(FG_DIM))
                                .child(format!("＋ {}", files.join(", ")))
                                .into_any_element(),
                        );
                    }
                    rows.push(message(true, Some(label), parts));
                    last = Some(true);
                }
                ChatItem::System { text } => {
                    flush_tools(&mut tools, &mut body, self, cx);
                    flush(&mut body, &mut body_label, &mut rows);
                    rows.push(event(text.clone()));
                    last = None;
                }
                ChatItem::Compacted { messages, .. } => {
                    flush_tools(&mut tools, &mut body, self, cx);
                    flush(&mut body, &mut body_label, &mut rows);
                    rows.push(event(format!("✦ {messages} earlier messages summarized")));
                    last = None;
                }
            }
        }
        flush_tools(&mut tools, &mut body, self, cx);
        // The reply streaming in, after what the agent already did this turn.
        if let Some(streaming) = &chat.streaming {
            if last != Some(false) {
                body_label = Some(agent.to_string());
            }
            if !streaming.thinking.trim().is_empty() {
                // MessageDone inserts Thinking at this index, preserving its key and open
                // state. Answer text must not hide the thinking before it settles.
                body.push(self.thinking(
                    chat.items.len(),
                    &streaming.thinking,
                    streaming.content.is_empty(),
                    cx,
                ));
            }
            if !streaming.content.is_empty() {
                body.extend(self.markdown(&format!("{}▍", streaming.content), "md-stream", cx));
            }
        } else if self.busy() {
            if last != Some(false) {
                body_label = Some(agent.to_string());
            }
            body.push(
                div()
                    .text_size(px(TEXT_SMALL))
                    .text_color(rgb(FG_FAINT))
                    .child(if self.state == TurnState::AwaitingApproval {
                        "waiting for your answer"
                    } else {
                        "◌ working…"
                    })
                    .into_any_element(),
            );
        }
        for approval in &self.approvals {
            if body.is_empty() && body_label.is_none() {
                body_label = Some(agent.to_string());
            }
            body.push(self.approval(approval, cx));
        }
        // A Hunk review open on the stage: your verdict goes back as a message.
        if let Some(card) = verdict_card(&chat.items, &chat.session, &self.outbox) {
            if body.is_empty() && body_label.is_none() {
                body_label = Some(agent.to_string());
            }
            body.push(self.verdict(&card, cx));
        }
        flush(&mut body, &mut body_label, &mut rows);
        if chat.loaded && rows.is_empty() {
            rows.push(event("a new conversation".into()));
        }
        rows
    }

    /// The draft with its cursor: a bar in INSERT, the character under it lit in NORMAL.
    fn draft_text(&self) -> AnyElement {
        let Draft { text, cursor, mode } = &self.draft;
        let (shown, range) = match mode {
            Mode::Insert => {
                let mut shown = text.clone();
                shown.insert(*cursor, '▏');
                (shown, *cursor..*cursor + '▏'.len_utf8())
            }
            Mode::Normal => match text[*cursor..].chars().next() {
                Some('\n') | None => {
                    let mut shown = text.clone();
                    shown.insert(*cursor, ' ');
                    (shown, *cursor..*cursor + 1)
                }
                Some(c) => (text.clone(), *cursor..*cursor + c.len_utf8()),
            },
        };
        let style = match mode {
            Mode::Insert => HighlightStyle {
                color: Some(rgb(ACCENT_SOFT).into()),
                ..HighlightStyle::default()
            },
            Mode::Normal => HighlightStyle {
                background_color: Some(rgb(ACCENT).into()),
                color: Some(rgb(FG_BRIGHT).into()),
                ..HighlightStyle::default()
            },
        };
        let mut highlights = vec![(range, style)];
        // The composition, underlined (it ends where the cursor is, so it comes first).
        if let Some(marked) = self.marked.clone().filter(|m| m.end <= *cursor) {
            highlights.insert(
                0,
                (
                    marked,
                    HighlightStyle {
                        underline: Some(UnderlineStyle {
                            thickness: px(1.),
                            color: Some(rgb(ACCENT_SOFT).into()),
                            wavy: false,
                        }),
                        ..HighlightStyle::default()
                    },
                ),
            );
        }
        div()
            .font_family(PROSE)
            .text_size(px(TEXT_SIZE))
            .line_height(px(22.))
            .text_color(rgb(FG_BRIGHT))
            .child(StyledText::new(shown).with_highlights(highlights))
            .into_any_element()
    }

    fn composer(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let focused = self.focus.is_focused(window);
        let insert = self.draft.mode == Mode::Insert;
        let key = |text: &'static str| {
            div()
                .px(px(4.))
                .border_1()
                .border_color(rgb(BORDER_2))
                .rounded(px(R_SM))
                .text_color(rgb(FG_DIM))
                .child(text)
        };
        let hint = |k: &'static str, what: &'static str| {
            div()
                .flex()
                .flex_row()
                .gap(px(5.))
                .child(key(k))
                .child(what)
        };
        let mode_line = div()
            .flex()
            .flex_row()
            .items_center()
            .flex_wrap()
            .gap(px(8.))
            .text_size(px(TEXT_SMALL))
            .text_color(rgb(FG_FAINT))
            .child(
                div()
                    .mr(px(12.))
                    .text_size(px(TEXT_SMALL))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(if insert { ACCENT_SOFT } else { FG_DIM }))
                    .child(if insert { "I N S E R T" } else { "N O R M A L" }),
            )
            .when(insert, |d| {
                d.child(hint("Esc", "normal"))
                    .child("·")
                    .child(hint("Enter", "new line"))
                    .child("·")
                    .child(hint("ctrl+Enter", "send"))
            })
            .when(!insert, |d| {
                d.child(hint("i", "insert"))
                    .child("·")
                    .child(hint("o", "new line"))
                    .child("·")
                    .child(hint("Enter", "send"))
            });
        let placeholder = if insert {
            format!(
                "Write to {}…",
                if self.agent.is_empty() {
                    "the agent"
                } else {
                    &self.agent
                }
            )
        } else if self.approvals.is_empty() {
            "Press i to write".to_string()
        } else {
            "Press i to write, or answer above".to_string()
        };
        let body = if self.draft.text.is_empty() && !(focused && insert) {
            div()
                .font_family(PROSE)
                .text_size(px(TEXT_SIZE))
                .line_height(px(22.))
                .text_color(rgb(FG_FAINT))
                .child(placeholder)
                .into_any_element()
        } else if self.draft.text.is_empty() {
            div()
                .font_family(PROSE)
                .text_size(px(TEXT_SIZE))
                .line_height(px(22.))
                .text_color(rgb(FG_FAINT))
                .child(
                    StyledText::new(format!("▏{placeholder}")).with_highlights(vec![(
                        0..'▏'.len_utf8(),
                        HighlightStyle {
                            color: Some(rgb(ACCENT_SOFT).into()),
                            ..HighlightStyle::default()
                        },
                    )]),
                )
                .into_any_element()
        } else if focused {
            self.draft_text()
        } else {
            div()
                .font_family(PROSE)
                .text_size(px(TEXT_SIZE))
                .line_height(px(22.))
                .text_color(rgb(FG))
                .child(self.draft.text.clone())
                .into_any_element()
        };
        let files = self.files.iter().enumerate().map(|(n, f)| {
            let (label, colour) = match f {
                Upload::Uploading { name, .. } => (format!("{name} …"), FG_FAINT),
                Upload::Ready(a) => (a.name.clone(), FG_DIM),
                Upload::Failed(name, e) => (format!("{name}: {e}"), ERR_TEXT),
            };
            div()
                .id(("file", n))
                .px(px(7.))
                .py(px(2.))
                .rounded(px(R_SM))
                .bg(rgb(SURFACE_2))
                .text_color(rgb(colour))
                .cursor_pointer()
                .on_click(cx.listener(move |pane, _, _, cx| {
                    if n < pane.files.len() {
                        pane.files.remove(n);
                        cx.notify();
                    }
                }))
                .child(format!("{label} ×"))
        });
        let can_send = !self.draft.text.trim().is_empty() && self.files.ready();
        div()
            .id("composer")
            .flex_none()
            .mx(px(17.))
            .mb(px(17.))
            .p(px(13.))
            .bg(rgb(SURFACE))
            .border_1()
            .border_color(rgb(if focused && insert { ACCENT } else { BORDER_2 }))
            .rounded(px(R_CARD))
            .flex()
            .flex_col()
            .gap(px(10.))
            .font_family(FONT)
            .child(mode_line)
            .child(
                div()
                    .relative()
                    .min_h(px(22.))
                    .max_h(px(220.))
                    .overflow_hidden()
                    .child(body)
                    .child({
                        // Text input (and an input method) goes to the draft while it has focus.
                        let pane = cx.entity();
                        let focus = self.focus.clone();
                        canvas(
                            |_, _, _| {},
                            move |bounds, _, window, cx| {
                                pane.update(cx, |pane, _| pane.draft_bounds = bounds);
                                window.handle_input(
                                    &focus,
                                    ElementInputHandler::new(bounds, pane.clone()),
                                    cx,
                                );
                            },
                        )
                        .absolute()
                        .inset_0()
                    }),
            )
            .when(!self.files.is_empty(), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(6.))
                        .text_size(px(TEXT_SMALL))
                        .children(files),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(12.))
                    .text_size(px(TEXT_SMALL))
                    .text_color(rgb(FG_DIM))
                    .child(
                        div()
                            .id("attach")
                            .cursor_pointer()
                            .hover(|d| d.text_color(rgb(FG)))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::Attach)))
                            .child("＋ Attach"),
                    )
                    .when_some(self.site.clone(), |d, site| {
                        d.child(
                            div()
                                .px(px(6.))
                                .py(px(1.))
                                .rounded(px(R_SM))
                                .bg(rgb(ACCENT_DEEP))
                                .border_1()
                                .border_color(rgb(ACCENT_BORDER))
                                .text_color(rgb(ACCENT_SOFT))
                                .child(format!("site://{site}")),
                        )
                    })
                    .child(div().flex_1())
                    .when(self.busy(), |d| {
                        d.child(
                            div()
                                .id("stop")
                                .cursor_pointer()
                                .text_color(rgb(WARN))
                                .hover(|d| d.text_color(rgb(FG_BRIGHT)))
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::Cancel)))
                                .child("■ Stop"),
                        )
                    })
                    .child(
                        div()
                            .id("send")
                            .px(px(9.))
                            .py(px(3.))
                            .rounded(px(R_CONTROL))
                            .when(can_send, |d| {
                                d.cursor_pointer()
                                    .bg(rgb(ACCENT))
                                    .text_color(rgb(FG_BRIGHT))
                                    .hover(|d| d.bg(rgb(ACCENT_HOVER)))
                                    .on_click(cx.listener(|pane, _, _, cx| {
                                        pane.send_draft(cx);
                                    }))
                            })
                            .when(!can_send, |d| d.text_color(rgb(FG_FAINT)))
                            .child("Send ↑"),
                    ),
            )
            .into_any_element()
    }
}

impl Render for ChatPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self.transcript(cx);
        let opening = self.chat.as_ref().is_some_and(|c| !c.loaded);
        let composer = self.composer(window, cx);
        div()
            .track_focus(&self.focus)
            .key_context("chat")
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|pane, _, window, cx| {
                    window.focus(&pane.focus, cx);
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG))
            .text_color(rgb(FG))
            .text_size(px(TEXT_SIZE))
            .child(
                div()
                    .id("transcript")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .px(px(24.))
                    .pt(px(22.))
                    .pb(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(24.))
                    .when(opening, |d| {
                        d.child(event("opening the conversation…".into()))
                    })
                    .when(self.chat.is_none(), |d| {
                        d.child(event("no session yet".into()))
                    })
                    .children(rows),
            )
            .child(composer)
    }
}

/// A UTF-16 offset in `text` as a byte offset (on a char boundary).
fn from_utf16(text: &str, utf16: usize) -> usize {
    let mut count = 0;
    for (i, c) in text.char_indices() {
        if count >= utf16 {
            return i;
        }
        count += c.len_utf16();
    }
    text.len()
}

fn to_utf16(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

impl EntityInputHandler for ChatPane {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let text = &self.draft.text;
        let bytes = from_utf16(text, range.start)..from_utf16(text, range.end);
        *adjusted = Some(to_utf16(text, bytes.start)..to_utf16(text, bytes.end));
        Some(text[bytes].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let at = to_utf16(&self.draft.text, self.draft.cursor);
        Some(UTF16Selection {
            range: at..at,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        let text = &self.draft.text;
        self.marked
            .as_ref()
            .map(|m| to_utf16(text, m.start)..to_utf16(text, m.end))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked = None;
    }

    /// Typed or committed text: written in INSERT only (NORMAL's letters are commands).
    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.draft.mode != Mode::Insert {
            self.marked = None;
            return;
        }
        let replaced = self.replaced(range);
        self.draft.text.replace_range(replaced.clone(), text);
        self.draft.cursor = replaced.start + text.len();
        self.marked = None;
        cx.notify();
    }

    /// An input method's composition so far: in the draft, marked, until it commits.
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.draft.mode != Mode::Insert {
            return;
        }
        let replaced = self.replaced(range);
        self.draft.text.replace_range(replaced.clone(), text);
        self.draft.cursor = replaced.start + text.len();
        self.marked = (!text.is_empty()).then(|| replaced.start..replaced.start + text.len());
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // The draft's first line: near enough for a candidate window.
        Some(Bounds::new(
            self.draft_bounds.origin,
            gpui::size(self.draft_bounds.size.width, px(22.)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl ChatPane {
    /// The bytes a replacement covers: the given UTF-16 range, else the composition, else the
    /// cursor.
    fn replaced(&self, range: Option<Range<usize>>) -> Range<usize> {
        let text = &self.draft.text;
        match (range, &self.marked) {
            (Some(r), _) => from_utf16(text, r.start)..from_utf16(text, r.end),
            (None, Some(m)) => m.clone(),
            (None, None) => self.draft.cursor..self.draft.cursor,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_offsets() {
        let text = "a€😀b";
        assert_eq!(from_utf16(text, 0), 0);
        assert_eq!(from_utf16(text, 2), 4, "after the euro sign");
        assert_eq!(from_utf16(text, 4), 8, "after the emoji's two units");
        assert_eq!(to_utf16(text, 8), 4);
        assert_eq!(from_utf16(text, 9), text.len());
    }
}
