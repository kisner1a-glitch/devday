use std::io::Write;
use std::process::{Command, Stdio};

/// Spawn `command args...`, write `prompt` to its stdin, return trimmed stdout.
pub fn run(command: &str, args: &[String], prompt: &str) -> Result<String, String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn {command}: {e}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "no stdin handle".to_string())?
        .write_all(prompt.as_bytes())
        .map_err(|e| format!("write stdin: {e}"))?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("provider exited {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipes_prompt_through_cat() {
        // `cat` echoes stdin to stdout — a stand-in for a real provider CLI.
        let out = run("cat", &[], "hello prompt").unwrap();
        assert_eq!(out, "hello prompt");
    }

    #[test]
    fn missing_binary_is_error() {
        let err = run("definitely-not-a-real-binary-xyz", &[], "x");
        assert!(err.is_err());
    }
}
