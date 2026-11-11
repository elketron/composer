//! Scoped mounts (ticket 0122): with an allowed root set, a symlink inside a site is followed
//! for reads, listings, writes and indexing only when it lands inside the web or an allowed
//! root; a mount that lands elsewhere is refused, not followed, with an error that names what
//! to do. Without an allowed list the web stays lenient, as it was before the feature.
//! Decision, recorded here: indexing follows allowed mounts; refused ones contribute nothing.

use std::fs;
use std::path::PathBuf;

use composer_core::{Index, MountMode, Mounts, Query, SiteUrl, Web};

fn url(s: &str) -> SiteUrl {
    SiteUrl::parse(s).unwrap()
}

/// A web with one site, a mount into the allowed root and a mount into an outside root.
/// Returns (web, allowed root, outside root, workspace with `allowed` set).
fn web_with_mounts(
    allowed: Vec<PathBuf>,
) -> (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir, Web) {
    let web = tempfile::tempdir().unwrap();
    let allowed_root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(web.path().join("widget")).unwrap();
    fs::write(web.path().join("widget/index.md"), "# Widget\n").unwrap();
    // A real subfolder (not a mount): it belongs to the watched tree and must not be
    // listed as a mount point, or the watcher would watch the web to itself twice (R2).
    fs::create_dir_all(web.path().join("widget/notes/deep")).unwrap();
    fs::write(web.path().join("widget/notes/deep/page.md"), "# Plain\n").unwrap();
    std::os::unix::fs::symlink(allowed_root.path(), web.path().join("widget/build")).unwrap();
    std::os::unix::fs::symlink(outside.path(), web.path().join("widget/outside")).unwrap();
    let mut allowed = allowed;
    allowed.insert(0, allowed_root.path().to_path_buf());
    let ws = Web::with_allowed(web.path(), allowed);
    (web, allowed_root, outside, ws)
}

fn page(frontmatter_type: &str) -> String {
    format!("---\ntitle: Page\ntype: {frontmatter_type}\n---\n# Page\n")
}

#[test]
fn a_project_mount_is_read_only_in_read_mode() {
    let dir = tempfile::tempdir().unwrap();
    let web = dir.path().join("sites");
    let project = dir.path().join("project");
    let work = web.join("coding/work");
    fs::create_dir_all(&work).unwrap();
    fs::create_dir_all(&project).unwrap();
    let link = work.join("project");
    std::os::unix::fs::symlink(&project, &link).unwrap();
    let mounts = Mounts::with_allowed(web, vec![dir.path().to_path_buf()]);
    mounts.set_project(link.clone(), project.clone(), MountMode::Read);
    assert!(mounts.check(&link.join("main.rs")).is_ok());
    assert!(mounts.check_write(&link.join("main.rs")).is_err());
    mounts.set_project(link.clone(), project, MountMode::Write);
    assert!(mounts.check_write(&link.join("main.rs")).is_ok());
}

#[test]
fn an_allowed_mount_is_read_written_listed_and_indexed() {
    let (web, allowed_root, _outside, ws) = web_with_mounts(vec![]);
    fs::write(allowed_root.path().join("page.md"), page("mount-allowed")).unwrap();

    let read = ws.read_page(&url("site://widget/build/page")).unwrap();
    assert_eq!(read.url.to_string(), "site://widget/build/page.md");

    let written = ws
        .write_page(
            &url("site://widget/build/note"),
            "# Written through the mount\n",
        )
        .unwrap();
    assert_eq!(written.to_string(), "site://widget/build/note.md");
    assert!(allowed_root.path().join("note.md").is_file());

    // Indexing follows allowed mounts, naming the pages by their address inside the web.
    let index = Index::build(&ws).unwrap();
    let hits: Vec<_> = index
        .search(&Query::parse("type:mount-allowed"))
        .into_iter()
        .map(|h| h.url.to_string())
        .collect();
    assert_eq!(hits, ["site://widget/build/page.md"]);

    // The mount point itself is listed, by its web path.
    assert_eq!(ws.mount_points(), [web.path().join("widget/build")]);
}

