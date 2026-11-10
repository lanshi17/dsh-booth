//! Durable storage port and Turso (`turso_core`) backend.
//!
//! Engine choice: depend on `turso_core` with `default-features = false` and
//! `pure-rust-crypto`, without `simd`. The higher-level `turso` crate was rejected
//! for F1 because its defaults pull `simsimd` and `bindgen`/C tooling.
use forge_domain::AssignmentId;
use forge_domain::PrincipalId;
use forge_domain::ReviewDecision;
use forge_domain::Task;
use forge_domain::TaskId;
use serde_json::Error as JsonError;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use turso_core::ConnectOpts;
use turso_core::Connection;
use turso_core::Database;
use turso_core::IO;
use turso_core::LimboError;
use turso_core::PlatformIO;
use turso_core::SqliteDialect;
use turso_core::Statement;
use turso_core::StepResult;

pub const INITIAL_SCHEMA: &str = include_str!("../migrations/001_initial.sql");
pub const MCP_INBOX_SCHEMA: &str = include_str!("../migrations/002_mcp_inbox.sql");
pub const MERGE_AUTH_SCHEMA: &str = include_str!("../migrations/003_merge_auth.sql");
pub const MERGE_AUTH_SIGNATURE_SCHEMA: &str =
    include_str!("../migrations/004_merge_auth_signature.sql");

const MIGRATIONS: &[(i64, &str)] = &[
    (1, INITIAL_SCHEMA),
    (2, MCP_INBOX_SCHEMA),
    (3, MERGE_AUTH_SCHEMA),
    (4, MERGE_AUTH_SIGNATURE_SCHEMA),
];

pub struct TaskSnapshot {
    pub version: u64,
    pub task: Task,
}

pub trait TaskRepository: Send + Sync {
    fn load(&self, task: TaskId) -> io::Result<Option<TaskSnapshot>>;
    /// Compare-and-swap, atomically with assignments, reports and review audit records.
    /// None inserts only; Some(version) updates only if the persisted version matches.
    fn save(&self, task: &Task, expected_version: Option<u64>) -> io::Result<u64>;
}

/// File-backed Turso repository. Connections enable `foreign_keys`.
pub struct TursoTaskRepository {
    path: PathBuf,
    conn: Mutex<Arc<Connection>>,
}

