use std::fs;
use std::path::Path;

const MAX_LINES: usize = 200;

#[test]
fn service_modules_stay_within_the_line_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services");
    let mut oversized = Vec::new();
    collect(&root, &mut oversized);
    assert!(
        oversized.is_empty(),
        "split these service files into modules (> {MAX_LINES} lines): {oversized:#?}"
    );
}

fn collect(dir: &Path, oversized: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("services directory is readable") {
        let path = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            collect(&path, oversized);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let lines = fs::read_to_string(&path)
            .expect("service source file is readable")
            .lines()
            .count();
        if lines > MAX_LINES {
            oversized.push(format!("{} ({lines})", path.display()));
        }
    }
}
