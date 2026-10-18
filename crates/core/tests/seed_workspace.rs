//! Runs the core against a copy of the repo's seed sites.

use std::fs;
use std::path::Path;

use composer_core::{Index, Query, SiteUrl, Workspace};

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}

fn seed() -> (tempfile::TempDir, Workspace) {
    let dir = tempfile::tempdir().unwrap();
    let seed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../seed/sites");
    copy_dir(&seed, dir.path());
    let ws = Workspace::new(dir.path());
    (dir, ws)
}

fn url(s: &str) -> SiteUrl {
    SiteUrl::parse(s).unwrap()
}

#[test]
fn lists_sites() {
    let (_dir, ws) = seed();
    let sites = ws.sites().unwrap();
    let names: Vec<_> = sites.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names,
        ["agents", "chat", "home", "memory", "orchestration", "plans"]
    );
    let orch = sites.iter().find(|s| s.name == "orchestration").unwrap();
    assert!(orch.has_justfile && orch.has_index && orch.has_ui);
    assert_eq!(orch.title, "Orchestration");
}

#[test]
fn reads_pages_by_any_address() {
    let (_dir, ws) = seed();
    let home = ws.read_page(&url("site://home")).unwrap();
    assert_eq!(home.url.to_string(), "site://home/index.md");
    assert_eq!(home.title(), "Home");

    let user = ws.read_page(&url("site://memory/user")).unwrap();
    assert_eq!(user.url.to_string(), "site://memory/user.md");
    assert!(user.frontmatter.matches("kind", "user"));

    assert!(ws.read_page(&url("site://memory/nope")).is_err());
    assert!(ws.read_page(&url("site://nosite")).is_err());
}

#[test]
fn writes_new_pages_and_overwrites_existing() {
    let (dir, ws) = seed();
    let written = ws
        .write_page(
            &url("site://memory/projects/composer"),
            "---\ntitle: composer\n---\nhi\n",
        )
        .unwrap();
    assert_eq!(written.to_string(), "site://memory/projects/composer.md");
    assert!(dir.path().join("memory/projects/composer.md").is_file());

    let written = ws
        .write_page(&url("site://memory"), "# Memory v2\n")
        .unwrap();
    assert_eq!(written.to_string(), "site://memory/index.md");
    assert_eq!(
        ws.read_page(&url("site://memory")).unwrap().title(),
        "Memory v2"
    );
}

#[test]
fn index_search_and_backlinks() {
    let (_dir, ws) = seed();
    let index = Index::build(&ws).unwrap();
    assert!(index.len() >= 10, "indexed {} pages", index.len());

    let agents: Vec<_> = index
        .search(&Query::parse("type:agent"))
        .into_iter()
        .map(|h| h.url.to_string())
        .collect();
    assert_eq!(
        agents,
        [
            "site://agents/default.md",
            "site://agents/orchestrator.md",
            "site://agents/worker.md"
        ]
    );

    let hits = index.search(&Query::parse("site:memory feedback"));
    assert_eq!(hits[0].url.to_string(), "site://memory/index.md");

    let backlinks: Vec<_> = index
        .backlinks(&url("site://memory"))
        .into_iter()
        .map(|u| u.to_string())
        .collect();
    assert!(
        backlinks.contains(&"site://home/index.md".to_string()),
        "{backlinks:?}"
    );

    // the orchestrator's ticket template is a page too, but the ui folder would not be
    assert!(
        index
            .get(&url("site://orchestration/templates/ticket.md"))
            .is_some()
    );
}

#[test]
fn seed_pages_have_no_broken_links() {
    let (_dir, ws) = seed();
    let index = Index::build(&ws).unwrap();
    for entry in index.entries() {
        let broken = index.broken_links(&entry.url);
        // example links inside code blocks are not parsed as links, so anything here is real
        assert!(
            broken.is_empty(),
            "{} links to missing pages: {broken:?}",
            entry.url
        );
    }
}

#[test]
fn skips_ui_and_hidden_folders() {
    let (dir, ws) = seed();
    fs::create_dir_all(dir.path().join("design/ui/node_modules/x")).unwrap();
    fs::write(dir.path().join("design/ui/README.md"), "# ui").unwrap();
    fs::write(
        dir.path().join("design/ui/node_modules/x/README.md"),
        "# dep",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("design/.cache")).unwrap();
    fs::write(dir.path().join("design/.cache/x.md"), "# cache").unwrap();
    fs::write(dir.path().join("design/index.md"), "# Design").unwrap();
    let index = Index::build(&ws).unwrap();
    let design: Vec<_> = index
        .search(&Query::parse("site:design"))
        .into_iter()
        .map(|h| h.url.to_string())
        .collect();
    assert_eq!(design, ["site://design/index.md"]);
    assert!(
        ws.sites()
            .unwrap()
            .iter()
            .any(|s| s.name == "design" && s.has_ui)
    );
}
