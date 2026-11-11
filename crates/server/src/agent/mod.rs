//! Agent definitions (`site://agents/<name>`) and system prompt assembly.

mod agent_definition;
mod app_launch;
mod entered;
mod prompt;
mod site;
mod site_access;
mod site_switch;

pub use agent_definition::AgentDefinition;
pub use app_launch::AppLaunch;
pub use entered::Entered;
pub use prompt::system_prompt;
pub use site::Site;
pub use site_access::SiteAccess;
pub use site_switch::{enter_site_as_agent, enter_site_as_client, leave_site};
