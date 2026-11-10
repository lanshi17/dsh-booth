//! Host-collected evidence and real Git merge helpers.
//!
//! Worker-authored statements cannot satisfy the review gate. Diff/HEAD/tests must be
//! observed from a controlled worktree by the host.
use forge_domain::valid_commit;
use std::io;
use std::path::Path;
use std::process::Command;
use thiserror::Error;

pub struct CheckResult {
    pub command: Vec<String>,
    pub exit_code: i32,
}

pub struct ReviewEvidence {
    pub observed_head: String,
    pub observed_base: String,
    pub diff_inspected: bool,
    pub diff_stat: String,
    pub checks: Vec<CheckResult>,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("review requires a matching commit, inspected diff and passing host-run checks")]
pub struct ReviewError;

impl ReviewEvidence {
    pub fn bind_commit(&self, reported_head: &str) -> Result<(), ReviewError> {
        if !valid_commit(reported_head) || self.observed_head != reported_head {
            return Err(ReviewError);
        }
        Ok(())
    }

    pub fn validate(&self, reported_head: &str) -> Result<(), ReviewError> {
        self.bind_commit(reported_head)?;
        if !self.diff_inspected
            || self.checks.is_empty()
            || self
                .checks
                .iter()
                .any(|c| c.command.is_empty() || c.exit_code != 0)
        {
            return Err(ReviewError);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CollectError {
    #[error("git/command failed: {0}")]
    Command(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Observe HEAD, diff against `base_commit`, and run host check argv arrays in `worktree`.
pub fn collect_evidence(
    worktree: &Path,
    base_commit: &str,
    checks: &[Vec<String>],
) -> Result<ReviewEvidence, CollectError> {
    if !valid_commit(base_commit) {
        return Err(CollectError::Command("base commit is invalid".into()));
    }
    let head = git_output(worktree, &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    if !valid_commit(&head) {
        return Err(CollectError::Command(format!("invalid HEAD {head}")));
    }
    let range = format!("{base_commit}..{head}");
    let diff_stat = git_output(worktree, &["diff", "--stat", &range])?;
    let mut results = Vec::with_capacity(checks.len());
    for command in checks {
        if command.is_empty() {
            return Err(CollectError::Command("empty check command".into()));
        }
        let status = Command::new(&command[0])
            .args(&command[1..])
            .current_dir(worktree)
            .env("GIT_TERMINAL_PROMPT", "0")
            .status()?;
        results.push(CheckResult {
            command: command.clone(),
            exit_code: status.code().unwrap_or(-1),
        });
    }
    Ok(ReviewEvidence {
        observed_head: head,
        observed_base: base_commit.to_owned(),
        diff_inspected: true,
        diff_stat,
        checks: results,
    })
}

#[derive(Debug, Error)]
pub enum MergeError {
    #[error("merge authorization is missing")]
    MissingAuth,
    #[error("merge authorization expired")]
    Expired,
    #[error("merge authorization already consumed")]
    Consumed,
    #[error("worktree HEAD does not match authorized head (post-review drift or tampering)")]
    HeadMismatch,
    #[error("target branch tip does not match authorized base")]
    BaseMismatch,
    #[error("git merge failed: {0}")]
    Git(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Verify target tip == `expected_base` and source tip == `expected_head`.
pub fn verify_merge_cas(
    repo: &Path,
    target_branch: &str,
    source_branch: &str,
    expected_base: &str,
    expected_head: &str,
) -> Result<(), MergeError> {
    let current_base = git_output(repo, &["rev-parse", target_branch])?
        .trim()
        .to_owned();
    if current_base != expected_base {
        return Err(MergeError::BaseMismatch);
    }
    let source_head = git_output(repo, &["rev-parse", source_branch])?
        .trim()
        .to_owned();
    if source_head != expected_head {
        return Err(MergeError::HeadMismatch);
    }
    Ok(())
}

/// Merge `source_branch` into `target_branch` after verifying base/head CAS.
///
/// Returns the resulting HEAD commit of `target_branch`.
pub fn merge_approved_branch(
    repo: &Path,
    target_branch: &str,
    source_branch: &str,
    expected_base: &str,
    expected_head: &str,
) -> Result<String, MergeError> {
    verify_merge_cas(
        repo,
        target_branch,
        source_branch,
        expected_base,
        expected_head,
    )?;
    git_ok(repo, &["checkout", target_branch])?;
    if git_ok(repo, &["merge", "--ff-only", source_branch]).is_err() {
        git_ok(
            repo,
            &[
                "merge",
                "--no-ff",
                "-m",
                "forge: merge approved assignment",
                source_branch,
            ],
        )?;
    }
    let merged = git_output(repo, &["rev-parse", "HEAD"])?.trim().to_owned();
    if !valid_commit(&merged) {
        return Err(MergeError::Git(format!("invalid merged HEAD {merged}")));
    }
    Ok(merged)
}

/// Push `branch` to `remote` with argv-only `git push` (no force, no shell).
///
/// Intended for post-merge publication of an already-authorized target tip.
pub fn push_branch(repo: &Path, remote: &str, branch: &str) -> Result<(), MergeError> {
    if remote.is_empty() || remote.contains(['\0', '\n', ' ']) {
        return Err(MergeError::Git("invalid remote name".into()));
    }
    if branch.is_empty() || branch.contains(['\0', '\n', ' ']) {
        return Err(MergeError::Git("invalid branch name".into()));
    }
    git_ok(repo, &["push", remote, branch])
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String, io::Error> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LANG", "C")
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_ok(cwd: &Path, args: &[&str]) -> Result<(), MergeError> {
    git_output(cwd, args).map(|_| ()).map_err(|err| {
        let msg = err.to_string();
        if msg.contains("git ") {
            MergeError::Git(msg)
        } else {
            MergeError::Io(err)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn init_repo(root: &Path) -> String {
        for args in [
            ["init"][..].as_ref(),
            ["config", "user.email", "forge@example.com"].as_ref(),
            ["config", "user.name", "Forge"].as_ref(),
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(root)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        fs::write(root.join("README"), "base\n").unwrap();
        assert!(
            Command::new("git")
                .args(["add", "README"])
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "base"])
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
        git_output(root, &["rev-parse", "HEAD"])
            .unwrap()
            .trim()
            .to_owned()
    }

    #[test]
    fn stale_commit_missing_checks_and_failed_tests_cannot_pass() {
        let head = "a".repeat(40);
        let mut evidence = ReviewEvidence {
            observed_head: head.clone(),
            observed_base: "b".repeat(40),
            diff_inspected: true,
            diff_stat: String::new(),
            checks: vec![],
        };
        assert!(evidence.validate(&head).is_err());
        evidence.checks.push(CheckResult {
            command: vec!["cargo".into(), "test".into()],
            exit_code: 1,
        });
        assert!(evidence.validate(&head).is_err());
        evidence.checks[0].exit_code = 0;
        assert!(evidence.validate(&"b".repeat(40)).is_err());
        assert!(evidence.validate(&head).is_ok());
    }

    #[test]
    fn collect_evidence_runs_real_git_and_checks() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let base = init_repo(&repo);
        fs::write(repo.join("feature.txt"), "x\n").unwrap();
        assert!(
            Command::new("git")
                .args(["add", "feature.txt"])
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "feature"])
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
        let evidence = collect_evidence(&repo, &base, &[vec!["true".into()]]).unwrap();
        assert!(evidence.diff_inspected);
        assert!(!evidence.diff_stat.is_empty() || evidence.observed_head != base);
        assert_eq!(evidence.checks[0].exit_code, 0);
        assert!(evidence.validate(&evidence.observed_head).is_ok());
    }

    #[test]
    fn push_branch_updates_bare_remote() {
        let dir = tempdir().unwrap();
        let bare = dir.path().join("remote.git");
        let work = dir.path().join("work");
        fs::create_dir_all(&work).unwrap();
        assert!(
            Command::new("git")
                .args(["init", "--bare", "-b", "master"])
                .arg(&bare)
                .status()
                .unwrap()
                .success()
        );
        let base = init_repo(&work);
        assert!(
            Command::new("git")
                .args(["remote", "add", "origin"])
                .arg(&bare)
                .current_dir(&work)
                .status()
                .unwrap()
                .success()
        );
        push_branch(&work, "origin", "master").unwrap();
        let remote_head = git_output(&bare, &["rev-parse", "master"])
            .unwrap()
            .trim()
            .to_owned();
        assert_eq!(remote_head, base);
    }
}
