use std::sync::Arc;
use std::time::{Duration, SystemTime};

use super::{TempDir, server};

/// Set both times so a rewrite lands on a stamp the cache has not seen;
/// `atime` is irrelevant to the cache, `mtime` is the whole check.
fn filetime(path: &std::path::Path, when: SystemTime) {
    let ft = std::fs::FileTimes::new().set_accessed(when).set_modified(when);
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|f| f.set_times(ft))
        .expect("set file times");
}

#[test]
fn reads_a_file_then_notices_a_rewrite() {
    let dir = TempDir::new("read");
    let s = server(dir.path());
    let file = dir.path().join("page.html");

    assert_eq!(s.read(&file), None, "a missing file is not a cached empty body");

    std::fs::write(&file, b"<html>first</html>").unwrap();
    assert_eq!(&*s.read(&file).unwrap(), b"<html>first</html>");
    // Second read is the cache hit: same Arc, no re-read.
    let hit = s.read(&file).unwrap();
    assert!(Arc::ptr_eq(&hit, &s.read(&file).unwrap()));

    // Same mtime second is exactly the case a 1s-granularity filesystem
    // hides, so force the stamp to move before comparing.
    let next = SystemTime::now() + Duration::from_secs(2);
    std::fs::write(&file, b"<html>second, longer</html>").unwrap();
    filetime(&file, next);
    assert_eq!(&*s.read(&file).unwrap(), b"<html>second, longer</html>");

    // Third write, with its mtime pinned back to what the cache already holds: only the
    // length disagrees, which is the case a rebuild inside one clock tick hides.
    std::fs::write(&file, b"<html>third, same!</html>").unwrap();
    filetime(&file, next);
    assert_eq!(&*s.read(&file).unwrap(), b"<html>third, same!</html>");

    std::fs::remove_file(&file).unwrap();
    assert_eq!(s.read(&file), None);
}

#[test]
fn game_web_is_not_embedded_in_server() {
    let dir = TempDir::new("defaults");
    let s = server(dir.path());
    assert!(s.default_for("/website.html").is_none());
    assert!(s.default_for("/openoxide.js").is_none());
    assert!(s.default_for("/no-such-default").is_none());
}