#[test]
fn a_mount_outside_the_allowed_scope_is_refused_not_followed() {
    let (web, _allowed_root, outside, ws) = web_with_mounts(vec![]);
    fs::write(outside.path().join("page.md"), page("mount-refused")).unwrap();

    let err = ws
        .read_page(&url("site://widget/outside/page"))
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("symlink that points outside"), "{msg}");
    assert!(msg.contains("config.toml"), "{msg}");

    let err = ws
        .write_page(&url("site://widget/outside/new"), "# Refused\n")
        .unwrap_err();
    assert!(
        err.to_string().contains("symlink that points outside"),
        "{err}"
    );
    assert!(
        !outside.path().join("new.md").exists(),
        "a refused write writes nothing"
    );

    // Refused mounts contribute nothing to listings or the index.
    assert!(
        ws.all_pages()
            .all(|p| ws.url_for_path(&p).unwrap().to_string() != "site://widget/outside/page.md"),
        "a refused mount contributes no pages"
    );
    let index = Index::build(&ws).unwrap();
    assert!(index.search(&Query::parse("type:mount-refused")).is_empty());
    // Only symlinks are mount points: the web's own folders are already on the watched tree.
    assert_eq!(ws.mount_points(), [web.path().join("widget/build")]);
    // And the real subfolder's page is still listed (through the walk, not as a mount).
    assert!(
        ws.all_pages()
            .any(|p| ws.url_for_path(&p).unwrap().to_string() == "site://widget/notes/deep/page.md")
    );
}

#[test]
fn without_an_allowed_list_the_web_stays_lenient() {
    let web = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::create_dir_all(web.path().join("widget")).unwrap();
    std::os::unix::fs::symlink(outside.path(), web.path().join("widget/outside")).unwrap();
    fs::write(outside.path().join("page.md"), "# Outside\n").unwrap();

    let ws = Web::new(web.path());
    assert_eq!(
        ws.read_page(&url("site://widget/outside/page"))
            .unwrap()
            .title(),
        "Outside"
    );
    // Walks do not descend into symlinks without an allowed list — as before the feature.
    assert!(
        !ws.all_pages()
            .filter_map(|p| ws.url_for_path(&p))
            .any(|u| u.to_string() == "site://widget/outside/page.md"),
        "lenient mode must not index through mounts (decision: gating on an allowed list)"
    );
    assert!(ws.mount_points().is_empty());
}

#[test]
fn widening_the_allowed_list_opens_the_mount() {
    let (_web, allowed_root, outside, ws) = web_with_mounts(vec![]);
    fs::write(outside.path().join("page.md"), "# Outside\n").unwrap();

    assert!(!ws.allows(&ws.root().join("widget/outside")));

    // What config reload does when projects.dir changes.
    ws.set_allowed(vec![
        allowed_root.path().to_path_buf(),
        outside.path().to_path_buf(),
    ]);
    assert!(ws.allows(&ws.root().join("widget/outside")));
    assert_eq!(
        ws.read_page(&url("site://widget/outside/page"))
            .unwrap()
            .title(),
        "Outside"
    );
    assert!(
        ws.mount_points()
            .contains(&ws.root().join("widget/outside"))
    );

    // Narrowing it closes it again.
    ws.set_allowed(vec![allowed_root.path().to_path_buf()]);
    assert!(!ws.allows(&ws.root().join("widget/outside")));
}

#[test]
fn a_link_inside_an_allowed_mount_that_escapes_is_refused() {
    let (web, allowed_root, outside, ws) = web_with_mounts(vec![]);
    // widget/build -> allowed_root (fine); allowed_root/escape -> outside (not fine).
    fs::create_dir_all(allowed_root.path().join("escape")).unwrap();
    std::os::unix::fs::symlink(outside.path(), allowed_root.path().join("escape/deeper")).unwrap();
    fs::write(outside.path().join("secret.md"), "# Secret\n").unwrap();

    let err = ws
        .read_page(&url("site://widget/build/escape/deeper/secret"))
        .unwrap_err();
    assert!(
        err.to_string().contains("symlink that points outside"),
        "{err}"
    );

    let err = ws
        .write_page(&url("site://widget/build/escape/deeper/new"), "# x\n")
        .unwrap_err();
    assert!(
        err.to_string().contains("symlink that points outside"),
        "{err}"
    );
    assert!(
        !outside.path().join("new.md").exists(),
        "a refused write writes nothing"
    );

    // A link straight through to an outside file is refused too.
    std::os::unix::fs::symlink(
        outside.path().join("secret.md"),
        web.path().join("widget/leak.md"),
    )
    .unwrap();
    let err = ws.read_page(&url("site://widget/leak")).unwrap_err();
    assert!(
        err.to_string().contains("symlink that points outside"),
        "{err}"
    );

    // Nothing leaked into the listings either.
    let index = Index::build(&ws).unwrap();
    assert!(
        index
            .search(&Query::parse("Secret"))
            .into_iter()
            .all(|h| !h.url.to_string().contains("leak")),
        "a refused link contributes no pages"
    );
}