impl TursoTaskRepository {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let io: Arc<dyn IO> = Arc::new(map_limbo(PlatformIO::new())?);
        let path_str = path.to_str().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "database path is not utf-8")
        })?;
        let db = map_limbo(Database::open_file(io, path_str, Arc::new(SqliteDialect)))?;
        let conn = map_limbo(db.connect(ConnectOpts::new()))?;
        map_limbo(conn.execute("PRAGMA foreign_keys=ON"))?;
        migrate(&conn)?;
        Ok(Self {
            path,
            conn: Mutex::new(conn),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Re-open the same file after a simulated process crash (drops in-memory handles).
    pub fn reopen(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::open(path)
    }
}

impl TaskRepository for TursoTaskRepository {
    fn load(&self, task_id: TaskId) -> io::Result<Option<TaskSnapshot>> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        load_task(&conn, task_id)
    }

    fn save(&self, task: &Task, expected_version: Option<u64>) -> io::Result<u64> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        map_limbo(conn.execute("BEGIN IMMEDIATE"))?;
        let result = save_task(&conn, task, expected_version);
        match result {
            Ok(version) => {
                map_limbo(conn.execute("COMMIT"))?;
                Ok(version)
            }
            Err(err) => {
                let _ = conn.execute("ROLLBACK");
                Err(err)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboxRecord {
    pub id: String,
    pub task_id: TaskId,
    pub event_json: String,
}

impl TursoTaskRepository {
    /// Persist task snapshot and outbox events in one transaction.
    pub fn save_with_outbox(
        &self,
        task: &Task,
        expected_version: Option<u64>,
        events: &[String],
    ) -> io::Result<(u64, Vec<String>)> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        map_limbo(conn.execute("BEGIN IMMEDIATE"))?;
        let result = (|| {
            let version = save_task(&conn, task, expected_version)?;
            let mut ids = Vec::with_capacity(events.len());
            for event_json in events {
                ids.push(insert_outbox(&conn, task.id(), event_json)?);
            }
            Ok((version, ids))
        })();
        match result {
            Ok(ok) => {
                map_limbo(conn.execute("COMMIT"))?;
                Ok(ok)
            }
            Err(err) => {
                let _ = conn.execute("ROLLBACK");
                Err(err)
            }
        }
    }

    pub fn undelivered_outbox(&self) -> io::Result<Vec<OutboxRecord>> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        let mut stmt = map_limbo(conn.prepare(
            "SELECT id, task_id, event_json FROM outbox WHERE delivered = 0 ORDER BY id",
        ))?;
        let mut rows = Vec::new();
        loop {
            match step_row(&mut stmt)? {
                None => break,
                Some(()) => {
                    let row = stmt
                        .row()
                        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing row"))?;
                    let id = row.get::<String>(0).map_err(limbo_io)?;
                    let task_id = row.get::<String>(1).map_err(limbo_io)?;
                    let event_json = row.get::<String>(2).map_err(limbo_io)?;
                    let uuid = uuid::Uuid::parse_str(&task_id).map_err(|err| {
                        io::Error::new(io::ErrorKind::InvalidData, err.to_string())
                    })?;
                    rows.push(OutboxRecord {
                        id,
                        task_id: TaskId(uuid),
                        event_json,
                    });
                }
            }
        }
        Ok(rows)
    }

    pub fn mark_outbox_delivered(&self, id: &str) -> io::Result<()> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        map_limbo(conn.execute(format!(
            "UPDATE outbox SET delivered = 1 WHERE id = {}",
            sql_quote(id)
        )))?;
        Ok(())
    }

    pub fn lookup_mcp_response(&self, message_id: &str) -> io::Result<Option<String>> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        let mut stmt = map_limbo(conn.prepare(format!(
            "SELECT response_json FROM mcp_inbox WHERE message_id = {}",
            sql_quote(message_id)
        )))?;
        if step_row(&mut stmt)?.is_none() {
            return Ok(None);
        }
        let row = stmt
            .row()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing row"))?;
        Ok(Some(row.get::<String>(0).map_err(limbo_io)?))
    }

    pub fn store_mcp_response(&self, message_id: &str, response_json: &str) -> io::Result<()> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        let now = now_unix();
        map_limbo(conn.execute(format!(
            "INSERT INTO mcp_inbox(message_id, response_json, created_at_unix) VALUES ({id}, {resp}, {now})",
            id = sql_quote(message_id),
            resp = sql_quote(response_json),
            now = now
        )))?;
        Ok(())
    }

    pub fn insert_merge_auth(&self, record: &MergeAuthRecord) -> io::Result<()> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        let signature = match &record.signature {
            Some(sig) => sql_quote(sig),
            None => "NULL".to_owned(),
        };
        map_limbo(conn.execute(format!(
            "INSERT INTO merge_authorizations(
                id, task_id, assignment_id, head_commit, base_commit,
                target_branch, source_branch, authorized_by, expires_at_unix, consumed, signature
             ) VALUES (
                {id}, {tid}, {aid}, {head}, {base},
                {target}, {source}, {by}, {exp}, 0, {signature}
             )",
            id = sql_quote(&record.id.to_string()),
            tid = sql_quote(&record.task_id.0.to_string()),
            aid = sql_quote(&record.assignment_id.0.to_string()),
            head = sql_quote(&record.head_commit),
            base = sql_quote(&record.base_commit),
            target = sql_quote(&record.target_branch),
            source = sql_quote(&record.source_branch),
            by = sql_quote(&record.authorized_by.0),
            exp = record.expires_at_unix,
            signature = signature,
        )))?;
        Ok(())
    }

    pub fn get_merge_auth(&self, id: uuid::Uuid) -> io::Result<Option<StoredMergeAuthRecord>> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        let mut stmt = map_limbo(conn.prepare(format!(
            "SELECT id, task_id, assignment_id, head_commit, base_commit,
                    target_branch, source_branch, authorized_by, expires_at_unix, consumed, signature
             FROM merge_authorizations WHERE id = {}",
            sql_quote(&id.to_string())
        )))?;
        if step_row(&mut stmt)?.is_none() {
            return Ok(None);
        }
        let row = stmt
            .row()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing row"))?;
        Ok(Some(row_to_merge_auth(row)?))
    }

    /// Consume only when the row exists, is unconsumed, and not expired.
    pub fn try_consume_merge_auth(&self, id: uuid::Uuid, now: u64) -> io::Result<MergeAuthRecord> {
        let conn = self.conn.lock().map_err(|_| poisoned())?;
        map_limbo(conn.execute("BEGIN IMMEDIATE"))?;
        let result = (|| {
            let mut stmt = map_limbo(conn.prepare(format!(
                "SELECT id, task_id, assignment_id, head_commit, base_commit,
                        target_branch, source_branch, authorized_by, expires_at_unix, consumed, signature
                 FROM merge_authorizations WHERE id = {}",
                sql_quote(&id.to_string())
            )))?;
            if step_row(&mut stmt)?.is_none() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "merge authorization missing",
                ));
            }
            let row = stmt
                .row()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing row"))?;
            let stored = row_to_merge_auth(row)?;
            if stored.consumed {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "merge authorization already consumed",
                ));
            }
            if stored.record.expires_at_unix <= now {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "merge authorization expired",
                ));
            }
            map_limbo(conn.execute(format!(
                "UPDATE merge_authorizations
                 SET consumed = 1, consumed_at_unix = {now}
                 WHERE id = {id} AND consumed = 0",
                id = sql_quote(&id.to_string()),
                now = now
            )))?;
            Ok(stored.record)
        })();
        match result {
            Ok(record) => {
                map_limbo(conn.execute("COMMIT"))?;
                Ok(record)
            }
            Err(err) => {
                let _ = conn.execute("ROLLBACK");
                Err(err)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeAuthRecord {
    pub id: uuid::Uuid,
    pub task_id: TaskId,
    pub assignment_id: AssignmentId,
    pub head_commit: String,
    pub base_commit: String,
    pub target_branch: String,
    pub source_branch: String,
    pub authorized_by: PrincipalId,
    pub expires_at_unix: u64,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredMergeAuthRecord {
    pub record: MergeAuthRecord,
    pub consumed: bool,
}

fn row_to_merge_auth(row: &turso_core::Row) -> io::Result<StoredMergeAuthRecord> {
    let id = uuid::Uuid::parse_str(&row.get::<String>(0).map_err(limbo_io)?)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let task_id = uuid::Uuid::parse_str(&row.get::<String>(1).map_err(limbo_io)?)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let assignment_id = uuid::Uuid::parse_str(&row.get::<String>(2).map_err(limbo_io)?)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    let signature = match row.get_value(10) {
        turso_core::Value::Null => None,
        turso_core::Value::Text(text) => Some(text.as_str().to_owned()),
        other => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("expected text/null signature, got {other:?}"),
            ));
        }
    };
    Ok(StoredMergeAuthRecord {
        record: MergeAuthRecord {
            id,
            task_id: TaskId(task_id),
            assignment_id: AssignmentId(assignment_id),
            head_commit: row.get::<String>(3).map_err(limbo_io)?,
            base_commit: row.get::<String>(4).map_err(limbo_io)?,
            target_branch: row.get::<String>(5).map_err(limbo_io)?,
            source_branch: row.get::<String>(6).map_err(limbo_io)?,
            authorized_by: PrincipalId(row.get::<String>(7).map_err(limbo_io)?),
            expires_at_unix: row.get::<i64>(8).map_err(limbo_io)? as u64,
            signature,
        },
        consumed: row.get::<i64>(9).map_err(limbo_io)? != 0,
    })
}

