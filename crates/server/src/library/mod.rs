//! The library (`<home>/sites/`): site definitions shared by every workspace, kept as a local git
//! repo so a workspace's installed copy can be merged against the commit it came from (plan
//! phase 2). Library sites hold no pages or app state: only their [`SiteDefinition`].

mod installed;
mod installed_sites;
mod library_commit;
mod library_entry;
mod library_repo;
mod library_site_state;
mod publish_change;
mod publish_plan;
mod site_definition;
mod site_name;
mod workspace_library;

pub use installed::Installed;
pub use installed_sites::InstalledSites;
pub use library_commit::LibraryCommit;
pub use library_entry::LibraryEntry;
pub use library_repo::Library;
pub use library_site_state::{Installation, LibrarySiteState};
pub use publish_change::PublishChange;
pub use publish_plan::PublishPlan;
pub use site_definition::SiteDefinition;
pub use site_name::SiteName;
pub use workspace_library::WorkspaceLibrary;
