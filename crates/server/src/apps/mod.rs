//! Apps registered to projects (plan phase 4): a site's `app:` manifest in its `index.md`, the
//! facts of a project its `when` is matched against, and the apps a project (or the workspace's
//! Home) gets: matched, pinned or hidden by the project's `[projects."<path>"]` entry.

pub(crate) mod app_build;
mod app_condition;
mod app_grant;
mod app_manifest;
mod app_open;
mod app_rail;
mod app_rebuild;
mod app_scope;
mod app_tokens;
mod attach_reason;
mod attached_app;
mod file_glob;
mod granted_project;
mod local_allowance;
mod local_allowances;
mod local_app;
mod local_commands;
mod once_codes;
mod open_projects;
mod opened_project;
pub(crate) mod project_facts;
mod project_name;
mod project_watch;
mod workspace_apps;

pub use app_build::{SDK_VERSION, built_with, outdated};
pub use app_condition::AppCondition;
pub use app_grant::AppGrant;
pub use app_manifest::AppManifest;
pub use app_open::{open_app, site_manifest};
pub use app_rail::AppRail;
pub use app_rebuild::{rebuild, rebuild_plan};
pub use app_scope::AppScope;
pub use app_tokens::AppTokens;
pub use attach_reason::AttachReason;
pub use attached_app::AttachedApp;
pub use file_glob::FileGlob;
pub use granted_project::GrantedProject;
pub use local_allowances::LocalAllowances;
pub use local_app::LocalApp;
pub use local_commands::{allow_local, promote_local};
pub use once_codes::OnceCodes;
pub use open_projects::OpenProjects;
pub use project_facts::ProjectFacts;
pub use project_name::ProjectName;
pub use project_watch::ProjectWatch;
pub use workspace_apps::{
    attached_apps, find_project, find_project_by, home_apps, project_apps, project_info,
    workspace_apps, workspace_projects,
};