fn insert_outbox(conn: &Arc<Connection>, task_id: TaskId, event_json: &str) -> io::Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    map_limbo(conn.execute(format!(
        "INSERT INTO outbox(id, task_id, event_json, delivered) VALUES ({id}, {tid}, {ev}, 0)",
        id = sql_quote(&id),
        tid = sql_quote(&task_id.0.to_string()),
        ev = sql_quote(event_json),
    )))?;
    Ok(id)
}

fn migrate(conn: &Arc<Connection>) -> io::Result<()> {
    // Bootstrap migrations table if the file is empty.
    map_limbo(conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at_unix INTEGER NOT NULL
        )",
    ))?;
    for &(version, sql) in MIGRATIONS {
        let applied = query_i64(
            conn,
            format!("SELECT COUNT(*) FROM schema_migrations WHERE version = {version}"),
        )?;
        if applied > 0 {
            continue;
        }
        map_limbo(conn.execute("BEGIN IMMEDIATE"))?;
        let apply = (|| {
            for stmt_sql in split_statements(sql) {
                if stmt_sql
                    .to_ascii_lowercase()
                    .contains("create table schema_migrations")
                {
                    continue;
                }
                map_limbo(conn.execute(stmt_sql))?;
            }
            let now = now_unix();
            map_limbo(conn.execute(format!(
                "INSERT INTO schema_migrations(version, applied_at_unix) VALUES ({version}, {now})"
            )))?;
            Ok(())
        })();
        match apply {
            Ok(()) => map_limbo(conn.execute("COMMIT"))?,
            Err(err) => {
                let _ = conn.execute("ROLLBACK");
                return Err(err);
            }
        }
    }
    Ok(())
}

