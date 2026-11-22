//! `[system1]`: a small fast model that decides action approvals for the user in one call:
//! approve, reject, or forward to the user (plan site://plans/system-1-approvals).
//!
//! It sees only an [`Ask`] that lets it decide ([`Answerer::System1ThenUser`]: terminal input,
//! background processes, MCP and outside agents' tool calls, recipe runs); trust changes are built
//! with [`Ask::trust`] and never reach it. `always` rules the user gave are checked by the callers
//! before they ask, so they still hold without a model call. It reads the request, the user's own
//! recent messages in the session and the policy pages the user approved ([`policy`]), never tool
//! output or the agent's text, so nothing injected into a file or page can talk it into a yes.
//! Each decision is a line in the session's chat. Off (no model), failing, slow or unclear: the
//! user is asked, as without it. It never answers `always`.
//!
//! It asks for a strict JSON schema answer; an endpoint that refuses that `response_format` is
//! asked once with `json_object`, then once with none, and the form that worked is remembered per
//! base URL and model while the server runs.

mod policy;
mod prompt;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use composer_protocol::{ApprovalDecision, ChatMessage, MessageSource, Role, ServerMessage};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::actions::{Answer, Ask, DecidedBy};
use crate::config::Config;
use crate::config_sections::ProviderConfig;
use crate::provider::{
    CallError, ModelEvent, ModelMessage, ModelRequest, OpenAiProvider, Provider,
};
use crate::server::Server;
use crate::session::Session;

pub use policy::POLICY_FILE;

/// What system-1 keeps between decisions.
#[derive(Debug, Default)]
pub struct System1 {
    /// Policy page versions (site, hash) the user refused since the server started: not asked
    /// about again until they change.
    refused: Mutex<HashSet<(String, String)>>,
    /// The structured-output form each endpoint (base URL, model) took, once one worked.
    formats: Mutex<HashMap<(String, String), Format>>,
}

/// How system-1 asks for its JSON answer, strictest first: an endpoint that refuses one is asked
/// with the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// `json_schema`, strict: the decision is one of three words.
    Schema,
    /// `json_object`: any JSON object.
    JsonObject,
    /// No `response_format`: the instructions and the lenient parser do it.
    Plain,
}

impl Format {
    const ALL: [Format; 3] = [Format::Schema, Format::JsonObject, Format::Plain];

    fn response_format(self) -> Option<Value> {
        match self {
            Format::Schema => Some(response_format()),
            Format::JsonObject => Some(json!({ "type": "json_object" })),
            Format::Plain => None,
        }
    }
}

/// What became of an [`Ask`] system-1 was given.
#[derive(Debug)]
pub enum Ruling {
    /// It approved or rejected.
    Decided(Answer),
    /// The user decides: system-1 is off, forwarded it, or failed.
    User,
    /// The turn was cancelled meanwhile.
    Cancelled,
}

/// What the model answered.
#[derive(Debug, Clone, PartialEq)]
enum Verdict {
    Approve(String),
    Reject(String),
    Forward(String),
}

/// The longest reason kept from the model (or an error), in characters.
const REASON_CHARS: usize = 300;

/// Decides `ask` in `session`, or says the user should. Asks the user first about a policy page
/// version they never answered (a trust change).
pub async fn decide(
    server: &Arc<Server>,
    session: &Session,
    ask: &Ask,
    cancel: &CancellationToken,
) -> Ruling {
    let config = server.config();
    let settings = &config.system1;
    if settings.model.trim().is_empty() {
        return Ruling::User;
    }
    let site = session.info().site;
    let Some(policies) = policy::approved(server, session, site.as_deref(), cancel).await else {
        return Ruling::Cancelled;
    };
    let messages = prompt::messages(server, session, ask, site.as_deref(), &policies);
    let timeout = Duration::from_secs(settings.timeout_secs.max(1));
    let answer = tokio::select! {
        answer = tokio::time::timeout(timeout, complete(server, &config, messages)) => answer,
        _ = cancel.cancelled() => return Ruling::Cancelled,
    };
    let verdict = match answer {
        Err(_) => Verdict::Forward(format!("it didn't answer within {}s", timeout.as_secs())),
        Ok(Err(e)) => {
            tracing::warn!("system1 ({}): {e}; the user decides", settings.model);
            Verdict::Forward(shortened(&format!("it failed: {e}")))
        }
        Ok(Ok(text)) => verdict(&text).unwrap_or_else(|| {
            tracing::warn!(
                "system1 ({}) answered {text:?}; the user decides",
                settings.model
            );
            Verdict::Forward("its answer wasn't a decision".into())
        }),
    };
    line(server, session, &ask.what, &verdict);
    let decided = |decision, reason| {
        Ruling::Decided(Answer {
            decision,
            by: DecidedBy::System1 { reason },
        })
    };
    match verdict {
        Verdict::Approve(reason) => decided(ApprovalDecision::Approve, reason),
        Verdict::Reject(reason) => decided(ApprovalDecision::Deny, reason),
        Verdict::Forward(_) => Ruling::User,
    }
}

impl System1 {
    fn refused(&self, site: &str, hash: &str) -> bool {
        self.refused
            .lock()
            .expect("system1 lock")
            .contains(&(site.to_string(), hash.to_string()))
    }

    fn format(&self, endpoint: &(String, String)) -> Option<Format> {
        self.formats
            .lock()
            .expect("system1 lock")
            .get(endpoint)
            .copied()
    }

    fn remember_format(&self, endpoint: (String, String), format: Format) {
        self.formats
            .lock()
            .expect("system1 lock")
            .insert(endpoint, format);
    }

    fn refuse(&self, site: &str, hash: &str) {
        self.refused
            .lock()
            .expect("system1 lock")
            .insert((site.to_string(), hash.to_string()));
    }
}

