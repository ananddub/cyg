//! Service modules stay small enough to read in one sitting.
//!
//! The budget is 200 lines. Eleven files predate it and are listed in
//! `service_file_size_baseline.txt` with the size they were at when the guard
//! landed; splitting them is worth doing, but not in the change that turns the
//! guard on. The baseline ratchets one way: a listed file may shrink (and then
//! the entry must be lowered or removed by hand), and no file may grow past
//! what the baseline allows.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MAX_LINES: usize = 200;

fn baseline() -> BTreeMap<String, usize> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/service_file_size_baseline.txt");
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("{} is readable: {err}", path.display()))
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (name, count) = line
                .rsplit_once(" (")
                .unwrap_or_else(|| panic!("baseline line is `path (lines)`: {line}"));
            (
                name.trim().to_string(),
                count
                    .trim_end_matches(')')
                    .parse()
                    .unwrap_or_else(|_| panic!("baseline line carries a number: {line}")),
            )
        })
        .collect()
}

fn collect(dir: &Path, sizes: &mut BTreeMap<String, usize>) {
    for entry in fs::read_dir(dir).expect("services directory is readable") {
        let path: PathBuf = entry.expect("directory entry is readable").path();
        if path.is_dir() {
            collect(&path, sizes);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let lines = fs::read_to_string(&path)
            .expect("service source file is readable")
            .lines()
            .count();
        let name = path
            .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")))
            .expect("the file lives under the crate")
            .to_string_lossy()
            .replace('\\', "/");
        sizes.insert(name, lines);
    }
}

#[test]
fn service_modules_stay_within_the_line_budget() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/services");
    let mut sizes = BTreeMap::new();
    collect(&root, &mut sizes);

    let allowed = baseline();
    let mut over = Vec::new();
    for (name, lines) in &sizes {
        let budget = allowed.get(name).copied().unwrap_or(MAX_LINES);
        if *lines > budget {
            over.push(format!("{name} ({lines} > {budget})"));
        }
    }
    assert!(
        over.is_empty(),
        "split these service files into modules, or lower the baseline if they shrank: {over:#?}"
    );

    // A stale baseline hides work that is already done: drop the entries whose
    // file shrank, so the ratchet keeps pulling in the same direction.
    let stale: Vec<&String> = allowed
        .iter()
        .filter(|(name, budget)| sizes.get(*name).is_some_and(|lines| lines < budget))
        .map(|(name, _)| name)
        .collect();
    assert!(
        stale.is_empty(),
        "these baseline entries are now under budget, remove them: {stale:#?}"
    );
}
