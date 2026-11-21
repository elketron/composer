//! Run settings, sites and system prompt assembly.

mod entered;
mod folder_gate;
mod folder_note;
mod prompt;
mod run_settings;
mod site;
mod site_set;
mod site_switch;

pub use entered::Entered;
pub use folder_gate::ensure_folder_approved;
pub use folder_note::FolderNote;
pub use prompt::{folder_instructions, single_shot_prompt, system_prompt};
pub use run_settings::RunSettings;
pub use site::{FOLDER_APPROVAL, Site, folder_hash};
pub use site_set::SiteSet;
pub use site_switch::{enter_site, set_site};
