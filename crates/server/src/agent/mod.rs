//! Agent definitions (`site://agents/<name>`) and system prompt assembly.

mod agent_definition;
mod prompt;
mod site_access;

pub use agent_definition::AgentDefinition;
pub use prompt::system_prompt;
pub use site_access::SiteAccess;
