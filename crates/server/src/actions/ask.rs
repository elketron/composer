//! One approval to ask for, and its answer.
//!
//! Who may answer is part of the request, set by the constructor the caller picks, never by
//! looking at what is asked: [`Ask::trust`] (a site's `folder:`, its `mcp.toml`, a policy page:
//! what the agent may later do without asking) goes to the user only; [`Ask::action`] and
//! [`Ask::recipe_run`] (one thing the agent wants to do now) may be decided by the `[system1]`
//! model first ([`crate::system1`]).

use composer_protocol::{ApprovalDecision, ApprovalKind, ServerMessage};

/// Who may answer an [`Ask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answerer {
    /// A trust change: only the user, through their clients.
    User,
    /// An action: `[system1]` first when it is on (approve, reject, or forward), else the user.
    System1ThenUser,
}

/// An approval request, before it has an id.
#[derive(Debug, Clone)]
pub struct Ask {
    answerer: Answerer,
    pub kind: ApprovalKind,
    /// A short name for chat lines: the command, the tool, the recipe.
    pub what: String,
    pub title: String,
    pub site: String,
    pub recipe: String,
    pub args: Vec<String>,
    pub body: String,
    pub hash: String,
    /// What `always` would allow (tools only).
    pub rule: Option<String>,
}

impl Ask {
    /// A trust change kept in the recipe approval store under `key` (`folder:`, `mcp.toml`,
    /// `approvals.md`) for `hash`: always the user's to answer.
    pub fn trust(site: &str, key: &str, args: Vec<String>, body: String, hash: &str) -> Self {
        Self {
            answerer: Answerer::User,
            kind: ApprovalKind::Recipe,
            what: format!("{key} of site://{site}"),
            title: String::new(),
            site: site.to_string(),
            recipe: key.to_string(),
            args,
            body,
            hash: hash.to_string(),
            rule: None,
        }
    }

    /// Running this version of a recipe (`body` is its source).
    pub fn recipe_run(site: &str, recipe: &str, args: &[String], body: String, hash: &str) -> Self {
        let mut what = recipe.to_string();
        for arg in args {
            what.push(' ');
            what.push_str(arg);
        }
        Self {
            answerer: Answerer::System1ThenUser,
            kind: ApprovalKind::Recipe,
            what: format!("the recipe `{what}`"),
            title: String::new(),
            site: site.to_string(),
            recipe: recipe.to_string(),
            args: args.to_vec(),
            body,
            hash: hash.to_string(),
            rule: None,
        }
    }

    /// Something the agent wants to do now: a terminal input or background process, an MCP
    /// tool, an outside agent's tool call. `what` names it in chat lines (`` `cargo test` ``).
    pub fn action(
        kind: ApprovalKind,
        what: String,
        title: String,
        body: String,
        rule: Option<String>,
    ) -> Self {
        Self {
            answerer: Answerer::System1ThenUser,
            kind,
            what,
            title,
            site: String::new(),
            recipe: String::new(),
            args: vec![],
            body,
            hash: String::new(),
            rule,
        }
    }

    pub fn answerer(&self) -> Answerer {
        self.answerer
    }

    /// The request as clients get it.
    pub fn message(&self, request: String, session: &str) -> ServerMessage {
        ServerMessage::ApprovalRequest {
            request,
            session: session.to_string(),
            kind: self.kind,
            title: self.title.clone(),
            site: self.site.clone(),
            recipe: self.recipe.clone(),
            args: self.args.clone(),
            body: self.body.clone(),
            hash: self.hash.clone(),
            rule: self.rule.clone(),
        }
    }
}

/// Who answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecidedBy {
    User,
    /// The `[system1]` model, with its reason (it only approves or rejects; never `always`).
    System1 {
        reason: String,
    },
}

/// The answer to an [`Ask`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub decision: ApprovalDecision,
    pub by: DecidedBy,
}

impl Answer {
    pub fn user(decision: ApprovalDecision) -> Self {
        Self {
            decision,
            by: DecidedBy::User,
        }
    }

    /// A yes, once or always.
    pub fn approved(&self) -> bool {
        matches!(
            self.decision,
            ApprovalDecision::Approve | ApprovalDecision::Always
        )
    }

    /// The error the agent gets for a no: system-1's reason when it decided, else `user`.
    pub fn refusal(&self, what: &str, user: impl FnOnce() -> String) -> String {
        match &self.by {
            DecidedBy::System1 { reason } => format!(
                "system-1 rejected {what} on the user's behalf: {reason}. Don't try to get around \
                 it: do without, or ask the user to allow it."
            ),
            DecidedBy::User => user(),
        }
    }
}
