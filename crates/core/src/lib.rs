//! The workspace model: sites, `site://` URLs, pages, frontmatter, links and the page index.

mod error;
mod frontmatter;
mod index;
mod link;
mod page;
mod query;
mod site_url;
mod types;
mod workspace;

pub use error::{CoreError, Result};
pub use frontmatter::Frontmatter;
pub use index::Index;
pub use link::{Link, LinkTarget, extract_links};
pub use page::Page;
pub use query::Query;
pub use site_url::SiteUrl;
pub use types::{IndexEntry, SearchHit, SiteInfo};
pub use workspace::Workspace;
