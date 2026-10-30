use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use engine_config::SessionConfig;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use sha2::{Digest, Sha256};

pub struct Artifacts {
    pub archive: PathBuf,
    pub checksum: PathBuf,
}

fn http_endpoint(session: &SessionConfig) -> Result<String, String> {
    let endpoint = session.database_endpoint.trim_end_matches('/');
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        Ok(endpoint.to_string())
    } else {
        Err(format!(
            "{} is an embedded endpoint; back it up with scripts/backup.sh while the server is stopped",
            session.database_endpoint
        ))
    }
}

fn stamp() -> String {
    Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte))
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn database_url(endpoint: &str, session: &SessionConfig, path: &str) -> String {
    format!(
        "{endpoint}/{path}?ns={}&db={}",
        encode(&session.database_ns),
        encode(&session.database_db)
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn gzip(sql: &[u8]) -> Result<Vec<u8>, String> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(sql).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())
}

pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut dec = GzDecoder::new(bytes);
    let mut out = Vec::new();
    dec.read_to_end(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// `sha256sum -c` compatible sidecar line.
pub fn sidecar_line(archive: &Path, gz: &[u8]) -> String {
    let name = archive.file_name().unwrap_or_default().to_string_lossy();
    format!("{}  {name}\n", hex(&Sha256::digest(gz)))
}

pub async fn backup(session: &SessionConfig, out_dir: &Path) -> Result<Artifacts, String> {
    let endpoint = http_endpoint(session)?;
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let client = reqwest::Client::new();
    let mut req = client.post(database_url(&endpoint, session, "export"));
    if !session.database_user.is_empty() {
        req = req.basic_auth(&session.database_user, Some(&session.database_pass));
    }
    let response = req.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "export failed: {status}: {}",
            response.text().await.unwrap_or_default()
        ));
    }
    let sql = response.bytes().await.map_err(|e| e.to_string())?;
    let gz = gzip(&sql)?;
    let archive = out_dir.join(format!("cyg-backup-{}.sql.gz", stamp()));
    std::fs::write(&archive, &gz).map_err(|e| e.to_string())?;
    let checksum = archive.with_extension("gz.sha256");
    std::fs::write(&checksum, sidecar_line(&archive, &gz)).map_err(|e| e.to_string())?;
    Ok(Artifacts { archive, checksum })
}

fn verify_sidecar(archive: &Path, gz: &[u8]) -> Result<(), String> {
    let sidecar = archive.with_extension("gz.sha256");
    if !sidecar.exists() {
        return Ok(());
    }
    let expected = std::fs::read_to_string(&sidecar)
        .map_err(|e| e.to_string())?
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string();
    let actual = hex(&Sha256::digest(gz));
    if expected != actual {
        return Err(format!(
            "checksum mismatch for {}: expected {expected}, got {actual}",
            archive.display()
        ));
    }
    Ok(())
}

async fn target_is_empty(session: &SessionConfig, endpoint: &str) -> Result<bool, String> {
    let client = reqwest::Client::new();
    let mut req = client
        .post(database_url(endpoint, session, "sql"))
        .header("Accept", "application/json")
        .body("SELECT VALUE id FROM users LIMIT 1;");
    if !session.database_user.is_empty() {
        req = req.basic_auth(&session.database_user, Some(&session.database_pass));
    }
    let response = req.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "liveness query failed: {status}: {}",
            response.text().await.unwrap_or_default()
        ));
    }
    let body: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let rows = body
        .as_array()
        .and_then(|a| a.first())
        .and_then(|r| r.get("result"));
    Ok(rows
        .map(|r| r.as_array().is_none_or(|a| a.is_empty()))
        .unwrap_or(true))
}

pub async fn restore(session: &SessionConfig, archive: &Path, force: bool) -> Result<(), String> {
    let gz = std::fs::read(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    verify_sidecar(archive, &gz)?;
    let sql = gunzip(&gz)?;
    let endpoint = http_endpoint(session)?;
    if !force && !target_is_empty(session, &endpoint).await? {
        return Err(
            "target database already holds user rows; re-run with --force to import anyway".into(),
        );
    }
    let client = reqwest::Client::new();
    let mut req = client
        .post(database_url(&endpoint, session, "import"))
        .body(sql);
    if !session.database_user.is_empty() {
        req = req.basic_auth(&session.database_user, Some(&session.database_pass));
    }
    let response = req.send().await.map_err(|e| e.to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "import failed: {status}: {}",
            response.text().await.unwrap_or_default()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(endpoint: &str) -> SessionConfig {
        let mut s = SessionConfig::default();
        s.database_endpoint = endpoint.into();
        s
    }

    #[test]
    fn gz_round_trips() {
        let sql = b"DEFINE TABLE users; CREATE users SET name = 'a';";
        assert_eq!(gunzip(&gzip(sql).unwrap()).unwrap(), sql);
    }

    #[test]
    fn sidecar_verifies_against_gzip_bytes() {
        let gz = gzip(b"hello").unwrap();
        let path = Path::new("/tmp/cyg-backup-test.sql.gz");
        let line = sidecar_line(path, &gz);
        let expected = format!("{}  cyg-backup-test.sql.gz\n", hex(&Sha256::digest(&gz)));
        assert_eq!(line, expected);
        assert!(line.starts_with(|c: char| c.is_ascii_hexdigit()));
        assert_eq!(line.trim_end().split_whitespace().next().unwrap().len(), 64);
    }

    #[test]
    fn embedded_endpoints_refuse_the_http_path() {
        assert!(http_endpoint(&session("mem://")).is_err());
        assert!(http_endpoint(&session("surrealkv://./data")).is_err());
        assert_eq!(
            http_endpoint(&session("http://127.0.0.1:8000/")).unwrap(),
            "http://127.0.0.1:8000"
        );
    }
}
