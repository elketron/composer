use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEvent, KeyEventKind, MouseEventKind};
use futures::StreamExt;
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use ratatui::{DefaultTerminal, Terminal};
use tokio::sync::mpsc;

use crate::action::{Action, KeyOutcome};
use crate::components::{Component, Root};
use crate::dispatcher::Dispatcher;
use crate::editor;
use crate::effect::Effect;
use crate::effect_runner::EffectRunner;
use crate::keymap::Keymap;
use crate::options::TuiOptions;
use crate::store::AppState;
use crate::store::types::{FlashLevel, Mode};

/// The shortest time between two draws (60 a second).
const FRAME: Duration = Duration::from_millis(16);

/// The application: stores, components, keymap and the effect runner.
pub struct App {
    pub state: AppState,
    pub root: Root,
    keymap: Keymap,
    dispatcher: Dispatcher,
    actions: mpsc::UnboundedReceiver<Action>,
    runner: EffectRunner,
    /// The system clipboard, once something was copied (it serves the text while it lives).
    clipboard: Option<arboard::Clipboard>,
}

impl App {
    /// Must be called inside a tokio runtime: starts the server connection.
    pub fn new(options: &TuiOptions) -> Self {
        let (dispatcher, actions) = Dispatcher::new();
        let keymap = Keymap::defaults();
        let mut state = AppState::new(options.startup.clone(), options.context_limit);
        state.conn.server_url = options.server_url.trim_end_matches('/').to_string();
        state.conn.token = options.token.clone();
        Self {
            state,
            root: Root::new(keymap.help()),
            keymap,
            runner: EffectRunner::new(&options.server_url, &options.token, dispatcher.clone()),
            dispatcher,
            actions,
            clipboard: None,
        }
    }

    /// Turns a key into actions: overlays and non-Normal modes go straight to their
    /// component; in Normal mode the keymap goes first.
    pub fn handle_key(&mut self, key: KeyEvent) {
        let normal = self.state.mode() == Mode::Normal
            && !self.state.ui.picker_open
            && !self.state.ui.help_open
            && self.state.approvals.current().is_none();
        let actions = if normal {
            match self.keymap.feed(key) {
                KeyOutcome::Action(action) => vec![action],
                KeyOutcome::Pending => vec![],
                KeyOutcome::Unbound => self.root.handle_key(key, &self.state),
                KeyOutcome::Replay(keys) => keys
                    .into_iter()
                    .flat_map(|k| self.root.handle_key(k, &self.state))
                    .collect(),
            }
        } else {
            self.root.handle_key(key, &self.state)
        };
        for action in actions {
            self.dispatcher.dispatch(action);
        }
    }

