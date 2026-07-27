use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use rusqlite::{params, Connection, ErrorCode, OptionalExtension, TransactionBehavior};

use crate::error::StoreError;
use crate::limits::{cap_text, StoreLimits};
use crate::records::{
    DecisionRecord, EvidenceDraft, EvidenceRecord, FileEvidenceRecord, HandoffRecord,
    LaunchConfigRecord, SelectedChangeRecord, TaskRecord, TrustRecord,
};

/// 终态集合：与 halo-core 任务状态机的 is_terminal 语义一致（两 crate 零依赖，各自锁定）。
/// review_ready 仍可 accept/reject，属于非终态，Sidecar 重启恢复时同样标记 interrupted，
/// 由用户依据保留的证据决定重试或交接（对齐记录 01「不自动恢复或重放」）。
const TERMINAL_STATES: [&str; 5] = ["accepted", "rejected", "cancelled", "failed", "interrupted"];

/// 内嵌迁移脚本：新版本只允许在数组尾部追加，已发布条目不得修改。
const MIGRATIONS: &[&str] = &[MIGRATION_V1, MIGRATION_V2, MIGRATION_V3];

const MIGRATION_V1: &str = r#"
CREATE TABLE trust_records (
    real_path   TEXT PRIMARY KEY,
    root_commit TEXT,
    trusted     INTEGER NOT NULL,
    decided_at  TEXT NOT NULL
);

