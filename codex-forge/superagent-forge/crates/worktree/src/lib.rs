//! Real Git worktree allocation. Commands use argv arrays; never `sh -c`.
use forge_domain::AssignmentId;
use forge_domain::WorkspaceLease;
use forge_domain::valid_commit;
use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use thiserror::Error;

pub trait WorktreeManager: Send + Sync {
    fn allocate(&self, assignment: AssignmentId, base_commit: &str) -> io::Result<WorkspaceLease>;
    fn release(&self, assignment: AssignmentId) -> io::Result<()>;
}

#[derive(Debug, Error)]
pub enum WorktreeError {
    #[error("git executable or repository is invalid")]
    InvalidRepo,
    #[error("base commit must be a full hex SHA")]
    InvalidCommit,
    #[error("assignment already has an active worktree")]
    AlreadyAllocated,
    #[error("no active worktree for assignment")]
    NotFound,
    #[error("git command failed: {0}")]
    Git(String),
}

impl From<WorktreeError> for io::Error {
    fn from(value: WorktreeError) -> Self {
        let kind = match value {
            WorktreeError::AlreadyAllocated => io::ErrorKind::AlreadyExists,
            WorktreeError::NotFound => io::ErrorKind::NotFound,
            WorktreeError::InvalidCommit | WorktreeError::InvalidRepo => {
                io::ErrorKind::InvalidInput
            }
            WorktreeError::Git(_) => io::ErrorKind::Other,
        };
        io::Error::new(kind, value)
    }
}

/// Creates one detached worktree per assignment under `worktrees_root`.
///
/// The main repository working tree is never used as a worker cwd. Callers must
/// still enforce OS sandboxing so workers cannot write the main tree by absolute path.
pub struct GitWorktreeManager {
    git: PathBuf,
    repo_root: PathBuf,
    worktrees_root: PathBuf,
    active: Mutex<HashMap<AssignmentId, WorkspaceLease>>,
}

impl GitWorktreeManager {
    pub fn new(
        repo_root: impl Into<PathBuf>,
        worktrees_root: impl Into<PathBuf>,
    ) -> io::Result<Self> {
        Self::with_git("git", repo_root, worktrees_root)
    }

    pub fn with_git(
        git: impl Into<PathBuf>,
        repo_root: impl Into<PathBuf>,
        worktrees_root: impl Into<PathBuf>,
    ) -> io::Result<Self> {
        let git = git.into();
        let repo_root = repo_root
            .into()
            .canonicalize()
            .map_err(|err| io::Error::new(err.kind(), format!("canonicalize repo_root: {err}")))?;
        let worktrees_root = worktrees_root.into();
        std::fs::create_dir_all(&worktrees_root)?;
        let worktrees_root = worktrees_root.canonicalize().map_err(|err| {
            io::Error::new(err.kind(), format!("canonicalize worktrees_root: {err}"))
        })?;
        if !run_git(&git, &repo_root, &["rev-parse", "--is-inside-work-tree"])?
            .stdout
            .trim()
            .eq_ignore_ascii_case("true")
        {
            return Err(WorktreeError::InvalidRepo.into());
        }
        // Refuse overlapping roots: workers must not share the main checkout path.
        if worktrees_root.starts_with(&repo_root) || repo_root.starts_with(&worktrees_root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "worktrees_root must be outside the main repository root",
            ));
        }
        Ok(Self {
            git,
            repo_root,
            worktrees_root,
            active: Mutex::new(HashMap::new()),
        })
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn worktrees_root(&self) -> &Path {
        &self.worktrees_root
    }
}

