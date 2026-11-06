//! The library (`<home>/sites/`): site definitions shared by every workspace, kept as a local git
//! repo so a workspace's installed copy can be merged against the commit it came from (plan
//! phase 2). Library sites hold no pages or app state: only their [`SiteDefinition`].

mod library_commit;
mod library_repo;
mod site_definition;

pub use library_commit::LibraryCommit;
pub use library_repo::Library;
pub use site_definition::SiteDefinition;