CREATE TABLE launch_configs (
    config_id       TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    agent           TEXT NOT NULL,
    executable_path TEXT NOT NULL,
    model           TEXT NOT NULL,
    thinking_level  TEXT NOT NULL,
    credential_ref  TEXT,
    extra_args      TEXT NOT NULL,
    env_overrides   TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE tasks (
    task_id              TEXT PRIMARY KEY,
    agent                TEXT NOT NULL,
    title                TEXT NOT NULL,
    state                TEXT NOT NULL,
    attribution          TEXT NOT NULL,
    baseline_head        TEXT,
    baseline_captured_at TEXT NOT NULL,
    created_at           TEXT NOT NULL,
    ended_at             TEXT,
    cancel_mode          TEXT
);
CREATE INDEX idx_tasks_state ON tasks(state);

CREATE TABLE evidence (
    task_id              TEXT NOT NULL,
    version              INTEGER NOT NULL,
    outcome              TEXT NOT NULL,
    attribution          TEXT NOT NULL,
    attribution_reasons  TEXT NOT NULL,
    summary              TEXT NOT NULL,
    summary_truncated    INTEGER NOT NULL,
    files                TEXT NOT NULL,
    verification_status  TEXT NOT NULL,
    verification_detail  TEXT NOT NULL,
    verification_source  TEXT NOT NULL,
    baseline_dirty_files TEXT NOT NULL,
    truncated            INTEGER NOT NULL,
    created_at           TEXT NOT NULL,
    PRIMARY KEY (task_id, version)
);

CREATE TRIGGER evidence_append_only_update BEFORE UPDATE ON evidence
BEGIN
    SELECT RAISE(ABORT, '交付证据为追加式，禁止改写既有版本');
END;

CREATE TRIGGER evidence_append_only_delete BEFORE DELETE ON evidence
BEGIN
    SELECT RAISE(ABORT, '交付证据为追加式，禁止删除既有版本');
END;

CREATE TABLE decisions (
    kind             TEXT NOT NULL,
    task_id          TEXT NOT NULL,
    evidence_version INTEGER NOT NULL,
    decided_at       TEXT NOT NULL,
    reason           TEXT,
    reason_truncated INTEGER NOT NULL
);
CREATE INDEX idx_decisions_task ON decisions(task_id);

CREATE TABLE handoffs (
    handoff_id          TEXT PRIMARY KEY,
    task_id             TEXT NOT NULL,
    source_agent        TEXT NOT NULL,
    target_agent        TEXT,
    goal                TEXT NOT NULL,
    goal_truncated      INTEGER NOT NULL,
    summary             TEXT NOT NULL,
    summary_truncated   INTEGER NOT NULL,
    selected_changes    TEXT NOT NULL,
    verification_status TEXT NOT NULL,
    verification_detail TEXT NOT NULL,
    truncated           INTEGER NOT NULL,
    created_at          TEXT NOT NULL
);
CREATE INDEX idx_handoffs_task ON handoffs(task_id);
"#;

// v1 已发布的任务记录没有任务目标。保留旧记录并以空目标回退到标题，
// 新记录由 Sidecar 写入脱敏、限长目标，保证重启后的交接不依赖内存状态。
const MIGRATION_V2: &str = r#"
ALTER TABLE tasks ADD COLUMN goal TEXT NOT NULL DEFAULT '';
"#;

// 任务恢复和 review.get 都需要知道哪些文件曾发生人工介入。路径列表在任务表
// 持久化；证据 files 列本来就是 JSON，新增 end_hash 以 serde 默认值兼容旧行。
const MIGRATION_V3: &str = r#"
ALTER TABLE tasks ADD COLUMN manual_edit_paths TEXT NOT NULL DEFAULT '[]';
"#;

pub struct Store {
    conn: Mutex<Connection>,
    limits: StoreLimits,
}

impl Store {
    /// 打开（必要时创建）数据库并执行内嵌迁移；重复调用幂等。
    pub fn open(path: &Path, limits: StoreLimits) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // WAL：本地单进程多线程访问下的稳妥默认（该 PRAGMA 会返回一行，须用查询而非 execute）
        let _mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |row| row.get(0))?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Mutex::new(conn),
            limits,
        })
    }

    /// 当前 schema 版本（诊断用）。
    pub fn schema_version(&self) -> Result<u32, StoreError> {
        let conn = self.conn();
        let v: i64 = conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |row| row.get(0),
        )?;
        Ok(v as u32)
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // 锁中毒仅代表持锁线程 panic；SQLite 连接自身状态仍一致，恢复继续使用而非扩大故障
        self.conn.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    // ---------- 信任记录 ----------

    pub fn get_trust(&self, real_path: &str) -> Result<Option<TrustRecord>, StoreError> {
        let conn = self.conn();
        let rec = conn
            .query_row(
                "SELECT real_path, root_commit, trusted, decided_at
                 FROM trust_records WHERE real_path = ?1",
                params![real_path],
                |row| {
                    Ok(TrustRecord {
                        real_path: row.get(0)?,
                        root_commit: row.get(1)?,
                        trusted: row.get(2)?,
                        decided_at: row.get(3)?,
                    })
                },
            )
            .optional()?;
        Ok(rec)
    }

    pub fn put_trust(&self, rec: &TrustRecord) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO trust_records (real_path, root_commit, trusted, decided_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(real_path) DO UPDATE SET
                 root_commit = excluded.root_commit,
                 trusted     = excluded.trusted,
                 decided_at  = excluded.decided_at",
            params![rec.real_path, rec.root_commit, rec.trusted, rec.decided_at],
        )?;
        Ok(())
    }

    /// 撤销信任：删除该路径的信任记录（无记录即未信任）；目标不存在时为无害空操作。
    pub fn revoke_trust(&self, real_path: &str) -> Result<(), StoreError> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM trust_records WHERE real_path = ?1",
            params![real_path],
        )?;
        Ok(())
    }

    // ---------- 受管启动配置 ----------

    pub fn list_configs(&self) -> Result<Vec<LaunchConfigRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT config_id, name, agent, executable_path, model, thinking_level,
                    credential_ref, extra_args, env_overrides, created_at, updated_at
             FROM launch_configs ORDER BY created_at ASC, config_id ASC",
        )?;
        let rows = stmt.query_map([], row_to_config)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn put_config(&self, cfg: &LaunchConfigRecord) -> Result<(), StoreError> {
        let extra_args = serde_json::to_string(&cfg.extra_args)?;
        let env_overrides = serde_json::to_string(&cfg.env_overrides)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO launch_configs (config_id, name, agent, executable_path, model,
                 thinking_level, credential_ref, extra_args, env_overrides, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(config_id) DO UPDATE SET
                 name            = excluded.name,
                 agent           = excluded.agent,
                 executable_path = excluded.executable_path,
                 model           = excluded.model,
                 thinking_level  = excluded.thinking_level,
                 credential_ref  = excluded.credential_ref,
                 extra_args      = excluded.extra_args,
                 env_overrides   = excluded.env_overrides,
                 created_at      = excluded.created_at,
                 updated_at      = excluded.updated_at",
            params![
                cfg.config_id,
                cfg.name,
                cfg.agent,
                cfg.executable_path,
                cfg.model,
                cfg.thinking_level,
                cfg.credential_ref,
                extra_args,
                env_overrides,
                cfg.created_at,
                cfg.updated_at,
            ],
        )?;
        Ok(())
    }

    /// 返回是否确实删除了一条配置；false 由上层映射为 CONFIG_NOT_FOUND。
    pub fn delete_config(&self, config_id: &str) -> Result<bool, StoreError> {
        let conn = self.conn();
        let n = conn.execute(
            "DELETE FROM launch_configs WHERE config_id = ?1",
            params![config_id],
        )?;
        Ok(n > 0)
    }

    // ---------- 任务 ----------

    pub fn put_task(&self, t: &TaskRecord) -> Result<(), StoreError> {
        let manual_edit_paths = serde_json::to_string(&t.manual_edit_paths)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO tasks (task_id, agent, title, goal, state, attribution, baseline_head,
                 manual_edit_paths, baseline_captured_at, created_at, ended_at, cancel_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(task_id) DO UPDATE SET
                 agent                = excluded.agent,
                 title                = excluded.title,
                 goal                 = excluded.goal,
                 state                = excluded.state,
                 attribution          = excluded.attribution,
                 baseline_head        = excluded.baseline_head,
                 manual_edit_paths    = excluded.manual_edit_paths,
                 baseline_captured_at = excluded.baseline_captured_at,
                 created_at           = excluded.created_at,
                 ended_at             = excluded.ended_at,
                 cancel_mode          = excluded.cancel_mode",
            params![
                t.task_id,
                t.agent,
                t.title,
                t.goal,
                t.state,
                t.attribution,
                t.baseline_head,
                manual_edit_paths,
                t.baseline_captured_at,
                t.created_at,
                t.ended_at,
                t.cancel_mode,
            ],
        )?;
        Ok(())
    }

    pub fn get_task(&self, task_id: &str) -> Result<Option<TaskRecord>, StoreError> {
        let conn = self.conn();
        let rec = conn
            .query_row(
            "SELECT task_id, agent, title, goal, state, attribution, baseline_head,
                        manual_edit_paths, baseline_captured_at, created_at, ended_at, cancel_mode
                 FROM tasks WHERE task_id = ?1",
                params![task_id],
                row_to_task,
            )
            .optional()?;
        Ok(rec)
    }

    /// 按创建时间倒序返回最近的任务。
    pub fn list_tasks(&self, limit: usize) -> Result<Vec<TaskRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT task_id, agent, title, goal, state, attribution, baseline_head,
                    manual_edit_paths, baseline_captured_at, created_at, ended_at, cancel_mode
             FROM tasks ORDER BY created_at DESC, rowid DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_task)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// 启动恢复：把所有非终态任务标记为 interrupted，返回受影响的 task_id（按 id 升序）。
    /// 不自动恢复或重放；ended_at 缺失时补记当前 UTC 时间。
    pub fn mark_non_terminal_interrupted(&self) -> Result<Vec<String>, StoreError> {
        let predicate = non_terminal_predicate();
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let ids: Vec<String> = {
            let mut stmt = tx.prepare(&format!(
                "SELECT task_id FROM tasks WHERE {predicate} ORDER BY task_id ASC"
            ))?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut ids = Vec::new();
            for r in rows {
                ids.push(r?);
            }
            ids
        };
        if !ids.is_empty() {
            let now = now_utc_rfc3339();
            tx.execute(
                &format!(
                    "UPDATE tasks SET state = 'interrupted',
                            ended_at = COALESCE(ended_at, ?1)
                     WHERE {predicate}"
                ),
                params![now],
            )?;
        }
        tx.commit()?;
        Ok(ids)
    }

    // ---------- 交付证据（追加式） ----------

    /// 追加一个证据版本，返回分配的版本号（当前最大版本 + 1，首个为 1）。
    /// 追加式双保险：代码上在独占事务内分配 max+1 且不存在任何改写 API；
    /// 库表上由 PRIMARY KEY 与禁止 UPDATE/DELETE 的触发器兜底。
    pub fn append_evidence(&self, task_id: &str, draft: &EvidenceDraft) -> Result<u32, StoreError> {
        let limits = self.limits;
        let (summary, summary_truncated) = cap_text(&draft.summary, limits.summary_max_bytes);
        let (detail, detail_truncated) =
            cap_text(&draft.verification_detail, limits.trace_text_max_bytes);

        let mut any_truncated = summary_truncated || detail_truncated;

        let mut reasons = Vec::with_capacity(draft.attribution_reasons.len());
        for r in &draft.attribution_reasons {
            let (text, tr) = cap_text(r, limits.trace_text_max_bytes);
            any_truncated |= tr;
            reasons.push(text);
        }

        // 单文件 diff 受 file_diff_max 约束，同时全版本 diff 总量受 version_total_max 预算约束
        let mut remaining = limits.version_total_max_bytes;
        let mut files = Vec::with_capacity(draft.files.len());
        for f in &draft.files {
            let per_cap = limits.file_diff_max_bytes.min(remaining);
            let (diff, tr) = cap_text(&f.diff, per_cap);
            remaining = remaining.saturating_sub(diff.len());
            any_truncated |= tr;
            files.push(FileEvidenceRecord {
                path: f.path.clone(),
                change: f.change.clone(),
                diff,
                truncated: tr,
                end_hash: f.end_hash.clone(),
            });
        }

        let reasons_json = serde_json::to_string(&reasons)?;
        let files_json = serde_json::to_string(&files)?;
        let dirty_json = serde_json::to_string(&draft.baseline_dirty_files)?;

        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let next: u32 = tx.query_row(
            "SELECT COALESCE(MAX(version), 0) + 1 FROM evidence WHERE task_id = ?1",
            params![task_id],
            |row| row.get(0),
        )?;
        let inserted = tx.execute(
            "INSERT INTO evidence (task_id, version, outcome, attribution, attribution_reasons,
                 summary, summary_truncated, files, verification_status, verification_detail,
                 verification_source, baseline_dirty_files, truncated, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                task_id,
                next,
                draft.outcome,
                draft.attribution,
                reasons_json,
                summary,
                summary_truncated,
                files_json,
                draft.verification_status,
                detail,
                draft.verification_source,
                dirty_json,
                any_truncated,
                draft.created_at,
            ],
        );
        match inserted {
            Ok(_) => {}
            Err(rusqlite::Error::SqliteFailure(err, _))
                if err.code == ErrorCode::ConstraintViolation =>
            {
                return Err(StoreError::EvidenceVersionExists {
                    task_id: task_id.to_owned(),
                    version: next,
                });
            }
            Err(e) => return Err(e.into()),
        }
        tx.commit()?;
        Ok(next)
    }

    /// 按版本升序返回某任务全部证据版本。
    pub fn list_evidence(&self, task_id: &str) -> Result<Vec<EvidenceRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT task_id, version, outcome, attribution, attribution_reasons, summary,
                    summary_truncated, files, verification_status, verification_detail,
                    verification_source, baseline_dirty_files, truncated, created_at
             FROM evidence WHERE task_id = ?1 ORDER BY version ASC",
        )?;
        let rows = stmt.query_map(params![task_id], row_to_evidence)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn latest_evidence(&self, task_id: &str) -> Result<Option<EvidenceRecord>, StoreError> {
        let conn = self.conn();
        let rec = conn
            .query_row(
                "SELECT task_id, version, outcome, attribution, attribution_reasons, summary,
                        summary_truncated, files, verification_status, verification_detail,
                        verification_source, baseline_dirty_files, truncated, created_at
                 FROM evidence WHERE task_id = ?1 ORDER BY version DESC LIMIT 1",
                params![task_id],
                row_to_evidence,
            )
            .optional()?;
        Ok(rec)
    }

    // ---------- 审查决定 ----------

    pub fn put_decision(&self, d: &DecisionRecord) -> Result<(), StoreError> {
        let (reason, capped) = match &d.reason {
            Some(r) => {
                let (text, c) = cap_text(r, self.limits.trace_text_max_bytes);
                (Some(text), c)
            }
            None => (None, false),
        };
        // 上游已截断的标记保留（OR），本层截断只增不减
        let reason_truncated = d.reason_truncated || capped;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO decisions (kind, task_id, evidence_version, decided_at, reason, reason_truncated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                d.kind,
                d.task_id,
                d.evidence_version,
                d.decided_at,
                reason,
                reason_truncated,
            ],
        )?;
        Ok(())
    }

    /// 按写入顺序倒序（最新在前）返回决定记录。
    pub fn list_decisions(&self, limit: usize) -> Result<Vec<DecisionRecord>, StoreError> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT kind, task_id, evidence_version, decided_at, reason, reason_truncated
             FROM decisions ORDER BY rowid DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(DecisionRecord {
                kind: row.get(0)?,
                task_id: row.get(1)?,
                evidence_version: row.get(2)?,
                decided_at: row.get(3)?,
                reason: row.get(4)?,
                reason_truncated: row.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ---------- 交接包 ----------

    pub fn put_handoff(&self, h: &HandoffRecord) -> Result<(), StoreError> {
        let limits = self.limits;
        // goal 与 summary 属于正文文本，同用 summary 上限；verification_detail 用 trace 上限
        let (goal, goal_capped) = cap_text(&h.goal, limits.summary_max_bytes);
        let (summary, summary_capped) = cap_text(&h.summary, limits.summary_max_bytes);
        let (detail, detail_capped) = cap_text(&h.verification_detail, limits.trace_text_max_bytes);

        let mut remaining = limits.version_total_max_bytes;
        let mut changes = Vec::with_capacity(h.selected_changes.len());
        let mut any_change_capped = false;
        for c in &h.selected_changes {
            let per_cap = limits.file_diff_max_bytes.min(remaining);
            let (diff, capped) = cap_text(&c.diff, per_cap);
            remaining = remaining.saturating_sub(diff.len());
            let truncated = c.truncated || capped;
            any_change_capped |= truncated;
            changes.push(SelectedChangeRecord {
                path: c.path.clone(),
                diff,
                truncated,
            });
        }

        let goal_truncated = h.goal_truncated || goal_capped;
        let summary_truncated = h.summary_truncated || summary_capped;
        let truncated =
            h.truncated || goal_truncated || summary_truncated || detail_capped || any_change_capped;
        let changes_json = serde_json::to_string(&changes)?;

        let conn = self.conn();
        conn.execute(
            "INSERT INTO handoffs (handoff_id, task_id, source_agent, target_agent, goal,
                 goal_truncated, summary, summary_truncated, selected_changes,
                 verification_status, verification_detail, truncated, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(handoff_id) DO UPDATE SET
                 task_id             = excluded.task_id,
                 source_agent        = excluded.source_agent,
                 target_agent        = excluded.target_agent,
                 goal                = excluded.goal,
                 goal_truncated      = excluded.goal_truncated,
                 summary             = excluded.summary,
                 summary_truncated   = excluded.summary_truncated,
                 selected_changes    = excluded.selected_changes,
                 verification_status = excluded.verification_status,
                 verification_detail = excluded.verification_detail,
                 truncated           = excluded.truncated,
                 created_at          = excluded.created_at",
            params![
                h.handoff_id,
                h.task_id,
                h.source_agent,
                h.target_agent,
                goal,
                goal_truncated,
                summary,
                summary_truncated,
                changes_json,
                h.verification_status,
                detail,
                truncated,
                h.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_handoff(&self, handoff_id: &str) -> Result<Option<HandoffRecord>, StoreError> {
        let conn = self.conn();
        let rec = conn
            .query_row(
                "SELECT handoff_id, task_id, source_agent, target_agent, goal, goal_truncated,
                        summary, summary_truncated, selected_changes, verification_status,
                        verification_detail, truncated, created_at
                 FROM handoffs WHERE handoff_id = ?1",
                params![handoff_id],
                row_to_handoff,
            )
            .optional()?;
        Ok(rec)
    }
}

// ---------- 行映射与内部工具 ----------

fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current: i64 = tx.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )?;
    let current = current.max(0) as usize;
    if current > MIGRATIONS.len() {
        return Err(StoreError::SchemaTooNew {
            found: current,
            supported: MIGRATIONS.len(),
        });
    }
    if current < MIGRATIONS.len() {
        for sql in &MIGRATIONS[current..] {
            tx.execute_batch(sql)?;
        }
        tx.execute("DELETE FROM schema_version", [])?;
        tx.execute(
            "INSERT INTO schema_version (version) VALUES (?1)",
            params![MIGRATIONS.len() as i64],
        )?;
    }
    tx.commit()?;
    Ok(())
}

fn non_terminal_predicate() -> String {
    let quoted: Vec<String> = TERMINAL_STATES.iter().map(|s| format!("'{s}'")).collect();
    format!("state NOT IN ({})", quoted.join(", "))
}

fn now_utc_rfc3339() -> String {
    let now = time::OffsetDateTime::now_utc();
    // 契约时间戳为整秒 UTC；replace_nanosecond(0) 与整秒 Rfc3339 格式化不会失败，
    // 兜底分支仅为满足非测试代码禁 panic 的纪律
    let now = now.replace_nanosecond(0).unwrap_or(now);
    now.format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn parse_json_col<T: serde::de::DeserializeOwned>(idx: usize, raw: &str) -> rusqlite::Result<T> {
    serde_json::from_str(raw).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn row_to_config(row: &rusqlite::Row<'_>) -> rusqlite::Result<LaunchConfigRecord> {
    let extra_args: String = row.get(7)?;
    let env_overrides: String = row.get(8)?;
    Ok(LaunchConfigRecord {
        config_id: row.get(0)?,
        name: row.get(1)?,
        agent: row.get(2)?,
        executable_path: row.get(3)?,
        model: row.get(4)?,
        thinking_level: row.get(5)?,
        credential_ref: row.get(6)?,
        extra_args: parse_json_col(7, &extra_args)?,
        env_overrides: parse_json_col(8, &env_overrides)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        task_id: row.get(0)?,
        agent: row.get(1)?,
        title: row.get(2)?,
        goal: row.get(3)?,
        state: row.get(4)?,
        attribution: row.get(5)?,
        baseline_head: row.get(6)?,
        manual_edit_paths: {
            let paths: String = row.get(7)?;
            parse_json_col(7, &paths)?
        },
        baseline_captured_at: row.get(8)?,
        created_at: row.get(9)?,
        ended_at: row.get(10)?,
        cancel_mode: row.get(11)?,
    })
}

fn row_to_evidence(row: &rusqlite::Row<'_>) -> rusqlite::Result<EvidenceRecord> {
    let reasons: String = row.get(4)?;
    let files: String = row.get(7)?;
    let dirty: String = row.get(11)?;
    Ok(EvidenceRecord {
        task_id: row.get(0)?,
        version: row.get(1)?,
        outcome: row.get(2)?,
        attribution: row.get(3)?,
        attribution_reasons: parse_json_col(4, &reasons)?,
        summary: row.get(5)?,
        summary_truncated: row.get(6)?,
        files: parse_json_col(7, &files)?,
        verification_status: row.get(8)?,
        verification_detail: row.get(9)?,
        verification_source: row.get(10)?,
        baseline_dirty_files: parse_json_col(11, &dirty)?,
        truncated: row.get(12)?,
        created_at: row.get(13)?,
    })
}

fn row_to_handoff(row: &rusqlite::Row<'_>) -> rusqlite::Result<HandoffRecord> {
    let changes: String = row.get(8)?;
    Ok(HandoffRecord {
        handoff_id: row.get(0)?,
        task_id: row.get(1)?,
        source_agent: row.get(2)?,
        target_agent: row.get(3)?,
        goal: row.get(4)?,
        goal_truncated: row.get(5)?,
        summary: row.get(6)?,
        summary_truncated: row.get(7)?,
        selected_changes: parse_json_col(8, &changes)?,
        verification_status: row.get(9)?,
        verification_detail: row.get(10)?,
        truncated: row.get(11)?,
        created_at: row.get(12)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::FileChangeDraft;

    fn draft() -> EvidenceDraft {
        EvidenceDraft {
            outcome: "finished".to_owned(),
            attribution: "agent_only".to_owned(),
            attribution_reasons: vec![],
            summary: "摘要".to_owned(),
            files: vec![FileChangeDraft {
                path: "src/a.rs".to_owned(),
                change: "modified".to_owned(),
                diff: "-old\n+new\n".to_owned(),
                end_hash: None,
            }],
            verification_status: "passed".to_owned(),
            verification_detail: "ok".to_owned(),
            verification_source: "agent".to_owned(),
            baseline_dirty_files: vec![],
            created_at: "2026-07-26T08:05:00Z".to_owned(),
        }
    }

    /// 追加式约束级兜底：绕过公共 API 的原始 SQL 改写路径必须被库表约束拒绝。
    #[test]
    fn evidence_raw_rewrite_paths_rejected_by_constraints() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();
        assert_eq!(store.append_evidence("task-1", &draft()).unwrap(), 1);

        let conn = store.conn();
        // UPDATE 被触发器拒绝
        let update = conn.execute("UPDATE evidence SET summary = 'x' WHERE task_id = 'task-1'", []);
        assert!(update.is_err(), "UPDATE 必须被 append-only 触发器拒绝");
        // DELETE 被触发器拒绝
        let delete = conn.execute("DELETE FROM evidence WHERE task_id = 'task-1'", []);
        assert!(delete.is_err(), "DELETE 必须被 append-only 触发器拒绝");
        // 重复 (task_id, version) 的 INSERT 被主键拒绝
        let dup = conn.execute(
            "INSERT INTO evidence (task_id, version, outcome, attribution, attribution_reasons,
                 summary, summary_truncated, files, verification_status, verification_detail,
                 verification_source, baseline_dirty_files, truncated, created_at)
             VALUES ('task-1', 1, 'finished', 'agent_only', '[]', 's', 0, '[]',
                     'passed', '', 'agent', '[]', 0, '2026-07-26T08:06:00Z')",
            [],
        );
        match dup {
            Err(rusqlite::Error::SqliteFailure(err, _)) => {
                assert_eq!(err.code, ErrorCode::ConstraintViolation);
            }
            other => panic!("重复版本 INSERT 应命中主键约束，实际：{other:?}"),
        }
        drop(conn);

        // 原纪录未被破坏
        let rec = store.latest_evidence("task-1").unwrap().unwrap();
        assert_eq!(rec.version, 1);
        assert_eq!(rec.summary, "摘要");
    }

    #[test]
    fn schema_version_is_single_row_after_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("halo.db");
        drop(Store::open(&path, StoreLimits::default()).unwrap());
        let store = Store::open(&path, StoreLimits::default()).unwrap();
        let conn = store.conn();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        let version: i64 = conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 3);
    }

    #[test]
    fn v1_tasks_migrate_without_losing_existing_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("halo.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATION_V1).unwrap();
            conn.execute("CREATE TABLE schema_version (version INTEGER NOT NULL)", [])
                .unwrap();
            conn.execute("INSERT INTO schema_version (version) VALUES (1)", [])
                .unwrap();
            conn.execute(
                "INSERT INTO tasks (task_id, agent, title, state, attribution, baseline_captured_at, created_at)
                 VALUES ('task-v1', 'pi', '旧任务标题', 'review_ready', 'agent_only',
                         '2026-07-26T08:00:00Z', '2026-07-26T08:00:00Z')",
                [],
            )
            .unwrap();
        }

        let store = Store::open(&path, StoreLimits::default()).unwrap();
        assert_eq!(store.schema_version().unwrap(), 3);
        let task = store.get_task("task-v1").unwrap().unwrap();
        assert_eq!(task.title, "旧任务标题");
        assert!(task.goal.is_empty(), "旧记录应显式使用空目标回退");
        assert!(task.manual_edit_paths.is_empty(), "旧记录应回退为空路径集合");
    }

    /// 凭据红线：表结构中不得出现任何密钥类列名。
    #[test]
    fn no_secret_like_column_names_in_schema() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("halo.db"), StoreLimits::default()).unwrap();
        let conn = store.conn();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        for table in tables {
            let mut cols = conn
                .prepare(&format!("PRAGMA table_info({table})"))
                .unwrap();
            let names: Vec<String> = cols
                .query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            for name in names {
                let lower = name.to_lowercase();
                for banned in ["secret", "token", "password", "apikey", "api_key"] {
                    assert!(
                        !lower.contains(banned),
                        "表 {table} 出现疑似密钥列：{name}"
                    );
                }
            }
        }
    }
}