impl WorktreeManager for GitWorktreeManager {
    fn allocate(&self, assignment: AssignmentId, base_commit: &str) -> io::Result<WorkspaceLease> {
        if !valid_commit(base_commit) {
            return Err(WorktreeError::InvalidCommit.into());
        }
        let mut active = self
            .active
            .lock()
            .map_err(|_| io::Error::other("worktree mutex poisoned"))?;
        if active.contains_key(&assignment) {
            return Err(WorktreeError::AlreadyAllocated.into());
        }

        let branch = format!("forge/{}", assignment.0);
        let directory = self.worktrees_root.join(assignment.0.to_string());
        if directory.exists() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "worktree directory already exists",
            ));
        }

        // Verify commit exists before creating directories.
        run_git(
            &self.git,
            &self.repo_root,
            &["cat-file", "-e", &format!("{base_commit}^{{commit}}")],
        )?;

        let dir_str = directory.to_string_lossy().into_owned();
        run_git(
            &self.git,
            &self.repo_root,
            &["worktree", "add", "-b", &branch, &dir_str, base_commit],
        )?;

        let lease = WorkspaceLease {
            directory: directory.canonicalize()?,
            branch,
            base_commit: base_commit.to_owned(),
        };
        lease
            .validate()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        active.insert(assignment, lease.clone());
        Ok(lease)
    }

    fn release(&self, assignment: AssignmentId) -> io::Result<()> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| io::Error::other("worktree mutex poisoned"))?;
        let Some(lease) = active.remove(&assignment) else {
            return Err(WorktreeError::NotFound.into());
        };
        let dir = lease.directory.to_string_lossy().into_owned();
        // Prefer git worktree remove; fall back to force cleanup of registry entry.
        let remove = run_git(
            &self.git,
            &self.repo_root,
            &["worktree", "remove", "--force", &dir],
        );
        if remove.is_err() {
            let _ = run_git(&self.git, &self.repo_root, &["worktree", "prune"]);
            if lease.directory.exists() {
                std::fs::remove_dir_all(&lease.directory)?;
            }
            let _ = run_git(&self.git, &self.repo_root, &["branch", "-D", &lease.branch]);
        }
        Ok(())
    }
}

struct GitOutput {
    stdout: String,
}

fn run_git(git: &Path, cwd: &Path, args: &[&str]) -> io::Result<GitOutput> {
    let output = Command::new(git)
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LANG", "C")
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(
            WorktreeError::Git(format!("{} {}: {detail}", git.display(), args.join(" "))).into(),
        );
    }
    Ok(GitOutput {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn init_repo(root: &Path) -> String {
        for args in [
            ["init"][..].as_ref(),
            ["config", "user.email", "forge@example.com"].as_ref(),
            ["config", "user.name", "Forge"].as_ref(),
        ] {
            run_git(Path::new("git"), root, args).unwrap();
        }
        fs::write(root.join("protected.txt"), "main-only\n").unwrap();
        run_git(Path::new("git"), root, &["add", "protected.txt"]).unwrap();
        run_git(Path::new("git"), root, &["commit", "-m", "initial"]).unwrap();
        run_git(Path::new("git"), root, &["rev-parse", "HEAD"])
            .unwrap()
            .stdout
            .trim()
            .to_owned()
    }

    #[test]
    fn allocates_isolated_worktrees_outside_main() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let trees = dir.path().join("trees");
        fs::create_dir_all(&repo).unwrap();
        let head = init_repo(&repo);
        let manager = GitWorktreeManager::new(&repo, &trees).unwrap();
        let a = AssignmentId(Uuid::new_v4());
        let b = AssignmentId(Uuid::new_v4());
        let lease_a = manager.allocate(a, &head).unwrap();
        let lease_b = manager.allocate(b, &head).unwrap();
        assert!(lease_a.directory.starts_with(manager.worktrees_root()));
        assert!(lease_b.directory.starts_with(manager.worktrees_root()));
        assert_ne!(lease_a.directory, lease_b.directory);
        assert_ne!(lease_a.branch, lease_b.branch);
        assert!(!lease_a.directory.starts_with(manager.repo_root()));
        assert_eq!(
            fs::read_to_string(lease_a.directory.join("protected.txt")).unwrap(),
            "main-only\n"
        );
        // Worker-style write stays in worktree, not main.
        fs::write(lease_a.directory.join("worker-out.txt"), "from-a\n").unwrap();
        assert!(!repo.join("worker-out.txt").exists());
        assert!(lease_a.directory.join("worker-out.txt").exists());
        manager.release(a).unwrap();
        manager.release(b).unwrap();
        assert!(!lease_a.directory.exists());
        assert!(!lease_b.directory.exists());
    }

    #[test]
    fn rejects_overlapping_roots_and_duplicate_allocate() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        let head = init_repo(&repo);
        let nested = repo.join("nested-trees");
        assert!(GitWorktreeManager::new(&repo, &nested).is_err());
        let trees = dir.path().join("trees");
        let manager = GitWorktreeManager::new(&repo, &trees).unwrap();
        let id = AssignmentId(Uuid::new_v4());
        manager.allocate(id, &head).unwrap();
        assert_eq!(
            manager.allocate(id, &head).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(
            manager
                .allocate(AssignmentId(Uuid::new_v4()), "deadbeef")
                .is_err()
        );
    }
}
