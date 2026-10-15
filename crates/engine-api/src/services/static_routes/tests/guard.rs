use super::{TempDir, server};

#[test]
fn resolve_accepts_plain_children() {
    let dir = TempDir::new("resolve");
    let s = server(dir.path());
    assert_eq!(s.resolve("/tank-3d.html"), Some(dir.path().join("tank-3d.html")));
    assert_eq!(
        s.resolve("/assets/player.png"),
        Some(dir.path().join("assets").join("player.png"))
    );
    // A duplicated or dot segment stays inside the root.
    assert_eq!(s.resolve("//assets//a.wasm"), s.resolve("/assets/a.wasm"));
    assert_eq!(s.resolve("/./assets/a.json"), s.resolve("/assets/a.json"));
}

#[test]
fn resolve_rejects_every_way_out_of_the_root() {
    let dir = TempDir::new("escape");
    let s = server(dir.path());
    for probe in [
        "/../etc/passwd",
        "/a/../../etc/passwd",
        "/%2e%2e/etc/passwd",
        "/.%2e/.%2e/%2e%2e/etc/passwd",
        r"/..\..\etc\passwd",
        r"/a/..\b",
        "/%5c%5cetc/passwd",
        "/a%00b",
        "/a\0b",
    ] {
        assert_eq!(s.resolve(probe), None, "{probe:?} escaped the root");
    }
    // Decoding is single-pass, so `%252e%252e` becomes the literal name
    // `%2e%2e` rather than a parent — still a child of the root, and still
    // not a file anybody shipped.
    let once = s.resolve("/%252e%252e/x").expect("a decoded name is not a traversal");
    assert!(once.starts_with(dir.path()));
}