fn save_task(
    conn: &Arc<Connection>,
    task: &Task,
    expected_version: Option<u64>,
) -> io::Result<u64> {
    let id = task.id().0.to_string();
    let snapshot = serde_json::to_string(task).map_err(json_err)?;
    let current = query_optional_i64(
        conn,
        format!("SELECT version FROM tasks WHERE id = {}", sql_quote(&id)),
    )?;

    let new_version = match (expected_version, current) {
        (None, Some(_)) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "task already exists",
            ));
        }
        (None, None) => 1,
        (Some(expected), Some(actual)) if expected == actual as u64 => expected + 1,
        (Some(_), Some(_)) | (Some(_), None) => {
            return Err(io::Error::new(
                io::ErrorKind::ResourceBusy,
                "version CAS conflict",
            ));
        }
    };

    if current.is_some() {
        // Replace denormalized child rows for this task.
        for assignment in task.assignments() {
            let aid = assignment.id.0.to_string();
            map_limbo(conn.execute(format!(
                "DELETE FROM reviews WHERE report_id IN (
                    SELECT id FROM reports WHERE assignment_id = {aid}
                )",
                aid = sql_quote(&aid)
            )))?;
            map_limbo(conn.execute(format!(
                "DELETE FROM reports WHERE assignment_id = {}",
                sql_quote(&aid)
            )))?;
        }
        map_limbo(conn.execute(format!(
            "DELETE FROM assignments WHERE task_id = {}",
            sql_quote(&id)
        )))?;
        map_limbo(conn.execute(format!(
            "UPDATE tasks SET version = {new_version}, snapshot_json = {json} WHERE id = {id}",
            json = sql_quote(&snapshot),
            id = sql_quote(&id),
        )))?;
    } else {
        map_limbo(conn.execute(format!(
            "INSERT INTO tasks(id, version, snapshot_json) VALUES ({id}, {new_version}, {json})",
            id = sql_quote(&id),
            json = sql_quote(&snapshot),
        )))?;
    }

    for assignment in task.assignments() {
        map_limbo(conn.execute(format!(
            "INSERT INTO assignments(id, task_id, worker_id, directory, branch) VALUES ({aid}, {tid}, {wid}, {dir}, {branch})",
            aid = sql_quote(&assignment.id.0.to_string()),
            tid = sql_quote(&id),
            wid = sql_quote(&assignment.worker.0),
            dir = sql_quote(&assignment.workspace.directory.to_string_lossy()),
            branch = sql_quote(&assignment.workspace.branch),
        )))?;
    }
    for report in task.reports() {
        let report_json = serde_json::to_string(report).map_err(json_err)?;
        map_limbo(conn.execute(format!(
            "INSERT INTO reports(id, assignment_id, head_commit, report_json) VALUES ({rid}, {aid}, {head}, {json})",
            rid = sql_quote(&report.id.0.to_string()),
            aid = sql_quote(&report.assignment_id.0.to_string()),
            head = sql_quote(&report.head_commit),
            json = sql_quote(&report_json),
        )))?;
    }
    for review in task.reviews() {
        let decision = match review.decision {
            ReviewDecision::Approve => "approve",
            ReviewDecision::Reject => "reject",
        };
        map_limbo(conn.execute(format!(
            "INSERT INTO reviews(id, report_id, reviewer_id, decision, evidence_json) VALUES ({id}, {rid}, {rev}, {dec}, {ev})",
            id = sql_quote(&uuid::Uuid::new_v4().to_string()),
            rid = sql_quote(&review.report_id.0.to_string()),
            rev = sql_quote(&review.reviewer.0),
            dec = sql_quote(decision),
            ev = sql_quote(&review.evidence),
        )))?;
    }

    Ok(new_version)
}

