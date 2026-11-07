//! The desktop talks to a server, never runs one (plan phase 7): it depends on the client crate
//! and the protocol, not on composer-server (nor composer-core, the server's side of pages).

#[test]
fn the_desktop_never_links_the_server() {
    let manifest = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"))
        .unwrap()
        .parse::<toml::Table>()
        .unwrap();
    let dependencies = manifest["dependencies"].as_table().unwrap();
    for forbidden in ["composer-server", "composer-core", "composer-tui"] {
        assert!(
            !dependencies.contains_key(forbidden),
            "the desktop depends on {forbidden}"
        );
    }
    assert!(dependencies.contains_key("composer-client"));
}
