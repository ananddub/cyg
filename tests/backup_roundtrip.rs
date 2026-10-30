use std::{path::Path, process::Command};

fn surreal_cli_present() -> bool {
    Command::new("surreal")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn backup_refuses_a_missing_server_and_stops() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("backups");
    let status = Command::new("./scripts/backup.sh")
        .env("SURREAL_ENDPOINT", "http://127.0.0.1:1")
        .arg(&out)
        .status()
        .expect("the script is executable");
    assert!(!status.success(), "export against a dead server must fail");
    assert!(!out.exists() || std::fs::read_dir(&out).unwrap().next().is_none());
}

#[test]
fn export_drop_restore_round_trip_with_the_surreal_cli() {
    if !surreal_cli_present() {
        eprintln!("SKIP: the surreal CLI is not installed on this machine");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let seed = dir.path().join("seed");
    let restored = dir.path().join("restored");
    let seed_url = format!("surrealkv://{}", seed.display());
    let restored_url = format!("surrealkv://{}", restored.display());

    assert!(
        Command::new("surreal")
            .args(["db", "import", "--log", "none"])
            .arg("--endpoint")
            .arg(&seed_url)
            .args(["--ns", "cyg", "--db", "cyg"])
            .arg(Path::new("tests/fixtures/backup_round_trip.surql"))
            .status()
            .expect("surreal import into the seed store")
            .success(),
        "the seed store must accept the fixture"
    );

    let out = dir.path().join("backups");
    let archive = Command::new("./scripts/backup.sh")
        .env("SURREAL_ENDPOINT", &seed_url)
        .arg(&out)
        .output()
        .expect("backup.sh")
        .stdout;
    let archive = String::from_utf8(archive).unwrap();
    let archive = archive.trim().to_owned();
    assert!(Path::new(&archive).exists(), "the archive must be written");

    assert!(
        Command::new("./scripts/restore.sh")
            .env("SURREAL_ENDPOINT", &restored_url)
            .arg(&archive)
            .status()
            .expect("restore.sh")
            .success(),
        "restore into a fresh path must succeed"
    );

    let refusal = Command::new("./scripts/restore.sh")
        .env("SURREAL_ENDPOINT", &restored_url)
        .arg(&archive)
        .status()
        .expect("restore.sh");
    assert!(
        !refusal.success(),
        "a non-empty target must be refused without --force"
    );
}