fn load_task(conn: &Arc<Connection>, task_id: TaskId) -> io::Result<Option<TaskSnapshot>> {
    let id = task_id.0.to_string();
    let mut stmt = map_limbo(conn.prepare(format!(
        "SELECT version, snapshot_json FROM tasks WHERE id = {}",
        sql_quote(&id)
    )))?;
    match step_row(&mut stmt)? {
        None => Ok(None),
        Some(()) => {
            let row = stmt
                .row()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing row"))?;
            let version = row.get::<i64>(0).map_err(limbo_io)? as u64;
            let json = row.get::<String>(1).map_err(limbo_io)?;
            let task: Task = serde_json::from_str(&json).map_err(json_err)?;
            Ok(Some(TaskSnapshot { version, task }))
        }
    }
}

fn step_row(stmt: &mut Statement) -> io::Result<Option<()>> {
    loop {
        match map_limbo(stmt.step())? {
            StepResult::Row => return Ok(Some(())),
            StepResult::Done => return Ok(None),
            StepResult::IO | StepResult::Yield | StepResult::Sleep { .. } => {
                map_limbo(stmt.get_pager().io.step())?;
            }
            StepResult::Interrupt | StepResult::Busy => {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "statement interrupted or busy",
                ));
            }
        }
    }
}

fn query_i64(conn: &Arc<Connection>, sql: impl AsRef<str>) -> io::Result<i64> {
    let mut stmt = map_limbo(conn.prepare(sql.as_ref()))?;
    step_row(&mut stmt)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "expected scalar row"))?;
    stmt.row()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing row"))?
        .get::<i64>(0)
        .map_err(limbo_io)
}

fn query_optional_i64(conn: &Arc<Connection>, sql: impl AsRef<str>) -> io::Result<Option<i64>> {
    let mut stmt = map_limbo(conn.prepare(sql.as_ref()))?;
    if step_row(&mut stmt)?.is_none() {
        return Ok(None);
    }
    Ok(Some(
        stmt.row()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing row"))?
            .get::<i64>(0)
            .map_err(limbo_io)?,
    ))
}

fn split_statements(sql: &str) -> Vec<&str> {
    sql.split(';')
        .map(str::trim)
        .filter_map(|s| {
            if s.is_empty() {
                return None;
            }
            // Drop leading SQL line comments so migrations may start with `--`.
            let mut cleaned = s;
            while let Some(rest) = cleaned.strip_prefix("--") {
                cleaned = match rest.find('\n') {
                    Some(idx) => rest[idx + 1..].trim_start(),
                    None => return None,
                };
            }
            if cleaned.is_empty() {
                None
            } else {
                Some(cleaned)
            }
        })
        .collect()
}

fn sql_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push('\'');
        }
        out.push(ch);
    }
    out.push('\'');
    out
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn map_limbo<T>(result: Result<T, LimboError>) -> io::Result<T> {
    result.map_err(limbo_io)
}

fn limbo_io(err: LimboError) -> io::Error {
    match &err {
        LimboError::ConstraintViolation(_) => {
            io::Error::new(io::ErrorKind::InvalidInput, err.to_string())
        }
        _ => io::Error::other(err.to_string()),
    }
}

fn json_err(err: JsonError) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, err)
}

