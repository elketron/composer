//! The library as a git repo (ticket 0151): `init` starts it with the stock, `update` commits hand
//! edits and new stock apart, and an old library folder becomes a repo on the next update.

use std::path::Path;
use std::process::Command;

use composer_server::library::{Library, SiteDefinition};
use composer_server::{Home, init_home, update_home};

fn log(home: &Home) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(home.library())
        .args(["log", "--format=%s"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn init_starts_the_library_with_the_stock() {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    let library = Library::of(&home);
    assert_eq!(log(&home).len(), 1);
    assert!(log(&home)[0].starts_with("stock: composer "));
    let sites: Vec<String> = library
        .sites()
        .unwrap()
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    for stock in ["agents", "home", "memory", "orchestration", "plans"] {
        assert!(sites.contains(&stock.to_string()), "{sites:?}");
    }
    // Each stock site reads back from the commit, and is all definition.
    let head = library.head().unwrap().unwrap();
    let index = library
        .read_at(&head, "home", Path::new("index.md"))
        .unwrap()
        .unwrap();
    assert_eq!(
        index,
        std::fs::read(home.library().join("home/index.md")).unwrap()
    );
    // A second init changes nothing.
    init_home(&home).await.unwrap();
    assert_eq!(log(&home).len(), 1);
}

#[tokio::test]
async fn update_commits_hand_edits_apart_and_nothing_when_nothing_changed() {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    update_home(&home, false).await.unwrap();
    assert_eq!(log(&home).len(), 1, "the same stock: no commit");

    let page = home.library().join("home/index.md");
    let text = std::fs::read_to_string(&page).unwrap();
    std::fs::write(&page, format!("{text}\nA line of the user's.\n")).unwrap();
    update_home(&home, true).await.unwrap();
    assert_eq!(log(&home).len(), 1, "a dry run writes nothing");
    update_home(&home, false).await.unwrap();
    assert_eq!(log(&home)[0], "library: changes made before this update");
    assert!(
        std::fs::read_to_string(&page)
            .unwrap()
            .contains("A line of the user's.")
    );
}

#[tokio::test]
async fn a_library_from_before_repos_becomes_one_on_update() {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    std::fs::remove_dir_all(home.library().join(".git")).unwrap();
    std::fs::remove_file(home.library().join(".gitignore")).unwrap();
    // Nor did its workspace know the library.
    let workspace = home.workspace(&composer_server::WorkspaceName::default_workspace());
    std::fs::remove_file(workspace.library_file()).unwrap();
    update_home(&home, false).await.unwrap();
    // Its stock is current, so the stock commit only marks it: workspaces join there.
    assert_eq!(
        log(&home),
        [
            log(&home)[0].clone(),
            "library: as it was before it became a git repo".to_string()
        ]
    );
    assert!(log(&home)[0].starts_with("stock: composer "));
    assert!(workspace.library_file().is_file(), "the workspace joined");
}

#[tokio::test]
async fn stock_sites_are_all_definition() {
    let dir = tempfile::tempdir().unwrap();
    let home = Home::new(dir.path());
    init_home(&home).await.unwrap();
    for (site, _) in Library::of(&home).sites().unwrap() {
        let definition = SiteDefinition::read(&home.library().join(&site)).unwrap();
        assert!(
            definition
                .files()
                .iter()
                .any(|f| f == Path::new("index.md")),
            "{site}"
        );
        assert!(
            definition.missing().is_empty(),
            "{site}: {:?}",
            definition.missing()
        );
    }
}