/// One call to the `[system1]` model, its whole answer as text. Its own endpoint, else the
/// provider's (sharing its slots, and its key unless one is set).
async fn complete(
    server: &Server,
    config: &Config,
    messages: Vec<ModelMessage>,
) -> Result<String, String> {
    let settings = &config.system1;
    let own_endpoint = !settings.base_url.trim().is_empty();
    let base_url = if own_endpoint {
        settings.base_url.clone()
    } else {
        config.provider.base_url.clone()
    };
    let model = settings.model.trim().to_string();
    let provider = OpenAiProvider::connect(&ProviderConfig {
        base_url: base_url.clone(),
        api_key: if own_endpoint || !settings.api_key.is_empty() {
            settings.api_key.clone()
        } else {
            config.provider.api_key.clone()
        },
        model: model.clone(),
        tool_images: false,
    })
    .await?;
    let request = |format: Format| ModelRequest {
        messages: messages.clone(),
        tools: vec![],
        model: None,
        reasoning_effort: Some(settings.reasoning_effort.trim().to_string())
            .filter(|e| !e.is_empty()),
        response_format: format.response_format(),
    };
    let _slot = match own_endpoint {
        true => None,
        false => Some(server.slots.acquire().await?),
    };
    // The form that worked here before, else the strictest; a refused form tries the next once.
    let endpoint = (base_url, model);
    let first = server.system1.format(&endpoint).unwrap_or(Format::Schema);
    let mut forms = Format::ALL
        .into_iter()
        .skip_while(|f| *f != first)
        .peekable();
    let mut stream = loop {
        let format = forms.next().expect("one form at least");
        match provider.stream(request(format)).await {
            Ok(stream) => {
                server.system1.remember_format(endpoint, format);
                break stream;
            }
            Err(CallError::Refused(e)) if forms.peek().is_some() && refuses_format(&e) => {
                tracing::info!(
                    "system1 ({}): {format:?} refused ({e}); trying the next",
                    endpoint.1
                );
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let mut text = String::new();
    while let Some(event) = stream.recv().await {
        match event? {
            ModelEvent::Content(part) => text.push_str(&part),
            ModelEvent::Finished { .. } => break,
            ModelEvent::Thinking(_) => {}
        }
    }
    Ok(text)
}

/// Whether a refused call is the endpoint not taking the `response_format`: a 400 or 422, or an
/// error that names it.
fn refuses_format(error: &str) -> bool {
    let lower = error.to_lowercase();
    error.starts_with("400")
        || error.starts_with("422")
        || lower.contains("response_format")
        || lower.contains("json_schema")
}

/// Structured output: one object with the decision and a reason.
fn response_format() -> Value {
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "approval_decision",
            "strict": true,
            "schema": {
                "type": "object",
                "properties": {
                    "decision": { "type": "string", "enum": ["approve", "reject", "forward"] },
                    "reason": { "type": "string" }
                },
                "required": ["decision", "reason"],
                "additionalProperties": false
            }
        }
    })
}

/// `{"decision": …, "reason": …}`, possibly inside other text or a code fence.
fn verdict(answer: &str) -> Option<Verdict> {
    let json: Value =
        serde_json::from_str(answer.get(answer.find('{')?..=answer.rfind('}')?)?).ok()?;
    let reason = shortened(json["reason"].as_str().unwrap_or_default());
    match json["decision"].as_str()?.trim() {
        "approve" => Some(Verdict::Approve(reason)),
        "reject" => Some(Verdict::Reject(reason)),
        "forward" => Some(Verdict::Forward(reason)),
        _ => None,
    }
}

/// One line, at most [`REASON_CHARS`] characters.
fn shortened(text: &str) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(REASON_CHARS) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line,
    }
}

/// The decision, as a line in the session's chat (the model never reads it).
fn line(server: &Server, session: &Session, what: &str, verdict: &Verdict) {
    let (said, reason) = match verdict {
        Verdict::Approve(reason) => (format!("approved {what}"), reason),
        Verdict::Reject(reason) => (format!("rejected {what}"), reason),
        Verdict::Forward(reason) => (format!("forwarded {what} to you"), reason),
    };
    let content = match reason.is_empty() {
        true => format!("system-1 {said}"),
        false => format!("system-1 {said} — {reason}"),
    };
    session.emit(&server.hub, |seq| ServerMessage::MessageDone {
        session: session.id().to_string(),
        seq,
        message: ChatMessage {
            role: Role::System,
            content,
            thinking: None,
            tool_calls: vec![],
            pages: vec![],
            images: vec![],
            attachments: vec![],
            source: Some(MessageSource::System1),
        },
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verdicts_from_what_models_write() {
        assert_eq!(
            verdict(r#"{"decision": "approve", "reason": "runs its tests"}"#),
            Some(Verdict::Approve("runs its tests".into()))
        );
        assert_eq!(
            verdict("```json\n{\"decision\":\"reject\",\"reason\":\"pushes\"}\n```"),
            Some(Verdict::Reject("pushes".into()))
        );
        assert_eq!(
            verdict(r#"Sure. {"decision": "forward"}"#),
            Some(Verdict::Forward(String::new()))
        );
        assert_eq!(verdict(r#"{"decision": "always"}"#), None);
        assert_eq!(verdict(r#"{"decision": "maybe"}"#), None);
        assert_eq!(verdict("approve"), None);
        assert_eq!(verdict("} {"), None);
    }

    #[test]
    fn reasons_are_one_short_line() {
        assert_eq!(shortened(" a\n b  c "), "a b c");
        assert_eq!(
            shortened(&"x".repeat(400)).chars().count(),
            REASON_CHARS + 1
        );
    }
}