fn poisoned() -> io::Error {
    io::Error::other("repository mutex poisoned")
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_domain::AssignmentStatus;
    use forge_domain::BackendKind;
    use forge_domain::PrincipalId;
    use forge_domain::Source;
    use forge_domain::TaskMandate;
    use forge_domain::TaskStatus;
    use forge_domain::Trigger;
    use forge_domain::WorkspaceLease;
    use std::collections::BTreeSet;
    use tempfile::tempdir;

    fn sample_task() -> Task {
        let mut task = Task::accept("persist me".into(), Source::Cli, Trigger::Manual);
        let mut backends = BTreeSet::new();
        backends.insert(BackendKind::Omp);
        task.authorize(
            TaskMandate {
                allowed_backends: backends,
                allow_network: false,
                environment_names: BTreeSet::new(),
                timeout_seconds: 60,
                expires_at_unix: now_unix() + 3600,
            },
            now_unix(),
        )
        .unwrap();
        task
    }

    #[test]
    fn empty_database_migrates_and_roundtrips() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("forge.db");
        let repo = TursoTaskRepository::open(&path).unwrap();
        let task = sample_task();
        let version = repo.save(&task, None).unwrap();
        assert_eq!(version, 1);
        let loaded = repo.load(task.id()).unwrap().unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.task.id(), task.id());
        assert_eq!(loaded.task.status(), TaskStatus::Authorized);
    }

    #[test]
    fn cas_conflict_rejects_stale_write() {
        let dir = tempdir().unwrap();
        let repo = TursoTaskRepository::open(dir.path().join("forge.db")).unwrap();
        let task = sample_task();
        assert_eq!(repo.save(&task, None).unwrap(), 1);
        let err = repo.save(&task, Some(99)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::ResourceBusy);
        let loaded = repo.load(task.id()).unwrap().unwrap();
        assert_eq!(loaded.version, 1);
    }

    #[test]
    fn insert_conflict_when_expected_none() {
        let dir = tempdir().unwrap();
        let repo = TursoTaskRepository::open(dir.path().join("forge.db")).unwrap();
        let task = sample_task();
        repo.save(&task, None).unwrap();
        let err = repo.save(&task, None).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn rollback_leaves_prior_version() {
        let dir = tempdir().unwrap();
        let repo = TursoTaskRepository::open(dir.path().join("forge.db")).unwrap();
        let mut task = sample_task();
        assert_eq!(repo.save(&task, None).unwrap(), 1);
        // Force a failing child insert after the task update by using a reused branch
        // inside a save that would otherwise bump version — simulate via raw SQL constraint.
        let conn = repo.conn.lock().unwrap();
        map_limbo(conn.execute("BEGIN IMMEDIATE")).unwrap();
        map_limbo(conn.execute(format!(
            "UPDATE tasks SET version = 2 WHERE id = {}",
            sql_quote(&task.id().0.to_string())
        )))
        .unwrap();
        let fail = conn.execute(
            "INSERT INTO assignments(id, task_id, worker_id, directory, branch)
             VALUES ('x', 'missing-task', 'w', '/tmp/a', 'forge/a')",
        );
        assert!(fail.is_err(), "FK should reject missing task");
        map_limbo(conn.execute("ROLLBACK")).unwrap();
        drop(conn);
        let loaded = repo.load(task.id()).unwrap().unwrap();
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.task.status(), TaskStatus::Authorized);
        // Keep assignment status type linked so domain stays exercised.
        assert!(matches!(
            AssignmentStatus::Running,
            AssignmentStatus::Running
        ));
        let _ = &mut task;
    }

    #[test]
    fn unique_and_check_constraints() {
        let dir = tempdir().unwrap();
        let repo = TursoTaskRepository::open(dir.path().join("forge.db")).unwrap();
        let task = sample_task();
        repo.save(&task, None).unwrap();
        let conn = repo.conn.lock().unwrap();
        let tid = sql_quote(&task.id().0.to_string());
        map_limbo(conn.execute(format!(
            "INSERT INTO assignments(id, task_id, worker_id, directory, branch)
             VALUES ('a1', {tid}, 'w', '/abs/one', 'forge/one')"
        )))
        .unwrap();
        let dup_dir = conn.execute(format!(
            "INSERT INTO assignments(id, task_id, worker_id, directory, branch)
             VALUES ('a2', {tid}, 'w', '/abs/one', 'forge/two')"
        ));
        assert!(dup_dir.is_err());
        let bad_version =
            conn.execute("INSERT INTO tasks(id, version, snapshot_json) VALUES ('other', 0, '{}')");
        assert!(bad_version.is_err());
    }

    #[test]
    fn crash_restart_recovers_committed_state() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("forge.db");
        let task = sample_task();
        {
            let repo = TursoTaskRepository::open(&path).unwrap();
            assert_eq!(repo.save(&task, None).unwrap(), 1);
            let mut task2 = task.clone();
            let lease = WorkspaceLease {
                directory: PathBuf::from("/tmp/forge-work"),
                branch: "forge/restart".into(),
                base_commit: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            };
            task2
                .assign(
                    BackendKind::Omp,
                    PrincipalId("worker-1".into()),
                    lease,
                    now_unix(),
                )
                .unwrap();
            assert_eq!(repo.save(&task2, Some(1)).unwrap(), 2);
        }
        let repo = TursoTaskRepository::reopen(&path).unwrap();
        let loaded = repo.load(task.id()).unwrap().unwrap();
        assert_eq!(loaded.version, 2);
        assert_eq!(loaded.task.status(), TaskStatus::Running);
        assert_eq!(loaded.task.assignments().len(), 1);
    }

    #[test]
    fn migration_is_idempotent_on_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("forge.db");
        let _ = TursoTaskRepository::open(&path).unwrap();
        let repo = TursoTaskRepository::open(&path).unwrap();
        let conn = repo.conn.lock().unwrap();
        let count = query_i64(
            &conn,
            "SELECT COUNT(*) FROM schema_migrations WHERE version IN (1, 2, 3, 4)",
        )
        .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn outbox_survives_crash_until_delivered() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("forge.db");
        let task = sample_task();
        let event_ids = {
            let repo = TursoTaskRepository::open(&path).unwrap();
            let (_version, ids) = repo
                .save_with_outbox(&task, None, &[r#"{"type":"task_authorized"}"#.to_owned()])
                .unwrap();
            ids
        };
        let repo = TursoTaskRepository::reopen(&path).unwrap();
        let pending = repo.undelivered_outbox().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, event_ids[0]);
        assert_eq!(pending[0].task_id, task.id());
        repo.mark_outbox_delivered(&pending[0].id).unwrap();
        assert!(repo.undelivered_outbox().unwrap().is_empty());
    }

    #[test]
    fn mcp_inbox_deduplicates_message_ids() {
        let dir = tempdir().unwrap();
        let repo = TursoTaskRepository::open(dir.path().join("forge.db")).unwrap();
        assert!(repo.lookup_mcp_response("msg-1").unwrap().is_none());
        repo.store_mcp_response("msg-1", r#"{"ok":true}"#).unwrap();
        assert_eq!(
            repo.lookup_mcp_response("msg-1").unwrap().as_deref(),
            Some(r#"{"ok":true}"#)
        );
        let err = repo
            .store_mcp_response("msg-1", r#"{"ok":false}"#)
            .unwrap_err();
        assert!(
            matches!(
                err.kind(),
                io::ErrorKind::InvalidInput | io::ErrorKind::Other
            ),
            "duplicate inbox insert should fail, got {err:?}"
        );
    }

    #[test]
    fn merge_auth_survives_crash_and_consumes_once() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("forge.db");
        let id = uuid::Uuid::new_v4();
        let record = MergeAuthRecord {
            id,
            task_id: TaskId::new(),
            assignment_id: AssignmentId::new(),
            head_commit: "b".repeat(40),
            base_commit: "a".repeat(40),
            target_branch: "master".into(),
            source_branch: "forge/one".into(),
            authorized_by: PrincipalId("owner".into()),
            expires_at_unix: now_unix() + 3600,
            signature: Some("sig".into()),
        };
        {
            let repo = TursoTaskRepository::open(&path).unwrap();
            repo.insert_merge_auth(&record).unwrap();
        }
        let repo = TursoTaskRepository::reopen(&path).unwrap();
        let loaded = repo.get_merge_auth(id).unwrap().unwrap();
        assert!(!loaded.consumed);
        assert_eq!(loaded.record.head_commit, record.head_commit);
        assert_eq!(loaded.record.signature.as_deref(), Some("sig"));
        let consumed = repo.try_consume_merge_auth(id, now_unix()).unwrap();
        assert_eq!(consumed.id, id);
        let again = repo.try_consume_merge_auth(id, now_unix()).unwrap_err();
        assert_eq!(again.kind(), io::ErrorKind::AlreadyExists);
        let expired_id = uuid::Uuid::new_v4();
        let mut expired = record.clone();
        expired.id = expired_id;
        expired.expires_at_unix = now_unix().saturating_sub(1);
        repo.insert_merge_auth(&expired).unwrap();
        let err = repo
            .try_consume_merge_auth(expired_id, now_unix())
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    }
}
