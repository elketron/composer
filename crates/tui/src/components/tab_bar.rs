//! The stage's tab bar (plan phase 12, ticket 0208): the tabs this terminal can draw, from the
//! session's desk, with the shown one marked. One row at the top of the stage pane, above the
//! view of the tab that is shown.
//!
//! It draws exactly what [`AppState::stage_tabs`] says is open, and clicks back as an index into
//! that same list, so the bar and the keys cannot disagree about what is there.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::components::widgets::{fit, pad};
use crate::store::{AppState, StageTab};
use crate::theme;

/// A stage this short gets no bar: the view needs the rows.
pub const MIN_STAGE_HEIGHT: u16 = 4;

/// What the bar calls itself, so the row is not mistaken for a pane border.
const LABEL: &str = " stage ";

/// Columns a tab title may use before it is cut.
const TITLE_MAX: usize = 26;

/// Offered on the right of the row, when the tabs leave room for it.
const HINT: &str = "ctrl-tab switch · ctrl-w c close";

/// Between two tabs.
const SEP: &str = " │ ";

/// The row of tabs. Not a [`Component`](crate::components::Component): it takes no keys, only
/// clicks, and Root turns those into actions like everyone else's.
pub struct TabBar;

impl TabBar {
    /// The bar's row within the stage, or `None` when the stage is too small for one. Whether
    /// there are tabs is the caller's business (with none there is no stage to label).
    pub fn row(stage: Rect) -> Option<Rect> {
        let fits = stage.height >= MIN_STAGE_HEIGHT && stage.width >= LABEL.width() as u16 + 4;
        fits.then_some(Rect { height: 1, ..stage })
    }

    /// Which tab a point is on: an index into the same list [`Self::render`] draws.
    pub fn hit(&self, state: &AppState, stage: Rect, at: Position) -> Option<usize> {
        let row = Self::row(stage)?;
        if at.y != row.y {
            return None;
        }
        let tabs = state.stage_tabs();
        slots(row, &tabs, state.shown_tab_index())
            .into_iter()
            .find(|(_, area)| area.contains(at))
            .map(|(index, _)| index)
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, state: &AppState) {
        let tabs = state.stage_tabs();
        let shown = state.shown_tab_index();
        frame.render_widget(Block::new().style(Style::new().bg(theme::BG)), area);
        faint(frame, area.x, area.y, LABEL);
        let mut after: Option<u16> = None;
        for (index, slot) in slots(area, &tabs, shown) {
            // The gap between two tabs says they are two; a tab cut off by the edge has none.
            if after.is_some_and(|end| end + SEP.width() as u16 <= slot.x) {
                faint(frame, slot.x - 2, area.y, "│");
            }
            let tab = &tabs[index];
            let text = pad(&label(tab, Some(index) == shown), slot.width as usize);
            frame.render_widget(
                Paragraph::new(Span::styled(text, style(tab, Some(index) == shown))),
                slot,
            );
            after = Some(slot.x + slot.width);
        }
        let hint_at = area.x + area.width.saturating_sub(HINT.width() as u16 + 1);
        if after.is_none_or(|end| end + 2 <= hint_at) {
            faint(frame, hint_at, area.y, HINT);
        }
    }
}

/// One run of faint text, on the row's own line.
fn faint(frame: &mut Frame, x: u16, y: u16, text: &str) {
    frame.buffer_mut().set_string(x, y, text, theme::faint());
}

/// A tab as the bar writes it: a dot when the agent opened it and it hasn't been seen, the
/// title cut to what fits. One space of padding each side, which is what makes it read as a tab.
fn label(tab: &StageTab, shown: bool) -> String {
    let dot = if tab.unseen() && !shown { "•" } else { "" };
    let room = TITLE_MAX.saturating_sub(dot.width());
    format!(" {dot}{} ", fit(&tab.title(), room))
}

fn style(tab: &StageTab, shown: bool) -> Style {
    if shown {
        Style::new()
            .fg(theme::BG)
            .bg(theme::ACCENT)
            .add_modifier(Modifier::BOLD)
    } else if tab.unseen() {
        Style::new().fg(theme::WARN)
    } else {
        theme::muted()
    }
}

/// Where each tab goes, left to right after the label. Tabs there is no room for are left out;
/// when that would leave out the shown one, the row starts there instead — seeing what you are
/// looking at beats seeing the first tab.
fn slots(row: Rect, tabs: &[StageTab], shown: Option<usize>) -> Vec<(usize, Rect)> {
    let width =
        |tab: &StageTab| (tab.title().width().min(TITLE_MAX) + tab.unseen() as usize + 2) as u16;
    let from = |first: usize| {
        let mut out: Vec<(usize, Rect)> = Vec::new();
        let right = row.x + row.width;
        let mut x = row.x + LABEL.width() as u16;
        for (index, tab) in tabs.iter().enumerate().skip(first) {
            let w = width(tab);
            if w < 3 || x + w > right {
                break;
            }
            out.push((
                index,
                Rect {
                    x,
                    y: row.y,
                    width: w,
                    height: 1,
                },
            ));
            x = (x + w + SEP.width() as u16).min(right);
        }
        out
    };
    let all = from(0);
    match shown {
        Some(at) if !all.iter().any(|(index, _)| *index == at) => from(at),
        _ => all,
    }
}
