//! Who carries out one instruction. The desk stays booth. The default worker is dsh.
//!
//! Naming someone else starts that coding agent directly. There is no second control plane.

use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Who {
    Dsh,
    Omp,
    Claude,
    Codex,
    Cursor,
}

impl Who {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name.trim().to_ascii_lowercase().as_str() {
            "dsh" => Ok(Self::Dsh),
            "omp" | "pi" => Ok(Self::Omp),
            "claude" | "claude-code" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "cursor" | "cursor-agent" => Ok(Self::Cursor),
            other => Err(format!(
                "unknown worker {other}; choose dsh, omp, claude, codex, or cursor-agent"
            )),
        }
    }
}

/// `@omp ...` names the worker. Anything else, including no prefix, is dsh.
pub fn split(text: &str) -> (Who, String) {
    let text = text.trim();
    for (prefix, who) in [
        ("@omp ", Who::Omp),
        ("@pi ", Who::Omp),
        ("@claude ", Who::Claude),
        ("@codex ", Who::Codex),
        ("@cursor-agent ", Who::Cursor),
        ("@cursor ", Who::Cursor),
        ("@dsh ", Who::Dsh),
    ] {
        if let Some(rest) = strip_ignore_case(text, prefix) {
            return (who, rest.trim().to_owned());
        }
    }
    (Who::Dsh, text.to_owned())
}

/// Run a named coding agent on one instruction. dsh is not started here.
pub fn run(who: Who, text: &str) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("an instruction needs text".into());
    }
    let (bin, mut args) = match who {
        Who::Dsh => return Err("dsh is the desk's own team".into()),
        Who::Omp => ("omp", vec!["-p".into()]),
        Who::Claude => ("claude", vec!["-p".into()]),
        Who::Codex => ("codex", vec!["exec".into()]),
        // --trust skips the workspace prompt; otherwise a background handoff waits forever.
        Who::Cursor => ("cursor-agent", vec!["-p".into(), "--trust".into()]),
    };
    args.push(text.to_owned());
    let output = Command::new(bin).args(&args).output().map_err(|error| {
        format!("cannot start {bin}: {error}. Install it, or leave the worker as dsh.")
    })?;
    let mut shown = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if shown.is_empty() {
        shown = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    }
    if output.status.success() {
        Ok(if shown.is_empty() {
            format!("{bin} finished")
        } else {
            shown
        })
    } else {
        Err(if shown.is_empty() {
            format!("{bin} failed")
        } else {
            shown
        })
    }
}

fn strip_ignore_case<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    let mut text_chars = text.chars();
    for expected in prefix.chars() {
        let found = text_chars.next()?;
        if !found.eq_ignore_ascii_case(&expected) {
            return None;
        }
    }
    Some(&text[prefix.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_worker_is_dsh() {
        assert_eq!(split("fix the readme").0, Who::Dsh);
        assert_eq!(Who::parse("dsh").unwrap(), Who::Dsh);
    }

    #[test]
    fn a_prefix_names_the_worker() {
        let (who, text) = split("@omp change the prompt");
        assert_eq!(who, Who::Omp);
        assert_eq!(text, "change the prompt");
        assert_eq!(Who::parse("claude-code").unwrap(), Who::Claude);
        assert_eq!(Who::parse("cursor-agent").unwrap(), Who::Cursor);
        let (who, text) = split("@cursor-agent fix the prompt");
        assert_eq!(who, Who::Cursor);
        assert_eq!(text, "fix the prompt");
        assert!(Who::parse("forge").is_err());
    }
}
