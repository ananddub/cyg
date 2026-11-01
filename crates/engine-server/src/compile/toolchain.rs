use std::path::Path;
use std::process::Command;

pub(crate) fn run(program: &str, args: &[String]) -> std::io::Result<()> {
    let status = Command::new(program).args(args).status()?;
    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("{program} exited with {status}"),
        ));
    }
    Ok(())
}

pub(crate) fn which(program: &str) -> bool {
    std::env::var("PATH")
        .unwrap_or_default()
        .split(std::path::MAIN_SEPARATOR)
        .any(|dir| {
            let p = Path::new(dir).join(program);
            p.is_file()
        })
}

pub(crate) fn detect_room_class(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("export class ") || trimmed.starts_with("export default class ") {
            let rest = if trimmed.starts_with("export default class ") {
                &trimmed["export default class ".len()..]
            } else {
                &trimmed["export class ".len()..]
            };
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("class ") && trimmed.contains(" extends Room") {
            let rest = &trimmed["class ".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}