    /// A terminal event; whether it needs a repaint.
    fn handle_event(&mut self, event: Event, screen: Rect) -> bool {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                self.root.selection = None;
                self.handle_key(key);
                // Keys can change local view state without an action (typing in the
                // composer, scrolling), so every key repaints.
                true
            }
            Event::Paste(text) => {
                self.handle_paste(&text);
                true
            }
            // The mouse moving without a button: nothing to show.
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Moved => false,
            Event::Mouse(mouse) => {
                for action in self.root.handle_mouse(mouse, screen, &self.state) {
                    self.dispatcher.dispatch(action);
                }
                true
            }
            Event::Resize(..) => {
                self.root.selection = None;
                true
            }
            _ => false,
        }
    }

    /// Pasted text (bracketed paste).
    pub fn handle_paste(&mut self, text: &str) {
        for action in self.root.handle_paste(text, &self.state) {
            self.dispatcher.dispatch(action);
        }
    }

    /// Reduces one action. Runs effects that don't need the terminal and returns the rest.
    pub fn process(&mut self, action: Action) -> Vec<Effect> {
        let before = self.state.page.current.as_ref().map(|p| p.url.clone());
        let effects = self.state.reduce(action);
        if self.state.page.current.as_ref().map(|p| &p.url) != before.as_ref() {
            self.root.page.page_changed();
        }
        let mut deferred = Vec::new();
        for effect in effects {
            match effect {
                Effect::EditPage(_)
                | Effect::ViewText { .. }
                | Effect::CopyText(_)
                | Effect::OpenBrowser(_)
                | Effect::OpenFile { .. }
                | Effect::Quit => deferred.push(effect),
                other => self.runner.run(other),
            }
        }
        deferred
    }

    /// Processes actions as they arrive for up to `duration`. Used by scripts and tests,
    /// where there is no terminal: editing runs only with an explicit, non-interactive `$EDITOR`.
    pub async fn pump(&mut self, duration: Duration) {
        let deadline = tokio::time::Instant::now() + duration;
        while let Ok(Some(action)) = tokio::time::timeout_at(deadline, self.actions.recv()).await {
            for effect in self.process(action) {
                if let Effect::OpenFile { path, line } = &effect {
                    if std::env::var_os("EDITOR").is_some() {
                        let result = editor::open_file(path, *line);
                        self.dispatcher.dispatch(result);
                    }
                    continue;
                }
                if let Effect::OpenBrowser(url) = &effect {
                    self.dispatcher
                        .dispatch(Action::Flash(format!("would open {url}"), FlashLevel::Info));
                    continue;
                }
                if let Effect::ViewText { name, text } = &effect {
                    if std::env::var_os("EDITOR").is_some() {
                        let result = editor::view_text(name, text);
                        self.dispatcher.dispatch(result);
                    }
                    continue;
                }
                if let Effect::EditPage(url) = effect {
                    let result = if std::env::var_os("EDITOR").is_some() {
                        editor::edit_page(self.runner.api(), &url).await
                    } else {
                        Action::Flash(
                            "set $EDITOR to edit without a terminal".into(),
                            FlashLevel::Warn,
                        )
                    };
                    self.dispatcher.dispatch(result);
                }
            }
        }
    }

    pub fn draw<B: Backend>(&self, terminal: &mut Terminal<B>) -> anyhow::Result<()> {
        terminal
            .draw(|frame| self.root.render(frame, frame.area(), &self.state))
            .map_err(|e| anyhow::anyhow!("draw: {e:?}"))?;
        Ok(())
    }

    /// The interactive loop.
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> anyhow::Result<()> {
        let mut events = EventStream::new();
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut ticks = 0u64;
        let mut dirty = true;
        // At most a draw per frame: a mouse drag sends an event per cell, and each draw renders
        // the whole chat; those in one frame share a draw.
        let mut drawn = tokio::time::Instant::now() - FRAME;
        loop {
            if dirty && drawn.elapsed() >= FRAME {
                self.draw(&mut terminal)?;
                drawn = tokio::time::Instant::now();
                dirty = false;
            }
            tokio::select! {
                _ = tokio::time::sleep_until(drawn + FRAME), if dirty => {}
                event = events.next() => match event {
                    Some(Ok(event)) => {
                        let size = terminal.size()?;
                        let screen = Rect::new(0, 0, size.width, size.height);
                        dirty |= self.handle_event(event, screen);
                    }
                    Some(Err(e)) => return Err(e.into()),
                    None => return Ok(()),
                },
                Some(action) = self.actions.recv() => {
                    for effect in self.process(action) {
                        match effect {
                            Effect::Quit => return Ok(()),
                            Effect::OpenBrowser(url) => self.dispatcher.dispatch(editor::open_browser(&url)),
                            Effect::CopyText(text) => crate::terminal::copy(&text, &mut self.clipboard),
                            Effect::OpenFile { path, line } => {
                                drop(events);
                                crate::terminal::restore();
                                let result = editor::open_file(&path, line);
                                terminal = crate::terminal::init();
                                terminal.clear()?;
                                events = EventStream::new();
                                self.dispatcher.dispatch(result);
                            }
                            Effect::ViewText { name, text } => {
                                drop(events);
                                crate::terminal::restore();
                                let result = editor::view_text(&name, &text);
                                terminal = crate::terminal::init();
                                terminal.clear()?;
                                events = EventStream::new();
                                self.dispatcher.dispatch(result);
                            }
                            Effect::EditPage(url) => {
                                // Stop reading the terminal so nvim gets every key.
                                drop(events);
                                crate::terminal::restore();
                                let result = editor::edit_page(self.runner.api(), &url).await;
                                terminal = crate::terminal::init();
                                terminal.clear()?;
                                events = EventStream::new();
                                self.dispatcher.dispatch(result);
                            }
                            _ => {}
                        }
                    }
                    dirty = true;
                }
                _ = tick.tick() => {
                    ticks += 1;
                    dirty |= self.state.ui.flash.is_some();
                    // The project changes outside our view too (you, in nvim): keep the changes view live.
                    if self.state.project.open && ticks.is_multiple_of(2) {
                        self.dispatcher.dispatch(Action::RefreshChanges);
                    }
                }
            }
        }
    }
}
