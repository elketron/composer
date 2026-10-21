//! Pipelines: sites of staged, reviewable work. A pipeline (`site://coding`) holds its stage
//! contracts (`stages/`) and shared rules (`_config/`); each piece of work is a folder in it.

mod create;
#[allow(clippy::module_inception)]
mod pipeline;
mod stage_contract;
pub mod types;
mod work;

pub use create::{NewWork, new_pipeline, new_work, pipeline, pipelines, templates, work_summary};
pub use pipeline::Pipeline;
pub use stage_contract::StageContract;
pub use work::Work;
