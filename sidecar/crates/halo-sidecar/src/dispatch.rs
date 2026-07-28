//! 方法路由：hello 门禁、typed params 解析、统一错误映射（契约错误码 + 中文文案）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use crossbeam_channel::unbounded;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use halo_config::{credential_env_var_for, AgentKind, CredentialStore};
use halo_core::{manual_edit_note, Attribution, ManualEditOp};
use halo_protocol::methods::{self, AgentKind as AgentKindDto};
use halo_protocol::{ErrorBody, ErrorCode, RequestEnvelope, Response, PROTOCOL_VERSION};
use halo_runtime::{OpenCodeRuntime, PiRuntime, RuntimeState, Timeouts};
use halo_store::Store;

use crate::git::{GitClient, GitError};
use crate::fs::{self, FsError};
use crate::mapping::{self, now_ts};
use crate::server::{EventBus, EventGapError};
use crate::state::{lock, ActiveWorkspace, AgentHandle, AppState};
use crate::task_flow::{self, FlowCtx};

const CAPABILITIES: &[&str] = &[
    "workspace", "config", "pi", "opencode", "task", "review", "handoff", "history", "fs",
];
const MANUAL_EDIT_OVERFLOW_NOTE: &str = "此后仍有更多文件发生人工编辑（逐条记录已省略）";

/// 统一错误载体：code 为契约错误码，message 为中文用户可读文案。
#[derive(Debug)]
pub struct SidecarError {
    pub code: ErrorCode,
    pub message: String,
    pub details: Value,
}

impl SidecarError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        SidecarError {
            code,
            message: message.into(),
            details: Value::Null,
        }
    }

    pub fn with_details(code: ErrorCode, message: impl Into<String>, details: Value) -> Self {
        SidecarError {
            code,
            message: message.into(),
            details,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        SidecarError::new(ErrorCode::Internal, message)
    }

    pub fn into_body(self) -> ErrorBody {
        ErrorBody {
            code: self.code,
            message: self.message,
            details: self.details,
        }
    }
}

impl From<GitError> for SidecarError {
    fn from(e: GitError) -> Self {
        let code = match &e {
            GitError::PathInvalid(_) => ErrorCode::WorkspacePathInvalid,
            GitError::NotReadable(_) => ErrorCode::WorkspaceNotReadable,
            GitError::NotGit(_) => ErrorCode::WorkspaceNotGit,
            GitError::Command(_) => ErrorCode::Internal,
        };
        SidecarError::new(code, e.to_string())
    }
}

impl From<FsError> for SidecarError {
    fn from(error: FsError) -> Self {
        match error {
            FsError::OutsideWorkspace(path) => SidecarError::with_details(
                ErrorCode::FsPathOutsideWorkspace,
                format!("路径超出工作区范围：{path}"),
                json!({"path": path}),
            ),
            FsError::NotFound(path) => SidecarError::with_details(
                ErrorCode::FsNotFound,
                format!("路径不存在：{path}"),
                json!({"path": path}),
            ),
            FsError::AlreadyExists(path) => SidecarError::with_details(
                ErrorCode::FsAlreadyExists,
                format!("目标已存在：{path}"),
                json!({"path": path}),
            ),
            FsError::TooLarge { size } => SidecarError::with_details(
                ErrorCode::FsTooLarge,
                "文件内容超过 8 MiB 上限",
                json!({"size": size, "max": fs::limits::FS_READ_MAX_BYTES}),
            ),
            FsError::Binary { size } => SidecarError::with_details(
                ErrorCode::FsBinary,
                "二进制文件不支持在编辑器中打开",
                json!({"size": size}),
            ),
            FsError::Conflict {
                current_hash,
                mtime,
            } => SidecarError::with_details(
                ErrorCode::FsConflict,
                "文件内容已被外部修改，请重新加载后再保存",
                json!({"current_hash": current_hash, "mtime": mtime}),
            ),
            FsError::GitProtected(path) => SidecarError::with_details(
                ErrorCode::FsGitProtected,
                format!(".git 目录受只读保护：{path}"),
                json!({"path": path}),
            ),
            FsError::InvalidName(message) => SidecarError::new(ErrorCode::InvalidParams, message),
            FsError::Io(message) => SidecarError::internal(message),
        }
    }
}

impl From<halo_store::StoreError> for SidecarError {
    fn from(e: halo_store::StoreError) -> Self {
        SidecarError::internal(format!("本地存储操作失败：{e}"))
    }
}

impl From<halo_config::CredentialError> for SidecarError {
    fn from(e: halo_config::CredentialError) -> Self {
        use halo_config::CredentialError as CE;
        match e {
            CE::StoreUnavailable => SidecarError::new(
                ErrorCode::CredentialStoreUnavailable,
                "操作系统凭据存储不可用，操作已失败关闭",
            ),
            CE::NotFound => SidecarError::new(ErrorCode::CredentialNotFound, "凭据引用不存在"),
            CE::Backend(msg) => SidecarError::internal(format!("凭据存储后端错误：{msg}")),
        }
    }
}

impl From<halo_config::ConfigError> for SidecarError {
    fn from(e: halo_config::ConfigError) -> Self {
        SidecarError::new(ErrorCode::InvalidParams, e.to_string())
    }
}

impl From<halo_runtime::RuntimeError> for SidecarError {
    fn from(e: halo_runtime::RuntimeError) -> Self {
        use halo_runtime::RuntimeError as RE;
        let code = match &e {
            RE::Spawn(_) | RE::Probe(_) => ErrorCode::RuntimeProbeFailed,
            RE::NotReady(_) | RE::InvalidState | RE::Unauthorized => ErrorCode::RuntimeNotReady,
            RE::VersionMismatch(_) => ErrorCode::RuntimeVersionMismatch,
            RE::CapabilityUnavailable(_) => ErrorCode::RuntimeCapabilityUnavailable,
            RE::Io(_) => ErrorCode::Internal,
        };
        SidecarError::new(code, e.to_string())
    }
}

impl From<EventGapError> for SidecarError {
    fn from(e: EventGapError) -> Self {
        SidecarError::with_details(
            ErrorCode::EventGap,
            "事件缓冲不足以覆盖请求的 after_seq，请整体重建视图",
            json!({"after_seq": e.after_seq, "oldest_available_seq": e.oldest}),
        )
    }
}

/// 请求处理上下文。
pub struct Ctx {
    pub store: Arc<Store>,
    pub cred: Arc<dyn CredentialStore>,
    pub bus: Arc<EventBus>,
    pub app: Arc<Mutex<AppState>>,
    pub timeouts: Timeouts,
}

pub struct Dispatcher {
    hello_done: bool,
    ctx: Ctx,
}

fn parse<T: DeserializeOwned>(params: Value) -> Result<T, SidecarError> {
    serde_json::from_value(params).map_err(|e| {
        SidecarError::new(ErrorCode::InvalidParams, format!("参数不符合方法契约：{e}"))
    })
}

fn ok<T: Serialize>(value: &T) -> Result<Value, SidecarError> {
    serde_json::to_value(value).map_err(|e| SidecarError::internal(format!("结果序列化失败：{e}")))
}

impl Dispatcher {
    pub fn new(ctx: Ctx) -> Self {
        Dispatcher {
            hello_done: false,
            ctx,
        }
    }

    pub fn bus(&self) -> &Arc<EventBus> {
        &self.ctx.bus
    }

    pub fn dispatch(&mut self, req: RequestEnvelope) -> Response {
        match self.handle(&req.method, req.params) {
            Ok(result) => Response::success(req.id, result),
            Err(e) => Response::failure(req.id, e.into_body()),
        }
    }

    fn handle(&mut self, method: &str, params: Value) -> Result<Value, SidecarError> {
        if method == "sidecar.hello" {
            return self.hello(params);
        }
        // hello 门禁：未握手前一律拒绝
        if !self.hello_done {
            return Err(SidecarError::new(
                ErrorCode::HelloRequired,
                "必须先调用 sidecar.hello 完成握手",
            ));
        }
        match method {
            "workspace.open" => self.workspace_open(params),
            "workspace.trust" => self.workspace_trust(params),
            "workspace.close" => self.workspace_close(params),
            "workspace.status" => self.workspace_status(params),
            "config.list" => self.config_list(params),
            "config.save" => self.config_save(params),
            "config.delete" => self.config_delete(params),
            "config.credential_check" => self.config_credential_check(params),
            "runtime.probe" => self.runtime_probe(params),
            "runtime.start" => self.runtime_start(params),
            "runtime.stop" => self.runtime_stop(params),
            "runtime.status" => self.runtime_status(params),
            "task.create" => self.task_create(params),
            "task.cancel" => self.task_cancel(params),
            "task.mark_manual_edit" => self.task_mark_manual_edit(params),
            "task.mark_verification" => self.task_mark_verification(params),
            "task.status" => self.task_status(params),
            "task.snapshot" => self.task_snapshot(params),
            "review.get" => self.review_get(params),
            "delivery.accept" => self.delivery_decide(params, true),
            "delivery.reject" => self.delivery_decide(params, false),
            "handoff.preview" => self.handoff_preview(params),
            "handoff.create" => self.handoff_create(params),
            "history.list" => self.history_list(params),
            "history.evidence" => self.history_evidence(params),
            "fs.list" => self.fs_list(params),
            "fs.read" => self.fs_read(params),
            "fs.write" => self.fs_write(params),
            "fs.create_file" => self.fs_create_file(params),
            "fs.create_dir" => self.fs_create_dir(params),
            "fs.rename" => self.fs_rename(params),
            "fs.stat" => self.fs_stat(params),
            "fs.search" => self.fs_search(params),
            other => Err(SidecarError::new(
                ErrorCode::MethodNotFound,
                format!("未知方法：{other}"),
            )),
        }
    }

    // ---------- 握手 ----------

    fn hello(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::HelloParams = parse(params)?;
        if !p.app_protocol_versions.contains(&PROTOCOL_VERSION) {
            return Err(SidecarError::with_details(
                ErrorCode::ProtocolVersionUnsupported,
                "应用与 Sidecar 没有公共协议版本",
                json!({"sidecar_protocol_versions": [PROTOCOL_VERSION]}),
            ));
        }
        self.hello_done = true;
        ok(&methods::HelloResult {
            protocol_version: PROTOCOL_VERSION,
            sidecar_version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: CAPABILITIES.iter().map(|s| s.to_string()).collect(),
        })
    }

    // ---------- workspace.* ----------

    fn workspace_open(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::workspace::OpenWorkspaceParams = parse(params)?;
        let probe = GitClient::validate_workspace(&p.path)?;

        {
            let app = lock(&self.ctx.app);
            if app.has_running_task() {
                return Err(SidecarError::new(
                    ErrorCode::TaskRunning,
                    "存在运行中的任务，无法切换工作区",
                ));
            }
        }
        // 无运行中任务：自动停止旧运行时并切换
        self.stop_all_runtimes();

        let saved = self.ctx.store.get_trust(&probe.real_path)?;
        let saved_core = saved.map(|r| halo_core::TrustRecord {
            real_path: r.real_path,
            root_commit: r.root_commit,
            trusted: r.trusted,
            decided_at: r.decided_at,
        });
        let evaluation = halo_core::evaluate_trust(
            saved_core.as_ref(),
            &halo_core::WorkspaceIdentity {
                real_path: probe.real_path.clone(),
                root_commit: probe.root_commit.clone(),
            },
        );

        let ws = ActiveWorkspace {
            workspace_id: format!("ws-{}", uuid::Uuid::new_v4()),
            real_path: probe.real_path,
            git_root: probe.git_root,
            root_commit: probe.root_commit,
            trust: evaluation.state,
            identity_changed: evaluation.identity_changed,
        };
        let status = workspace_status_dto(&ws);
        {
            let mut app = lock(&self.ctx.app);
            app.workspace = Some(ws);
            app.task = None;
        }
        self.emit_workspace_changed(&status)?;
        ok(&status)
    }

    fn workspace_trust(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::workspace::TrustWorkspaceParams = parse(params)?;
        let mut ws = {
            let app = lock(&self.ctx.app);
            app.workspace.clone().ok_or_else(|| {
                SidecarError::new(ErrorCode::WorkspaceNotActive, "当前没有活动工作区")
            })?
        };
        if ws.workspace_id != p.workspace_id {
            return Err(SidecarError::new(
                ErrorCode::WorkspaceNotActive,
                "workspace_id 与当前活动工作区不一致",
            ));
        }
        match p.decision {
            methods::workspace::TrustDecision::Trust => {
                self.ctx.store.put_trust(&halo_store::TrustRecord {
                    real_path: ws.real_path.clone(),
                    root_commit: ws.root_commit.clone(),
                    trusted: true,
                    decided_at: now_ts(),
                })?;
                ws.trust = halo_core::TrustState::Trusted;
                ws.identity_changed = false;
            }
            methods::workspace::TrustDecision::Revoke => {
                // revoke 立即停止并清理该工作区全部受管运行时
                self.stop_all_runtimes();
                self.ctx.store.revoke_trust(&ws.real_path)?;
                ws.trust = halo_core::TrustState::Untrusted;
                ws.identity_changed = false;
            }
        }
        let status = workspace_status_dto(&ws);
        lock(&self.ctx.app).workspace = Some(ws);
        self.emit_workspace_changed(&status)?;
        ok(&status)
    }

    fn workspace_close(&mut self, params: Value) -> Result<Value, SidecarError> {
        let _: methods::workspace::CloseWorkspaceParams = parse(params)?;
        {
            let app = lock(&self.ctx.app);
            if app.has_running_task() {
                return Err(SidecarError::new(
                    ErrorCode::TaskRunning,
                    "存在运行中的任务，无法关闭工作区",
                ));
            }
        }
        self.stop_all_runtimes();
        {
            let mut app = lock(&self.ctx.app);
            app.workspace = None;
            app.task = None;
        }
        self.ctx
            .bus
            .emit(None, "workspace.changed", json!({"active": false}));
        ok(&methods::workspace::CloseWorkspaceResult { closed: true })
    }

    fn workspace_status(&mut self, params: Value) -> Result<Value, SidecarError> {
        let _: methods::workspace::WorkspaceStatusParams = parse(params)?;
        let app = lock(&self.ctx.app);
        match &app.workspace {
            Some(ws) => ok(&workspace_status_dto(ws)),
            None => Ok(json!({"active": false})),
        }
    }

    fn emit_workspace_changed(
        &self,
        status: &methods::workspace::WorkspaceStatus,
    ) -> Result<(), SidecarError> {
        let payload = ok(status)?;
        self.ctx.bus.emit(None, "workspace.changed", payload);
        Ok(())
    }

    fn stop_all_runtimes(&self) {
        for agent in [AgentKind::Pi, AgentKind::OpenCode] {
            crate::state::stop_slot(
                &self.ctx.app,
                &self.ctx.bus,
                agent,
                self.ctx.timeouts.shutdown_grace,
            );
        }
    }

    // ---------- fs.* ----------

    fn trusted_workspace_root(&self) -> Result<String, SidecarError> {
        let app = lock(&self.ctx.app);
        let workspace = app.workspace.as_ref().ok_or_else(|| {
            SidecarError::new(ErrorCode::WorkspaceNotActive, "当前没有活动工作区")
        })?;
        if !workspace.is_trusted() {
            return Err(SidecarError::new(
                ErrorCode::WorkspaceNotTrusted,
                "工作区未确认信任，无法访问工作区文件",
            ));
        }
        Ok(workspace.real_path.clone())
    }

    fn fs_list(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::fs::FsListParams = parse(params)?;
        let root = self.trusted_workspace_root()?;
        ok(&fs::ops::list(Path::new(&root), &p.path, p.depth)?)
    }

    fn fs_read(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::fs::FsReadParams = parse(params)?;
        let root = self.trusted_workspace_root()?;
        ok(&fs::ops::read(Path::new(&root), &p.path)?)
    }

    fn fs_write(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::fs::FsWriteParams = parse(params)?;
        let root = self.trusted_workspace_root()?;
        let result = fs::ops::write(
            Path::new(&root),
            &p.path,
            &p.content,
            &p.expected_hash,
            p.encoding,
        )?;
        self.record_fs_manual_edit(ManualEditOp::Write, &result.path, None);
        ok(&result)
    }

    fn fs_create_file(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::fs::FsCreateFileParams = parse(params)?;
        let root = self.trusted_workspace_root()?;
        let entry = fs::ops::create_file(Path::new(&root), &p.path, &p.content)?;
        self.record_fs_manual_edit(ManualEditOp::CreateFile, &entry.path, None);
        ok(&methods::fs::FsEntryResult { entry })
    }

    fn fs_create_dir(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::fs::FsCreateDirParams = parse(params)?;
        let root = self.trusted_workspace_root()?;
        let entry = fs::ops::create_dir(Path::new(&root), &p.path)?;
        self.record_fs_manual_edit(ManualEditOp::CreateDir, &entry.path, None);
        ok(&methods::fs::FsEntryResult { entry })
    }

    fn fs_rename(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::fs::FsRenameParams = parse(params)?;
        let root = self.trusted_workspace_root()?;
        let entry = fs::ops::rename(Path::new(&root), &p.from, &p.to)?;
        self.record_fs_manual_edit(ManualEditOp::Rename, &p.from, Some(&entry.path));
        ok(&methods::fs::FsEntryResult { entry })
    }

    fn fs_stat(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::fs::FsStatParams = parse(params)?;
        let root = self.trusted_workspace_root()?;
        let entry = fs::ops::stat(Path::new(&root), &p.path)?;
        ok(&methods::fs::FsEntryResult { entry })
    }

    fn fs_search(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::fs::FsSearchParams = parse(params)?;
        let root = self.trusted_workspace_root()?;
        let git = GitClient::new(&root);
        ok(&fs::search::search(Path::new(&root), &git, &p)?)
    }

    /// 保存成功后才尝试记录人工介入；证据存储失败不得反向让文件保存失败。
    /// 同一路径只进入归因事实一次，但每次成功写入都会推送过程事件。
    fn record_fs_manual_edit(&self, operation: ManualEditOp, path: &str, to_path: Option<&str>) {
        let note = manual_edit_note(operation, path, to_path, &local_hhmm());
        if self
            .mark_manual_edit_internal(
                None,
                note.clone(),
                note,
                "fs_write",
                to_path.or(Some(path)),
                true,
            )
            .is_err()
        {
            eprintln!("[halo-sidecar] 人工介入归因未能写入本地历史");
        }
    }

    /// 在内存、持久化记录和过程事件之间原子地推进一条人工介入事实。
    /// `require_active_state` 用于 fs 写入：审查就绪后的保存不再改变已定稿证据。
    fn mark_manual_edit_internal(
        &self,
        expected_task_id: Option<&str>,
        attribution_note: String,
        event_note: String,
        source: &'static str,
        path: Option<&str>,
        require_active_state: bool,
    ) -> Result<bool, SidecarError> {
        let event = {
            let mut app = lock(&self.ctx.app);
            let Some(task) = app.task.as_mut().filter(|task| {
                expected_task_id.is_none_or(|task_id| task.task_id == task_id)
            }) else {
                return match expected_task_id {
                    Some(_) => Err(SidecarError::new(
                        ErrorCode::TaskNotFound,
                        "任务不存在或已结束，无法标记人工介入",
                    )),
                    None => Ok(false),
                };
            };
            let state_allowed = if require_active_state {
                matches!(
                    task.state,
                    halo_core::TaskState::Created
                        | halo_core::TaskState::Running
                        | halo_core::TaskState::WaitingDeveloper
                        | halo_core::TaskState::AwaitingAction
                        | halo_core::TaskState::Finishing
                )
            } else {
                !task.state.is_terminal()
            };
            if !state_allowed {
                return match expected_task_id {
                    Some(_) => Err(SidecarError::new(
                        ErrorCode::TaskNotFound,
                        "任务不存在或已结束，无法标记人工介入",
                    )),
                    None => Ok(false),
                };
            }

            let event_path = path.map(str::to_owned);
            let previous_attribution = task.attribution.clone();
            let previous_paths = task.manual_edit_paths.clone();
            let is_new_path = event_path
                .as_ref()
                .map(|path| task.manual_edit_paths.insert(path.clone()))
                .unwrap_or(true);
            task.attribution = if event_path.is_some() {
                attribution_after_manual_edit(&task.attribution, is_new_path, &attribution_note)
            } else {
                task
                    .attribution
                    .clone()
                    .with_manual_edit(attribution_note.clone())
            };
            if event_path.is_none() || is_new_path {
                let record = task.to_record();
                if let Err(error) = self.ctx.store.put_task(&record) {
                    task.attribution = previous_attribution;
                    task.manual_edit_paths = previous_paths;
                    return Err(error.into());
                }
            }
            (task.task_id.clone(), event_path)
        };
        let (task_id, event_path) = event;
        self.ctx.bus.emit(
            Some(&task_id),
            "task.manual_edit",
            json!({"note": event_note, "source": source, "path": event_path}),
        );
        Ok(true)
    }

    // ---------- config.* ----------

    fn config_list(&mut self, params: Value) -> Result<Value, SidecarError> {
        let _: methods::config::ListConfigsParams = parse(params)?;
        let configs: Vec<_> = self
            .ctx
            .store
            .list_configs()?
            .iter()
            .map(mapping::config_record_to_dto)
            .collect();
        ok(&methods::config::ListConfigsResult { configs })
    }

    fn config_save(&mut self, params: Value) -> Result<Value, SidecarError> {
        let input: methods::config::LaunchConfigInput = parse(params)?;
        let now = now_ts();
        let config_id = format!("cfg-{}", uuid::Uuid::new_v4());

        // 领域校验（含 env 白名单）
        let domain = halo_config::LaunchConfig {
            id: config_id.clone(),
            name: input.name.clone(),
            agent: mapping::agent_dto_to_domain(input.agent),
            executable_path: input.executable_path.clone(),
            model: input.model.clone(),
            thinking_level: match input.thinking_level {
                methods::config::ThinkingLevel::Off => halo_config::ThinkingLevel::Off,
                methods::config::ThinkingLevel::Low => halo_config::ThinkingLevel::Low,
                methods::config::ThinkingLevel::Medium => halo_config::ThinkingLevel::Medium,
                methods::config::ThinkingLevel::High => halo_config::ThinkingLevel::High,
            },
            credential_ref: input.credential_ref.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        halo_config::validate_launch_config(&domain)?;

        // 凭据存储不可用时失败关闭（仅当配置引用凭据）
        if input.credential_ref.is_some() && !self.ctx.cred.available() {
            return Err(SidecarError::new(
                ErrorCode::CredentialStoreUnavailable,
                "操作系统凭据存储不可用，包含凭据引用的配置保存已失败关闭",
            ));
        }

        let record = halo_store::LaunchConfigRecord {
            config_id,
            name: input.name,
            agent: mapping::agent_dto_to_domain(input.agent).as_str().to_string(),
            executable_path: input.executable_path,
            model: input.model,
            thinking_level: mapping::thinking_dto_to_str(input.thinking_level).to_string(),
            credential_ref: input.credential_ref,
            created_at: now.clone(),
            updated_at: now,
        };
        self.ctx.store.put_config(&record)?;
        ok(&methods::config::SaveConfigResult {
            config: mapping::config_record_to_dto(&record),
        })
    }

    fn config_delete(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::config::DeleteConfigParams = parse(params)?;
        if !self.ctx.store.delete_config(&p.config_id)? {
            return Err(SidecarError::new(
                ErrorCode::ConfigNotFound,
                format!("启动配置不存在：{}", p.config_id),
            ));
        }
        ok(&methods::config::DeleteConfigResult { deleted: true })
    }

    fn config_credential_check(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::config::CredentialCheckParams = parse(params)?;
        let store_available = self.ctx.cred.available();
        let exists = if store_available {
            self.ctx.cred.exists(&p.credential_ref).unwrap_or(false)
        } else {
            false
        };
        ok(&methods::config::CredentialCheckResult {
            exists,
            store_available,
        })
    }

    fn find_config(&self, config_id: &str) -> Result<halo_store::LaunchConfigRecord, SidecarError> {
        self.ctx
            .store
            .list_configs()?
            .into_iter()
            .find(|c| c.config_id == config_id)
            .ok_or_else(|| {
                SidecarError::new(
                    ErrorCode::ConfigNotFound,
                    format!("启动配置不存在：{config_id}"),
                )
            })
    }

    fn record_runtime_failure(
        &self,
        agent: AgentKind,
        version: Option<String>,
        reason: impl Into<String>,
        recovery_hint: impl Into<String>,
    ) {
        let state = RuntimeState::Failed {
            reason: reason.into(),
            recovery_hint: recovery_hint.into(),
        };
        let (payload, changed) = {
            let mut app = lock(&self.ctx.app);
            let slot = app.slot_mut(agent);
            slot.version = version;
            let changed = slot.last_state != state;
            slot.last_state = state.clone();
            let payload = mapping::runtime_state_payload(agent, &state, slot.version.clone());
            (payload, changed)
        };
        if changed {
            self.ctx.bus.emit(None, "runtime.state", payload);
        }
    }

    // ---------- runtime.* ----------

    fn runtime_probe(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::runtime::RuntimeProbeParams = parse(params)?;
        let config = self.find_config(&p.config_id)?;
        let agent = mapping::agent_dto_to_domain(p.agent);
        if config.agent != agent.as_str() {
            return Err(SidecarError::new(
                ErrorCode::InvalidParams,
                "agent 与所选配置的受管应用不一致",
            ));
        }
        let version = match agent {
            AgentKind::Pi => PiRuntime::probe(&config.executable_path),
            AgentKind::OpenCode => OpenCodeRuntime::probe(&config.executable_path),
        }
        .map_err(|e| SidecarError::new(ErrorCode::RuntimeProbeFailed, e.to_string()))?;
        let supported = match agent {
            AgentKind::Pi => true,
            AgentKind::OpenCode => OpenCodeRuntime::is_compatible_version(&version),
        };
        lock(&self.ctx.app).slot_mut(agent).version = Some(version.clone());
        ok(&methods::runtime::RuntimeProbeResult {
            agent: p.agent,
            version,
            supported,
        })
    }

    fn runtime_start(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::runtime::RuntimeStartParams = parse(params)?;
        let config = self.find_config(&p.config_id)?;
        let agent = mapping::agent_dto_to_domain(p.agent);
        if config.agent != agent.as_str() {
            return Err(SidecarError::new(
                ErrorCode::InvalidParams,
                "agent 与所选配置的受管应用不一致",
            ));
        }

        let (cwd, previous_handle, runtime_generation) = {
            let mut app = lock(&self.ctx.app);
            let cwd = {
                let ws = app.workspace.as_ref().ok_or_else(|| {
                    SidecarError::new(ErrorCode::WorkspaceNotActive, "没有活动工作区，无法启动运行时")
                })?;
                if !ws.is_trusted() {
                    return Err(SidecarError::new(
                        ErrorCode::WorkspaceNotTrusted,
                        "工作区未确认信任，无法启动受管运行时",
                    ));
                }
                ws.real_path.clone()
            };
            let slot = app.slot_mut(agent);
            if matches!(
                slot.effective_state(),
                RuntimeState::Starting | RuntimeState::Ready
            ) {
                return Err(SidecarError::new(
                    ErrorCode::RuntimeAlreadyRunning,
                    "该受管应用已在运行",
                ));
            }
            // 失败的运行时可以重试。先脱离其终态句柄，避免第二次失败后 runtime.status 仍报告旧原因。
            let runtime_generation = slot.advance_generation();
            (cwd, slot.handle.take(), runtime_generation)
        };
        drop(previous_handle);

        if agent == AgentKind::OpenCode && config.credential_ref.is_none() {
            self.record_runtime_failure(
                agent,
                None,
                "OpenCode 启动配置缺少凭据引用，已失败关闭",
                "请先在系统凭据存储中录入密钥，并为 OpenCode 配置选择对应的凭据引用后重试",
            );
            return Err(SidecarError::new(
                ErrorCode::CredentialNotFound,
                "OpenCode 启动配置缺少凭据引用，无法启动受管运行时",
            ));
        }

        let credential_env_var = match credential_env_var_for(agent, &config.model) {
            Ok(env_var) => env_var,
            Err(_) => {
                if agent == AgentKind::OpenCode {
                    self.record_runtime_failure(
                        agent,
                        None,
                        "OpenCode 模型的 Provider 凭据映射不受支持，启动已失败关闭",
                        "请将模型填写为受支持的 provider/model 形式，例如 openai/gpt-5，然后重新启动",
                    );
                }
                return Err(SidecarError::new(
                    ErrorCode::InvalidParams,
                    "OpenCode 模型必须使用受支持的 provider/model 形式，无法安全选择凭据环境变量",
                ));
            }
        };

        // 先解析凭据引用，避免在凭据不可用时执行任意配置路径；没有明文回退。
        let host: HashMap<String, String> = std::env::vars().collect();
        let mut injected: Vec<(String, halo_config::Secret)> = Vec::new();
        if let Some(ref_name) = &config.credential_ref {
            if !self.ctx.cred.available() {
                if agent == AgentKind::OpenCode {
                    self.record_runtime_failure(
                        agent,
                        None,
                        "操作系统凭据存储不可用，OpenCode 启动已失败关闭",
                        "请恢复系统凭据存储后重新启动；不会回退到明文凭据",
                    );
                }
                return Err(SidecarError::new(
                    ErrorCode::CredentialStoreUnavailable,
                    "操作系统凭据存储不可用，启动已失败关闭",
                ));
            }
            let secret = match self.ctx.cred.get(ref_name) {
                Ok(secret) => secret,
                Err(error) => {
                    if agent == AgentKind::OpenCode {
                        self.record_runtime_failure(
                            agent,
                            None,
                            "OpenCode 所需的凭据引用不可用",
                            "请检查凭据引用是否存在于操作系统凭据存储后重新启动",
                        );
                    }
                    return Err(error.into());
                }
            };
            injected.push((credential_env_var.to_string(), secret));
        }

        // 真实探测：版本必须可读；OpenCode 只允许已知稳定 1.x 兼容性档案。
        let version = match agent {
            AgentKind::Pi => PiRuntime::probe(&config.executable_path),
            AgentKind::OpenCode => OpenCodeRuntime::probe(&config.executable_path),
        };
        let version = match version {
            Ok(version) => version,
            Err(error) => {
                if agent == AgentKind::OpenCode {
                    self.record_runtime_failure(
                        agent,
                        None,
                        "无法探测 OpenCode 版本",
                        "请确认 OpenCode 可执行文件有效后重新探测或启动",
                    );
                }
                return Err(SidecarError::new(ErrorCode::RuntimeProbeFailed, error.to_string()));
            }
        };
        if agent == AgentKind::OpenCode && !OpenCodeRuntime::is_compatible_version(&version) {
            self.record_runtime_failure(
                agent,
                Some(version),
                "OpenCode 版本不受兼容性档案支持（RUNTIME_VERSION_MISMATCH）：需要稳定版 1.18.5 或更高的 1.x",
                "请安装稳定版 OpenCode 1.18.5 或更高的 1.x 版本后重新启动",
            );
            return Err(SidecarError::new(
                ErrorCode::RuntimeVersionMismatch,
                "OpenCode 版本不受兼容性档案支持：需要稳定版 1.18.5 或更高的 1.x",
            ));
        }

        {
            let mut app = lock(&self.ctx.app);
            app.slot_mut(agent).version = Some(version.clone());
        }

        // 子进程环境 = 固定白名单 + 启动瞬间注入的凭据；没有配置层环境覆盖。
        let env = halo_config::build_child_env(&host, injected);

        let cmd = halo_runtime::LaunchCmd {
            exe: config.executable_path.clone(),
            env,
            cwd,
        };

        let (tx, rx) = unbounded();
        crate::state::spawn_runtime_forwarder(
            Arc::clone(&self.ctx.app),
            Arc::clone(&self.ctx.bus),
            agent,
            runtime_generation,
            rx,
        );

        let handle: Arc<dyn AgentHandle> = match agent {
            AgentKind::Pi => Arc::new(PiRuntime::start(cmd, tx, self.ctx.timeouts)?),
            AgentKind::OpenCode => Arc::new(OpenCodeRuntime::start(cmd, tx, self.ctx.timeouts)?),
        };
        let state = handle.state();
        lock(&self.ctx.app).slot_mut(agent).handle = Some(handle);
        ok(&json!({"state": mapping::runtime_state_to_info(&state, None).state}))
    }

    fn runtime_stop(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::runtime::RuntimeStopParams = parse(params)?;
        let agent = mapping::agent_dto_to_domain(p.agent);
        let handle = {
            let mut app = lock(&self.ctx.app);
            let slot = app.slot_mut(agent);
            slot.task_tx = None;
            slot.advance_generation();
            slot.handle.take()
        };
        let state = match handle {
            Some(h) => {
                h.stop(self.ctx.timeouts.shutdown_grace);
                crate::state::mark_slot_stopped(&self.ctx.app, &self.ctx.bus, agent);
                RuntimeState::Stopped
            }
            None => lock(&self.ctx.app).slot(agent).effective_state(),
        };
        ok(&json!({"state": mapping::runtime_state_to_info(&state, None).state}))
    }

    fn runtime_status(&mut self, params: Value) -> Result<Value, SidecarError> {
        let _: methods::runtime::RuntimeStatusParams = parse(params)?;
        let app = lock(&self.ctx.app);
        ok(&methods::runtime::RuntimeStatusResult {
            pi: mapping::runtime_state_to_info(&app.pi.effective_state(), app.pi.version.clone()),
            opencode: mapping::runtime_state_to_info(
                &app.opencode.effective_state(),
                app.opencode.version.clone(),
            ),
        })
    }

    // ---------- task.* ----------

    fn task_create(&mut self, params: Value) -> Result<Value, SidecarError> {
        let spec: methods::task::TaskSpec = parse(params)?;
        let agent = mapping::agent_dto_to_domain(spec.agent);

        let (git_root, handle) = {
            let app = lock(&self.ctx.app);
            let ws = app.workspace.as_ref().ok_or_else(|| {
                SidecarError::new(ErrorCode::WorkspaceNotActive, "没有活动工作区，无法创建任务")
            })?;
            if !ws.is_trusted() {
                return Err(SidecarError::new(
                    ErrorCode::WorkspaceNotTrusted,
                    "工作区未确认信任，无法创建任务",
                ));
            }
            if app.has_running_task() {
                return Err(SidecarError::new(
                    ErrorCode::TaskAlreadyRunning,
                    "已存在运行中的任务；一个活动工作区同一时刻只允许一个任务",
                ));
            }
            let slot = app.slot(agent);
            let handle = match (&slot.handle, slot.effective_state()) {
                (Some(h), RuntimeState::Ready) => Arc::clone(h),
                _ => {
                    return Err(SidecarError::new(
                        ErrorCode::RuntimeNotReady,
                        format!("受管应用 {} 尚未就绪，无法创建任务", agent.as_str()),
                    ))
                }
            };
            (ws.git_root.clone(), handle)
        };

        let config = self.find_config(&spec.config_id)?;
        if config.agent != agent.as_str() {
            return Err(SidecarError::new(
                ErrorCode::InvalidParams,
                "agent 与所选配置的受管应用不一致",
            ));
        }

        // 交接接续：校验交接包存在，并把有限上下文并入任务输入
        let mut notes = spec.notes.clone();
        let mut base_diff = spec.base_diff.clone();
        if let Some(handoff_id) = &spec.handoff_id {
            let handoff = self.ctx.store.get_handoff(handoff_id)?.ok_or_else(|| {
                SidecarError::new(
                    ErrorCode::HandoffNotFound,
                    format!("交接包不存在：{handoff_id}"),
                )
            })?;
            let mut section = format!(
                "【交接包 {}】\n目标：{}\n上一 Agent 摘要：{}\n验证结论：{}（{}）",
                handoff.handoff_id,
                handoff.goal,
                handoff.summary,
                handoff.verification_status,
                handoff.verification_detail
            );
            if let Some(existing) = &notes {
                section = format!("{existing}\n\n{section}");
            }
            notes = Some(section);
            if base_diff.is_none() && !handoff.selected_changes.is_empty() {
                let combined: Vec<String> = handoff
                    .selected_changes
                    .iter()
                    .map(|c| c.diff.clone())
                    .collect();
                base_diff = Some(combined.join("\n"));
            }
        }

        let flow = self.flow_ctx();
        let status = task_flow::start_task(
            &flow,
            &git_root,
            handle,
            task_flow::StartArgs {
                agent,
                title: spec.title,
                instructions: spec.instructions,
                files: spec.files,
                base_diff,
                notes,
            },
        )?;
        ok(&methods::task::CreateTaskResult { task: status })
    }

    fn flow_ctx(&self) -> FlowCtx {
        FlowCtx {
            bus: Arc::clone(&self.ctx.bus),
            store: Arc::clone(&self.ctx.store),
            app: Arc::clone(&self.ctx.app),
            timeouts: self.ctx.timeouts,
        }
    }

    fn task_cancel(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::task::CancelTaskParams = parse(params)?;
        let app = lock(&self.ctx.app);
        let task = app
            .task
            .as_ref()
            .filter(|t| t.task_id == p.task_id)
            .ok_or_else(|| {
                SidecarError::new(ErrorCode::TaskNotFound, format!("任务不存在：{}", p.task_id))
            })?;
        if task.state.is_terminal() {
            return ok(&methods::task::CancelTaskResult { accepted: false });
        }
        let accepted = task
            .cancel_tx
            .as_ref()
            .map(task_flow::request_cancel)
            .unwrap_or(false);
        ok(&methods::task::CancelTaskResult { accepted })
    }

    fn task_mark_manual_edit(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::task::MarkManualEditParams = parse(params)?;
        let note = halo_core::sanitize(&p.note);
        let persisted_note = format!("{}：{}", now_ts(), note);
        self.mark_manual_edit_internal(
            Some(&p.task_id),
            persisted_note,
            note,
            "user_marked",
            None,
            false,
        )?;
        ok(&methods::task::MarkManualEditResult {
            attribution: methods::Attribution::Mixed,
        })
    }

    fn task_mark_verification(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::task::MarkVerificationParams = parse(params)?;
        if p.status != methods::VerificationStatus::NotRun {
            return Err(SidecarError::new(
                ErrorCode::InvalidParams,
                "用户只能显式标记验证为未执行（not_run）",
            ));
        }
        let note = halo_core::sanitize(&p.note);
        {
            let mut app = lock(&self.ctx.app);
            let task = app
                .task
                .as_mut()
                .filter(|t| t.task_id == p.task_id)
                .ok_or_else(|| {
                    SidecarError::new(ErrorCode::TaskNotFound, format!("任务不存在：{}", p.task_id))
                })?;
            task.verification_user = Some(halo_core::Verification::user_marked_not_run(note.clone()));
        }
        self.ctx.bus.emit(
            Some(&p.task_id),
            "task.verification",
            json!({"status": "not_run", "detail": note, "source": "user_marked"}),
        );
        ok(&methods::task::MarkVerificationResult { ok: true })
    }

    fn task_status(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::task::TaskStatusParams = parse(params)?;
        let current = {
            let app = lock(&self.ctx.app);
            app.task.as_ref().map(|t| (t.task_id.clone(), t.to_status()))
        };
        let task = match p.task_id {
            None => current.map(|(_, s)| s),
            Some(id) => match current {
                Some((cur_id, status)) if cur_id == id => Some(status),
                _ => self.load_task_status(&id)?,
            },
        };
        ok(&methods::task::TaskStatusResult { task })
    }

    fn load_task_status(
        &self,
        task_id: &str,
    ) -> Result<Option<methods::task::TaskStatus>, SidecarError> {
        match self.ctx.store.get_task(task_id)? {
            None => Ok(None),
            Some(rec) => {
                let latest = self
                    .ctx
                    .store
                    .latest_evidence(task_id)?
                    .map(|e| e.version)
                    .unwrap_or(0);
                Ok(Some(mapping::task_record_to_status(&rec, latest)))
            }
        }
    }

    fn task_snapshot(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::task::TaskSnapshotParams = parse(params)?;
        let (last_seq, events) = self.ctx.bus.events_after(p.after_seq)?;
        let task = {
            let app = lock(&self.ctx.app);
            app.task.as_ref().map(|t| t.to_status())
        };
        ok(&methods::task::TaskSnapshotResult {
            task,
            last_seq,
            events,
            session_messages: {
                let app = lock(&self.ctx.app);
                app.task
                    .as_ref()
                    .map(|task| task.session_messages.clone())
                    .unwrap_or_default()
            },
        })
    }

    // ---------- review.* / delivery.* ----------

    fn review_get(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::review::ReviewGetParams = parse(params)?;
        let rec = self.require_task(&p.task_id)?;
        let state = mapping::task_state_from_str(&rec.state);
        if !state.is_reviewable() {
            return Err(SidecarError::new(
                ErrorCode::TaskNotReviewable,
                "任务尚未产生可审查交付",
            ));
        }
        let versions = self.ctx.store.list_evidence(&p.task_id)?;
        let latest_version = versions.last().map(|v| v.version).unwrap_or(0);
        if versions.is_empty() {
            return Err(SidecarError::new(
                ErrorCode::EvidenceNotFound,
                "该任务没有交付证据",
            ));
        }
        let record = match p.version {
            None => versions.last(),
            Some(v) => versions.iter().find(|e| e.version == v),
        }
        .ok_or_else(|| {
            SidecarError::new(ErrorCode::EvidenceNotFound, "指定的证据版本不存在")
        })?;
        ok(&mapping::evidence_record_to_bundle(
            record,
            &rec.manual_edit_paths,
            record.version == latest_version,
        ))
    }

    fn delivery_decide(&mut self, params: Value, accept: bool) -> Result<Value, SidecarError> {
        // accept 与 reject 的 params 只差 reason 字段；统一用 reject 形状解析
        let p: methods::review::DeliveryRejectParams = parse(params)?;
        let rec = self.require_task(&p.task_id)?;
        let latest = self.ctx.store.latest_evidence(&p.task_id)?.ok_or_else(|| {
            SidecarError::new(ErrorCode::EvidenceNotFound, "该任务没有交付证据")
        })?;
        if p.evidence_version != latest.version {
            return Err(SidecarError::with_details(
                ErrorCode::EvidenceNotLatest,
                "只有最新的证据版本可以做出结论",
                json!({"latest_version": latest.version}),
            ));
        }
        let state = mapping::task_state_from_str(&rec.state);
        let ev = if accept {
            halo_core::TaskEvent::Accept
        } else {
            halo_core::TaskEvent::Reject
        };
        let next = state.apply(&ev).map_err(|_| {
            if !state.is_reviewable() && !state.is_terminal() {
                SidecarError::new(ErrorCode::TaskStillRunning, "任务仍在运行，无法做出交付结论")
            } else {
                SidecarError::new(
                    ErrorCode::TaskNotReviewable,
                    format!("任务状态 {} 不允许该结论", state.as_str()),
                )
            }
        })?;

        let mut updated = rec.clone();
        updated.state = next.as_str().to_string();
        if updated.ended_at.is_none() {
            updated.ended_at = Some(now_ts());
        }
        self.ctx.store.put_task(&updated)?;
        {
            let mut app = lock(&self.ctx.app);
            if let Some(task) = app.task.as_mut().filter(|t| t.task_id == p.task_id) {
                task.state = next;
                if task.ended_at.is_none() {
                    task.ended_at = updated.ended_at.clone();
                }
            }
        }

        let decision = halo_store::DecisionRecord {
            kind: if accept { "accepted" } else { "rejected" }.to_string(),
            task_id: p.task_id.clone(),
            evidence_version: latest.version,
            decided_at: now_ts(),
            reason: p.reason.as_deref().map(halo_core::sanitize),
            reason_truncated: false,
        };
        self.ctx.store.put_decision(&decision)?;

        let status = mapping::task_record_to_status(&updated, latest.version);
        let state_value = serde_json::to_value(status.state).unwrap_or(Value::Null);
        self.ctx.bus.emit(
            Some(&p.task_id),
            "task.state",
            json!({"state": state_value, "task": status}),
        );
        ok(&methods::review::DecisionResult {
            decision: mapping::decision_record_to_dto(&decision),
        })
    }

    fn require_task(&self, task_id: &str) -> Result<halo_store::TaskRecord, SidecarError> {
        self.ctx.store.get_task(task_id)?.ok_or_else(|| {
            SidecarError::new(ErrorCode::TaskNotFound, format!("任务不存在：{task_id}"))
        })
    }

    // ---------- handoff.* ----------

    fn handoff_source(
        &self,
        task_id: &str,
        selected: Option<&[String]>,
    ) -> Result<(halo_store::TaskRecord, halo_core::HandoffDraft), SidecarError> {
        let rec = self.require_task(task_id)?;
        let state = mapping::task_state_from_str(&rec.state);
        if !state.is_reviewable() {
            return Err(SidecarError::new(
                ErrorCode::TaskStillRunning,
                "任务尚未结束，交接只能发生在可审查交付之后",
            ));
        }
        let latest = self.ctx.store.latest_evidence(task_id)?.ok_or_else(|| {
            SidecarError::new(ErrorCode::EvidenceNotFound, "该任务没有交付证据，无法交接")
        })?;
        // store 记录 → core 证据类型：build_handoff 的白名单构造入口
        let evidence = evidence_record_to_core(&latest);
        let goal = if rec.goal.trim().is_empty() {
            // 旧 schema 的历史记录没有目标；保留可交接性，但不伪称标题就是完整任务说明。
            rec.title.clone()
        } else {
            rec.goal.clone()
        };
        let draft = halo_core::build_handoff(&evidence, &goal, selected);
        Ok((rec, draft))
    }

    fn handoff_preview(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::handoff::HandoffPreviewParams = parse(params)?;
        let (rec, draft) = self.handoff_source(&p.task_id, p.selected_files.as_deref())?;
        ok(&methods::handoff::HandoffPreviewResult {
            package: handoff_draft_to_package(&draft, &rec, None, None, None),
        })
    }

    fn handoff_create(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::handoff::HandoffCreateParams = parse(params)?;
        let (rec, draft) = self.handoff_source(&p.task_id, Some(&p.selected_files))?;
        let handoff_id = format!("ho-{}", uuid::Uuid::new_v4());
        let created_at = now_ts();
        let target = mapping::agent_dto_to_domain(p.target_agent);

        let record = halo_store::HandoffRecord {
            handoff_id: handoff_id.clone(),
            task_id: p.task_id.clone(),
            source_agent: rec.agent.clone(),
            target_agent: Some(target.as_str().to_string()),
            goal: draft.goal.clone(),
            goal_truncated: false,
            summary: draft.summary.clone(),
            summary_truncated: false,
            selected_changes: draft
                .selected_changes
                .iter()
                .map(|c| halo_store::SelectedChangeRecord {
                    path: c.path.clone(),
                    diff: c.diff.clone(),
                    truncated: false,
                })
                .collect(),
            verification_status: mapping::verification_status_core_to_str(draft.verification.status)
                .to_string(),
            verification_detail: draft.verification.detail.clone(),
            truncated: false,
            created_at: created_at.clone(),
        };
        self.ctx.store.put_handoff(&record)?;

        ok(&methods::handoff::HandoffCreateResult {
            handoff_id: handoff_id.clone(),
            package: handoff_draft_to_package(
                &draft,
                &rec,
                Some(handoff_id),
                Some(p.target_agent),
                Some(created_at),
            ),
        })
    }

    // ---------- history.* ----------

    fn history_list(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::history::HistoryListParams = parse(params)?;
        let limit = p.limit as usize;
        let mut tasks = Vec::new();
        for rec in self.ctx.store.list_tasks(limit)? {
            let latest = self
                .ctx
                .store
                .latest_evidence(&rec.task_id)?
                .map(|e| e.version)
                .unwrap_or(0);
            tasks.push(mapping::task_record_to_status(&rec, latest));
        }
        let decisions: Vec<_> = self
            .ctx
            .store
            .list_decisions(limit)?
            .iter()
            .map(mapping::decision_record_to_dto)
            .collect();
        ok(&methods::history::HistoryListResult { tasks, decisions })
    }

    fn history_evidence(&mut self, params: Value) -> Result<Value, SidecarError> {
        let p: methods::history::HistoryEvidenceParams = parse(params)?;
        self.require_task(&p.task_id)?;
        let records = self.ctx.store.list_evidence(&p.task_id)?;
        let latest = records.last().map(|r| r.version).unwrap_or(0);
        let versions: Vec<_> = records
            .iter()
            .map(|r| mapping::evidence_record_to_summary(r, r.version == latest))
            .collect();
        ok(&methods::history::HistoryEvidenceResult { versions })
    }
}

fn attribution_after_manual_edit(
    current: &Attribution,
    is_new_path: bool,
    note: &str,
) -> Attribution {
    if !is_new_path {
        return current.clone();
    }
    match current {
        Attribution::AgentOnly => current.clone().with_manual_edit(note),
        Attribution::Mixed { reasons } if reasons.len() < halo_core::limits::MANUAL_EDIT_REASONS_MAX => {
            current.clone().with_manual_edit(note)
        }
        Attribution::Mixed { reasons } if reasons.len() == halo_core::limits::MANUAL_EDIT_REASONS_MAX => {
            current.clone().with_manual_edit(MANUAL_EDIT_OVERFLOW_NOTE)
        }
        Attribution::Mixed { .. } => current.clone(),
    }
}

fn local_hhmm() -> String {
    local_hhmm_impl()
}

#[cfg(target_os = "windows")]
fn local_hhmm_impl() -> String {
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }

    unsafe extern "system" {
        fn GetLocalTime(system_time: *mut SystemTime);
    }

    let mut local = SystemTime {
        year: 0,
        month: 0,
        day_of_week: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        milliseconds: 0,
    };
    // GetLocalTime 只写入调用方提供的固定大小缓冲区，且没有可报告的失败路径。
    unsafe { GetLocalTime(&mut local) };
    format!("{:02}:{:02}", local.hour, local.minute)
}

#[cfg(not(target_os = "windows"))]
fn local_hhmm_impl() -> String {
    let utc = time::OffsetDateTime::now_utc();
    format!("{:02}:{:02} UTC", utc.hour(), utc.minute())
}

fn workspace_status_dto(ws: &ActiveWorkspace) -> methods::workspace::WorkspaceStatus {
    methods::workspace::WorkspaceStatus {
        active: true,
        workspace_id: ws.workspace_id.clone(),
        real_path: ws.real_path.clone(),
        git_root: ws.git_root.clone(),
        root_commit: ws.root_commit.clone(),
        trust: match ws.trust {
            halo_core::TrustState::Trusted => methods::workspace::TrustState::Trusted,
            halo_core::TrustState::Untrusted => methods::workspace::TrustState::Untrusted,
        },
        identity_changed: ws.identity_changed,
    }
}

/// store 证据记录 → core 证据版本（供 build_handoff 白名单构造使用）。
fn evidence_record_to_core(rec: &halo_store::EvidenceRecord) -> halo_core::EvidenceVersion {
    halo_core::EvidenceVersion {
        version: rec.version,
        outcome: match rec.outcome.as_str() {
            "finished" => halo_core::Outcome::Finished,
            "cancelled" => halo_core::Outcome::Cancelled,
            "interrupted" => halo_core::Outcome::Interrupted,
            _ => halo_core::Outcome::Failed,
        },
        attribution: if rec.attribution == "mixed" {
            halo_core::Attribution::Mixed {
                reasons: rec.attribution_reasons.clone(),
            }
        } else {
            halo_core::Attribution::AgentOnly
        },
        summary: rec.summary.clone(),
        files: rec
            .files
            .iter()
            .map(|f| halo_core::FileEvidence {
                path: f.path.clone(),
                change: match f.change.as_str() {
                    "added" => halo_core::ChangeKind::Added,
                    "deleted" => halo_core::ChangeKind::Deleted,
                    "renamed" => halo_core::ChangeKind::Renamed,
                    _ => halo_core::ChangeKind::Modified,
                },
                diff: f.diff.clone(),
                truncated: f.truncated,
                end_hash: f.end_hash.clone(),
            })
            .collect(),
        verification: halo_core::Verification {
            status: match rec.verification_status.as_str() {
                "passed" => halo_core::VerificationStatus::Passed,
                "failed" => halo_core::VerificationStatus::Failed,
                _ => halo_core::VerificationStatus::NotRun,
            },
            detail: rec.verification_detail.clone(),
            source: if rec.verification_source == "user_marked" {
                halo_core::VerificationSource::UserMarked
            } else {
                halo_core::VerificationSource::Agent
            },
        },
        created_at: rec.created_at.clone(),
    }
}

fn handoff_draft_to_package(
    draft: &halo_core::HandoffDraft,
    task: &halo_store::TaskRecord,
    handoff_id: Option<String>,
    target_agent: Option<AgentKindDto>,
    created_at: Option<String>,
) -> methods::handoff::HandoffPackage {
    methods::handoff::HandoffPackage {
        handoff_id,
        task_id: task.task_id.clone(),
        source_agent: mapping::agent_str_to_dto(&task.agent),
        target_agent,
        goal: draft.goal.clone(),
        summary: draft.summary.clone(),
        selected_changes: draft
            .selected_changes
            .iter()
            .map(|c| methods::handoff::SelectedChange {
                path: c.path.clone(),
                diff: c.diff.clone(),
            })
            .collect(),
        verification: methods::handoff::HandoffVerification {
            status: mapping::verification_status_str_to_dto(
                mapping::verification_status_core_to_str(draft.verification.status),
            ),
            detail: draft.verification.detail.clone(),
        },
        created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::Outbound;
    use crossbeam_channel::Receiver;
    use halo_config::Secret;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    // ---- 测试替身：内存凭据存储（仅 #[cfg(test)]）----

    struct FakeCred {
        available: bool,
        entries: Mutex<std::collections::HashMap<String, String>>,
    }

    impl FakeCred {
        fn new(available: bool) -> Self {
            FakeCred {
                available,
                entries: Mutex::new(Default::default()),
            }
        }
    }

    impl CredentialStore for FakeCred {
        fn set(&self, ref_name: &str, secret: &Secret) -> Result<(), halo_config::CredentialError> {
            if !self.available {
                return Err(halo_config::CredentialError::StoreUnavailable);
            }
            self.entries
                .lock()
                .unwrap()
                .insert(ref_name.to_string(), secret.expose().to_string());
            Ok(())
        }
        fn get(&self, ref_name: &str) -> Result<Secret, halo_config::CredentialError> {
            if !self.available {
                return Err(halo_config::CredentialError::StoreUnavailable);
            }
            self.entries
                .lock()
                .unwrap()
                .get(ref_name)
                .map(|v| Secret::new(v.clone()))
                .ok_or(halo_config::CredentialError::NotFound)
        }
        fn exists(&self, ref_name: &str) -> Result<bool, halo_config::CredentialError> {
            if !self.available {
                return Err(halo_config::CredentialError::StoreUnavailable);
            }
            Ok(self.entries.lock().unwrap().contains_key(ref_name))
        }
        fn available(&self) -> bool {
            self.available
        }
    }

    struct ReadyHandle;

    impl AgentHandle for ReadyHandle {
        fn run_task(
            &self,
            _spec: &halo_runtime::RunTaskSpec,
        ) -> Result<(), halo_runtime::RuntimeError> {
            Ok(())
        }

        fn cancel_native(&self) {}

        fn stop(&self, _grace: std::time::Duration) -> halo_runtime::StopOutcome {
            halo_runtime::StopOutcome::Graceful
        }

        fn state(&self) -> RuntimeState {
            RuntimeState::Ready
        }
    }

    struct Fixture {
        d: Dispatcher,
        events: Receiver<Outbound>,
        _dir: tempfile::TempDir,
    }

    fn fixture_with_cred(available: bool) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("halo.db"), halo_store::StoreLimits::default()).unwrap();
        let (tx, rx) = unbounded();
        let ctx = Ctx {
            store: Arc::new(store),
            cred: Arc::new(FakeCred::new(available)),
            bus: Arc::new(EventBus::new(tx)),
            app: Arc::new(Mutex::new(AppState::new())),
            timeouts: Timeouts::default(),
        };
        Fixture {
            d: Dispatcher::new(ctx),
            events: rx,
            _dir: dir,
        }
    }

    fn fixture() -> Fixture {
        fixture_with_cred(true)
    }

    fn req(method: &str, params: Value) -> RequestEnvelope {
        RequestEnvelope {
            v: PROTOCOL_VERSION,
            id: format!("r-{}", uuid::Uuid::new_v4()),
            method: method.to_string(),
            params,
        }
    }

    fn hello(f: &mut Fixture) {
        let resp = f.d.dispatch(req(
            "sidecar.hello",
            json!({"app_protocol_versions": [1], "app_version": "0.1.0"}),
        ));
        assert!(resp.ok, "{resp:?}");
    }

    fn err_code(resp: &Response) -> ErrorCode {
        resp.error.as_ref().expect("应为错误响应").code
    }

    fn init_repo(root: &Path) -> PathBuf {
        let repo = root.join("契约 仓库");
        fs::create_dir_all(&repo).unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git").args(args).current_dir(&repo).output().unwrap();
            assert!(out.status.success(), "git {args:?} 失败");
        };
        run(&["init", "-b", "main"]);
        fs::write(repo.join("a.txt"), "内容").unwrap();
        run(&["add", "-A"]);
        run(&[
            "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-m", "init", "--no-gpg-sign",
        ]);
        repo
    }

    #[test]
    fn fs_cage_allows_workspace_files_and_rejects_escapes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("工作区");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();

        let resolved = crate::fs::cage::resolve_existing(&root, "src/main.rs").unwrap();
        assert!(resolved.ends_with("src/main.rs"));
        assert!(matches!(
            crate::fs::cage::resolve_existing(&root, "../outside.txt"),
            Err(crate::fs::FsError::OutsideWorkspace(_))
        ));

        let git_target = crate::fs::cage::resolve_target(&root, ".git/config").unwrap();
        assert!(matches!(
            crate::fs::cage::ensure_not_git_protected(&root, &git_target.abs),
            Err(crate::fs::FsError::GitProtected(_))
        ));
    }

    #[test]
    fn fs_ops_read_write_and_list_preserve_the_contract_boundary() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("工作区");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join("src/main.rs"), b"\xef\xbb\xbffn main() {}\r\n").unwrap();
        fs::write(root.join("z.txt"), "z").unwrap();

        let read = crate::fs::ops::read(&root, "src/main.rs").unwrap();
        assert_eq!(read.encoding, methods::fs::FsEncoding::Utf8Bom);
        assert_eq!(read.line_ending, methods::fs::FsLineEnding::Crlf);
        assert_eq!(read.content, "fn main() {}\r\n");

        let write = crate::fs::ops::write(
            &root,
            "src/main.rs",
            "fn main() { println!(\"ok\"); }\n",
            &read.hash,
            methods::fs::FsWriteEncoding::Utf8Bom,
        )
        .unwrap();
        assert!(write.hash.starts_with("sha256:"));
        assert!(fs::read(root.join("src/main.rs")).unwrap().starts_with(b"\xef\xbb\xbf"));
        assert!(matches!(
            crate::fs::ops::write(
                &root,
                "src/main.rs",
                "stale",
                &read.hash,
                methods::fs::FsWriteEncoding::Utf8,
            ),
            Err(crate::fs::FsError::Conflict { .. })
        ));

        let list = crate::fs::ops::list(&root, "", 1).unwrap();
        let paths: Vec<_> = list.entries.iter().map(|entry| entry.path.as_str()).collect();
        assert_eq!(paths, vec!["src", "z.txt"]);
    }

    #[test]
    fn fs_search_uses_git_candidates_and_returns_text_locations() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/main.rs"), "fn main() {\n    println!(\"ok\");\n}\n").unwrap();
        fs::write(repo.join("ignored.txt"), "fn main() {}\n").unwrap();
        fs::write(repo.join(".gitignore"), "ignored.txt\n").unwrap();
        let git = GitClient::new(&repo);

        let paths = crate::fs::search::search(
            &repo,
            &git,
            &methods::fs::FsSearchParams {
                glob: Some("**/*.rs".to_string()),
                query: None,
                case_sensitive: false,
                max_results: 50,
            },
        )
        .unwrap();
        assert_eq!(paths.items.len(), 1);
        assert_eq!(paths.items[0].path, "src/main.rs");
        assert_eq!(paths.items[0].line, None);

        let matches = crate::fs::search::search(
            &repo,
            &git,
            &methods::fs::FsSearchParams {
                glob: None,
                query: Some("println".to_string()),
                case_sensitive: true,
                max_results: 50,
            },
        )
        .unwrap();
        assert_eq!(matches.items.len(), 1);
        assert_eq!(matches.items[0].path, "src/main.rs");
        assert_eq!(matches.items[0].line, Some(2));
        assert_eq!(matches.items[0].column, Some(5));
        assert_eq!(matches.items[0].preview.as_deref(), Some("    println!(\"ok\");"));
    }

    // ---------- hello 门禁 ----------

    #[test]
    fn any_method_before_hello_returns_hello_required() {
        let mut f = fixture();
        for method in ["workspace.status", "task.status", "history.list", "nonexistent.method"] {
            let resp = f.d.dispatch(req(method, json!({"limit": 1})));
            assert!(!resp.ok);
            assert_eq!(err_code(&resp), ErrorCode::HelloRequired, "{method}");
        }
    }

    #[test]
    fn hello_negotiates_version_and_capabilities() {
        let mut f = fixture();
        let resp = f.d.dispatch(req(
            "sidecar.hello",
            json!({"app_protocol_versions": [1, 2], "app_version": "0.1.0"}),
        ));
        assert!(resp.ok);
        let result = resp.result.unwrap();
        assert_eq!(result["protocol_version"], 1);
        let caps = result["capabilities"].as_array().unwrap();
        assert!(caps.iter().any(|c| c == "workspace"));
        assert!(caps.iter().any(|c| c == "handoff"));
        assert!(caps.iter().any(|c| c == "fs"));
        // 握手后方法放行
        let resp = f.d.dispatch(req("workspace.status", json!({})));
        assert!(resp.ok);
        assert_eq!(resp.result.unwrap()["active"], false);
    }

    #[test]
    fn hello_without_common_version_is_rejected_with_details() {
        let mut f = fixture();
        let resp = f.d.dispatch(req(
            "sidecar.hello",
            json!({"app_protocol_versions": [2, 3], "app_version": "9.0.0"}),
        ));
        assert!(!resp.ok);
        let err = resp.error.as_ref().unwrap();
        assert_eq!(err.code, ErrorCode::ProtocolVersionUnsupported);
        assert_eq!(err.details["sidecar_protocol_versions"][0], 1);
        // 失败的握手不放行后续方法
        let resp = f.d.dispatch(req("workspace.status", json!({})));
        assert_eq!(err_code(&resp), ErrorCode::HelloRequired);
    }

    #[test]
    fn unknown_method_after_hello_is_method_not_found() {
        let mut f = fixture();
        hello(&mut f);
        let resp = f.d.dispatch(req("sidecar.unknown", json!({})));
        assert_eq!(err_code(&resp), ErrorCode::MethodNotFound);
    }

    #[test]
    fn invalid_typed_params_map_to_invalid_params() {
        let mut f = fixture();
        hello(&mut f);
        for (method, params) in [
            ("task.create", json!({})),
            ("workspace.open", json!({"path": 42})),
            ("task.snapshot", json!({"after_seq": "not-a-number"})),
            ("history.list", json!({})),
        ] {
            let resp = f.d.dispatch(req(method, params));
            assert_eq!(err_code(&resp), ErrorCode::InvalidParams, "{method}");
        }
    }

    // ---------- workspace 错误映射 ----------

    #[test]
    fn workspace_open_maps_git_validation_errors() {
        let mut f = fixture();
        hello(&mut f);
        let resp = f.d.dispatch(req("workspace.open", json!({"path": "Z:\\不存在\\目录"})));
        assert_eq!(err_code(&resp), ErrorCode::WorkspacePathInvalid);
        let msg = &resp.error.as_ref().unwrap().message;
        assert!(msg.contains("工作区"), "错误文案必须中文：{msg}");

        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("非 git");
        fs::create_dir_all(&plain).unwrap();
        let resp = f.d.dispatch(req("workspace.open", json!({"path": plain.to_str().unwrap()})));
        assert_eq!(err_code(&resp), ErrorCode::WorkspaceNotGit);
    }

    #[test]
    fn workspace_open_trust_and_untrusted_task_create_flow() {
        let mut f = fixture();
        hello(&mut f);
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());

        let resp = f.d.dispatch(req("workspace.open", json!({"path": repo.to_str().unwrap()})));
        assert!(resp.ok, "{resp:?}");
        let ws = resp.result.unwrap();
        assert_eq!(ws["active"], true);
        assert_eq!(ws["trust"], "untrusted");
        assert_eq!(ws["identity_changed"], false);
        assert!(ws["root_commit"].as_str().unwrap().len() == 40);
        let ws_id = ws["workspace_id"].as_str().unwrap().to_string();
        assert!(ws_id.starts_with("ws-"));

        // 未信任：task.create 一律 WORKSPACE_NOT_TRUSTED
        let resp = f.d.dispatch(req(
            "task.create",
            json!({"agent": "pi", "config_id": "cfg-x", "title": "t", "instructions": "i"}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::WorkspaceNotTrusted);

        // 未信任：runtime.start 也一律 WORKSPACE_NOT_TRUSTED（先建配置）
        let resp = f.d.dispatch(req(
            "config.save",
            json!({
                "name": "Pi", "agent": "pi", "executable_path": "C:\\pi.exe", "model": "m",
                "thinking_level": "off", "credential_ref": null
            }),
        ));
        assert!(resp.ok, "{resp:?}");
        let cfg_id = resp.result.unwrap()["config"]["config_id"].as_str().unwrap().to_string();
        let resp = f.d.dispatch(req("runtime.start", json!({"agent": "pi", "config_id": cfg_id})));
        assert_eq!(err_code(&resp), ErrorCode::WorkspaceNotTrusted);

        // 信任决定
        let resp = f.d.dispatch(req(
            "workspace.trust",
            json!({"workspace_id": ws_id, "decision": "trust"}),
        ));
        assert!(resp.ok);
        assert_eq!(resp.result.unwrap()["trust"], "trusted");

        // 信任后：未知配置 → CONFIG_NOT_FOUND
        let resp = f.d.dispatch(req(
            "task.create",
            json!({"agent": "pi", "config_id": "cfg-unknown", "title": "t", "instructions": "i"}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::RuntimeNotReady, "运行时未就绪先于配置检查");

        // workspace.changed 事件已推送
        let mut saw_changed = 0;
        while let Ok(msg) = f.events.try_recv() {
            if let Outbound::Event(e) = msg {
                if e.event == "workspace.changed" {
                    saw_changed += 1;
                }
            }
        }
        assert!(saw_changed >= 2, "打开与信任都应推送 workspace.changed");
    }

    #[test]
    fn fs_methods_require_trust_and_use_workspace_relative_paths() {
        let mut f = fixture();
        hello(&mut f);
        let no_workspace = f.d.dispatch(req("fs.list", json!({"path": "", "depth": 1})));
        assert_eq!(err_code(&no_workspace), ErrorCode::WorkspaceNotActive);

        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let opened = f.d.dispatch(req(
            "workspace.open",
            json!({"path": repo.to_string_lossy()}),
        ));
        let workspace_id = opened.result.unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();
        let untrusted = f.d.dispatch(req("fs.read", json!({"path": "a.txt"})));
        assert_eq!(err_code(&untrusted), ErrorCode::WorkspaceNotTrusted);

        let trusted = f.d.dispatch(req(
            "workspace.trust",
            json!({"workspace_id": workspace_id, "decision": "trust"}),
        ));
        assert!(trusted.ok);
        let listed = f.d.dispatch(req("fs.list", json!({"path": "", "depth": 1})));
        assert!(listed.ok, "{listed:?}");
        assert_eq!(listed.result.as_ref().unwrap()["entries"][0]["path"], "a.txt");

        let read = f.d.dispatch(req("fs.read", json!({"path": "a.txt"})));
        let read_result = read.result.unwrap();
        let write = f.d.dispatch(req(
            "fs.write",
            json!({
                "path": "a.txt", "content": "新内容\n", "expected_hash": read_result["hash"], "encoding": "utf-8"
            }),
        ));
        assert!(write.ok, "{write:?}");
        assert_eq!(fs::read_to_string(repo.join("a.txt")).unwrap(), "新内容\n");

        let protected = f.d.dispatch(req(
            "fs.write",
            json!({"path": ".git/config", "content": "x", "expected_hash": "sha256:any"}),
        ));
        assert_eq!(err_code(&protected), ErrorCode::FsGitProtected);
        let bad_depth = f.d.dispatch(req("fs.list", json!({"path": "", "depth": 9})));
        assert_eq!(err_code(&bad_depth), ErrorCode::InvalidParams);
    }

    #[test]
    fn workspace_trust_with_wrong_id_is_rejected() {
        let mut f = fixture();
        hello(&mut f);
        let resp = f.d.dispatch(req(
            "workspace.trust",
            json!({"workspace_id": "ws-nope", "decision": "trust"}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::WorkspaceNotActive);
    }

    #[test]
    fn workspace_reopen_keeps_persisted_trust() {
        let mut f = fixture();
        hello(&mut f);
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let path = repo.to_str().unwrap();

        let resp = f.d.dispatch(req("workspace.open", json!({"path": path})));
        let ws_id = resp.result.unwrap()["workspace_id"].as_str().unwrap().to_string();
        f.d.dispatch(req("workspace.trust", json!({"workspace_id": ws_id, "decision": "trust"})));
        // 关闭后重新打开：信任决定持久化生效
        let resp = f.d.dispatch(req("workspace.close", json!({})));
        assert!(resp.ok);
        let resp = f.d.dispatch(req("workspace.open", json!({"path": path})));
        assert_eq!(resp.result.unwrap()["trust"], "trusted");
    }

    struct WorkspaceClosingHandle {
        stopped: std::sync::atomic::AtomicBool,
    }

    impl AgentHandle for WorkspaceClosingHandle {
        fn run_task(
            &self,
            _spec: &halo_runtime::RunTaskSpec,
        ) -> Result<(), halo_runtime::RuntimeError> {
            Ok(())
        }

        fn cancel_native(&self) {}

        fn stop(&self, _grace: std::time::Duration) -> halo_runtime::StopOutcome {
            self.stopped
                .store(true, std::sync::atomic::Ordering::SeqCst);
            halo_runtime::StopOutcome::Graceful
        }

        fn state(&self) -> RuntimeState {
            RuntimeState::Ready
        }
    }

    #[test]
    fn workspace_close_publishes_stopped_before_workspace_changed() {
        let mut f = fixture();
        hello(&mut f);
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let opened = f.d.dispatch(req(
            "workspace.open",
            json!({"path": repo.to_string_lossy()}),
        ));
        assert!(opened.ok, "{opened:?}");

        // workspace.open 的通知不属于本次关闭的可观察行为。
        let _: Vec<_> = f.events.try_iter().collect();
        let handle = Arc::new(WorkspaceClosingHandle {
            stopped: std::sync::atomic::AtomicBool::new(false),
        });
        {
            let mut app = lock(&f.d.ctx.app);
            let slot = app.slot_mut(AgentKind::OpenCode);
            slot.last_state = RuntimeState::Ready;
            slot.version = Some("1.18.5".to_string());
            slot.handle = Some(handle.clone());
        }

        let closed = f.d.dispatch(req("workspace.close", json!({})));
        assert!(closed.ok, "{closed:?}");
        assert!(handle.stopped.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(
            lock(&f.d.ctx.app).slot(AgentKind::OpenCode).last_state,
            RuntimeState::Stopped
        );

        let events: Vec<_> = f
            .events
            .try_iter()
            .filter_map(|outbound| match outbound {
                Outbound::Event(event) => Some(event),
                Outbound::Response(_) => None,
            })
            .collect();
        assert_eq!(events.len(), 2, "关闭只应广播运行时停止和工作区关闭事件");
        assert_eq!(events[0].event, "runtime.state");
        assert_eq!(events[0].payload["agent"], "opencode");
        assert_eq!(events[0].payload["state"], "stopped");
        assert_eq!(events[0].payload["version"], "1.18.5");
        assert_eq!(events[1].event, "workspace.changed");
        assert_eq!(events[1].payload["active"], false);
        assert!(events[0].seq < events[1].seq);
    }

    #[test]
    fn workspace_switch_publishes_stopped_before_new_workspace_changed() {
        let mut f = fixture();
        hello(&mut f);
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let first_repo = init_repo(first_dir.path());
        let second_repo = init_repo(second_dir.path());
        let opened = f.d.dispatch(req(
            "workspace.open",
            json!({"path": first_repo.to_string_lossy()}),
        ));
        assert!(opened.ok, "{opened:?}");

        let _: Vec<_> = f.events.try_iter().collect();
        let handle = Arc::new(WorkspaceClosingHandle {
            stopped: std::sync::atomic::AtomicBool::new(false),
        });
        {
            let mut app = lock(&f.d.ctx.app);
            let slot = app.slot_mut(AgentKind::OpenCode);
            slot.last_state = RuntimeState::Ready;
            slot.version = Some("1.18.5".to_string());
            slot.handle = Some(handle.clone());
        }

        let switched = f.d.dispatch(req(
            "workspace.open",
            json!({"path": second_repo.to_string_lossy()}),
        ));
        assert!(switched.ok, "{switched:?}");
        assert!(handle.stopped.load(std::sync::atomic::Ordering::SeqCst));

        let events: Vec<_> = f
            .events
            .try_iter()
            .filter_map(|outbound| match outbound {
                Outbound::Event(event) => Some(event),
                Outbound::Response(_) => None,
            })
            .collect();
        assert_eq!(events.len(), 2, "切换只应广播运行时停止和新工作区事件");
        assert_eq!(events[0].event, "runtime.state");
        assert_eq!(events[0].payload["agent"], "opencode");
        assert_eq!(events[0].payload["state"], "stopped");
        assert_eq!(events[1].event, "workspace.changed");
        assert_eq!(events[1].payload["active"], true);
        assert!(events[0].seq < events[1].seq);
    }

    // ---------- config 错误映射 ----------

    #[test]
    fn config_save_rejects_arbitrary_launch_injection_fields() {
        let mut f = fixture();
        hello(&mut f);
        let resp = f.d.dispatch(req(
            "config.save",
            json!({
                "name": "x", "agent": "pi", "executable_path": "C:\\pi.exe", "model": "m",
                "thinking_level": "low", "credential_ref": null,
                "env_overrides": {"LD_PRELOAD": "evil.dll"}
            }),
        ));
        assert_eq!(err_code(&resp), ErrorCode::InvalidParams);
    }

    #[test]
    fn config_save_with_credential_fails_closed_when_store_unavailable() {
        let mut f = fixture_with_cred(false);
        hello(&mut f);
        let resp = f.d.dispatch(req(
            "config.save",
            json!({
                "name": "x", "agent": "pi", "executable_path": "C:\\pi.exe", "model": "m",
                "thinking_level": "low", "credential_ref": "halo/pi/openai"
            }),
        ));
        assert_eq!(err_code(&resp), ErrorCode::CredentialStoreUnavailable);
        // 不含凭据引用的配置仍可保存
        let resp = f.d.dispatch(req(
            "config.save",
            json!({
                "name": "x", "agent": "pi", "executable_path": "C:\\pi.exe", "model": "m",
                "thinking_level": "low", "credential_ref": null
            }),
        ));
        assert!(resp.ok, "{resp:?}");
    }

    #[test]
    fn opencode_start_with_missing_credential_reference_reports_failed_state_and_recovery() {
        let mut f = fixture();
        hello(&mut f);
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let opened = f
            .d
            .dispatch(req("workspace.open", json!({"path": repo.to_string_lossy()})));
        let workspace_id = opened.result.unwrap()["workspace_id"].as_str().unwrap().to_string();
        let trusted = f.d.dispatch(req(
            "workspace.trust",
            json!({"workspace_id": workspace_id, "decision": "trust"}),
        ));
        assert!(trusted.ok);

        let saved = f.d.dispatch(req(
            "config.save",
            json!({
                "name": "OpenCode",
                "agent": "opencode",
                "executable_path": "C:\\tools\\opencode.exe",
                "model": "openai/gpt-5",
                "thinking_level": "off",
                "credential_ref": "halo/missing/opencode"
            }),
        ));
        assert!(saved.ok, "{saved:?}");
        let config_id = saved.result.unwrap()["config"]["config_id"]
            .as_str()
            .unwrap()
            .to_string();

        let start = f.d.dispatch(req(
            "runtime.start",
            json!({"agent": "opencode", "config_id": config_id}),
        ));
        assert_eq!(err_code(&start), ErrorCode::CredentialNotFound);
        let status = f
            .d
            .dispatch(req("runtime.status", json!({})))
            .result
            .unwrap();
        assert_eq!(status["opencode"]["state"], "failed");
        assert!(status["opencode"]["reason"].as_str().unwrap_or_default().contains("凭据引用"));
        assert!(!status["opencode"]["recovery_hint"].as_str().unwrap_or_default().is_empty());
        assert_eq!(status["pi"]["state"], "not_probed");
    }

    #[test]
    fn opencode_start_without_credential_reference_fails_closed_before_launch() {
        let mut f = fixture();
        hello(&mut f);
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let opened = f.d.dispatch(req(
            "workspace.open",
            json!({"path": repo.to_string_lossy()}),
        ));
        let workspace_id = opened.result.unwrap()["workspace_id"]
            .as_str()
            .unwrap()
            .to_string();
        let trusted = f.d.dispatch(req(
            "workspace.trust",
            json!({"workspace_id": workspace_id, "decision": "trust"}),
        ));
        assert!(trusted.ok);

        let saved = f.d.dispatch(req(
            "config.save",
            json!({
                "name": "OpenCode 无凭据引用",
                "agent": "opencode",
                "executable_path": "C:\\tools\\opencode.exe",
                "model": "openai/gpt-5",
                "thinking_level": "off",
                "credential_ref": null
            }),
        ));
        assert!(saved.ok, "{saved:?}");
        let config_id = saved.result.unwrap()["config"]["config_id"]
            .as_str()
            .unwrap()
            .to_string();

        let start = f.d.dispatch(req(
            "runtime.start",
            json!({"agent": "opencode", "config_id": config_id}),
        ));
        assert_eq!(err_code(&start), ErrorCode::CredentialNotFound);
        let status = f
            .d
            .dispatch(req("runtime.status", json!({})))
            .result
            .unwrap();
        assert_eq!(status["opencode"]["state"], "failed");
        assert!(status["opencode"]["version"].is_null());
        assert!(status["opencode"]["reason"]
            .as_str()
            .unwrap_or_default()
            .contains("缺少凭据引用"));
        assert!(!status["opencode"]["recovery_hint"]
            .as_str()
            .unwrap_or_default()
            .is_empty());
    }

    #[test]
    fn config_delete_unknown_is_config_not_found() {
        let mut f = fixture();
        hello(&mut f);
        let resp = f.d.dispatch(req("config.delete", json!({"config_id": "cfg-none"})));
        assert_eq!(err_code(&resp), ErrorCode::ConfigNotFound);
    }

    #[test]
    fn credential_check_reports_existence_and_availability() {
        let mut f = fixture();
        hello(&mut f);
        f.d.ctx.cred.set("halo/pi/openai", &Secret::new("sk-x")).unwrap();
        let resp = f.d.dispatch(req(
            "config.credential_check",
            json!({"credential_ref": "halo/pi/openai"}),
        ));
        let result = resp.result.unwrap();
        assert_eq!(result["exists"], true);
        assert_eq!(result["store_available"], true);

        let mut f2 = fixture_with_cred(false);
        hello(&mut f2);
        let resp = f2.d.dispatch(req(
            "config.credential_check",
            json!({"credential_ref": "halo/pi/openai"}),
        ));
        let result = resp.result.unwrap();
        assert_eq!(result["exists"], false);
        assert_eq!(result["store_available"], false);
    }

    // ---------- runtime / task 前置校验 ----------

    #[test]
    fn runtime_probe_unknown_config_and_status_shape() {
        let mut f = fixture();
        hello(&mut f);
        let resp = f.d.dispatch(req("runtime.probe", json!({"agent": "pi", "config_id": "cfg-none"})));
        assert_eq!(err_code(&resp), ErrorCode::ConfigNotFound);

        let resp = f.d.dispatch(req("runtime.status", json!({})));
        let result = resp.result.unwrap();
        assert_eq!(result["pi"]["state"], "not_probed");
        assert_eq!(result["opencode"]["state"], "not_probed");
    }

    #[test]
    fn runtime_stop_publishes_stopped_state_after_generation_advance() {
        let mut f = fixture();
        hello(&mut f);
        while f.events.try_recv().is_ok() {}
        {
            let mut app = lock(&f.d.ctx.app);
            let slot = app.slot_mut(AgentKind::OpenCode);
            slot.last_state = RuntimeState::Ready;
            slot.version = Some("1.18.5".to_string());
            slot.handle = Some(std::sync::Arc::new(ReadyHandle));
        }

        let response = f.d.dispatch(req("runtime.stop", json!({"agent": "opencode"})));

        assert!(response.ok, "{response:?}");
        assert_eq!(response.result.unwrap()["state"], "stopped");
        let outbound = f
            .events
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("runtime.stop 必须广播 runtime.state");
        let Outbound::Event(event) = outbound else {
            panic!("runtime.stop 必须产生事件");
        };
        assert_eq!(event.event, "runtime.state");
        assert_eq!(event.payload["agent"], "opencode");
        assert_eq!(event.payload["state"], "stopped");
        assert_eq!(event.payload["version"], "1.18.5");
    }

    #[test]
    fn task_create_without_workspace_is_workspace_not_active() {
        let mut f = fixture();
        hello(&mut f);
        let resp = f.d.dispatch(req(
            "task.create",
            json!({"agent": "pi", "config_id": "cfg-x", "title": "t", "instructions": "i"}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::WorkspaceNotActive);
    }

    #[test]
    fn task_cancel_unknown_task_is_not_found() {
        let mut f = fixture();
        hello(&mut f);
        let resp = f.d.dispatch(req("task.cancel", json!({"task_id": "task-none"})));
        assert_eq!(err_code(&resp), ErrorCode::TaskNotFound);
    }

    #[test]
    fn mark_manual_edit_on_running_task_yields_mixed_and_event() {
        let mut f = fixture();
        hello(&mut f);
        install_active_task(&f.d.ctx, "task-1", halo_core::TaskState::Running);

        let resp = f.d.dispatch(req(
            "task.mark_manual_edit",
            json!({"task_id": "task-1", "note": "手工调整了 src/auth.rs"}),
        ));
        assert!(resp.ok, "{resp:?}");
        assert_eq!(resp.result.unwrap()["attribution"], "mixed");
        let app = lock(&f.d.ctx.app);
        assert!(app.task.as_ref().unwrap().attribution.is_mixed());
        drop(app);
        let mut saw = false;
        while let Ok(msg) = f.events.try_recv() {
            if let Outbound::Event(e) = msg {
                if e.event == "task.manual_edit" {
                    assert_eq!(e.payload["note"], "手工调整了 src/auth.rs");
                    assert_eq!(e.task_id.as_deref(), Some("task-1"));
                    saw = true;
                }
            }
        }
        assert!(saw, "应推送 task.manual_edit 事件");

        // 终态任务不可标记
        lock(&f.d.ctx.app).task.as_mut().unwrap().state = halo_core::TaskState::Accepted;
        let resp = f.d.dispatch(req(
            "task.mark_manual_edit",
            json!({"task_id": "task-1", "note": "x"}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::TaskNotFound);
    }

    #[test]
    fn fs_manual_edits_emit_each_time_but_persist_each_path_once() {
        let mut f = fixture();
        hello(&mut f);
        install_active_task(
            &f.d.ctx,
            "task-fs",
            halo_core::TaskState::WaitingDeveloper,
        );

        f.d.record_fs_manual_edit(ManualEditOp::Write, "src/auth.rs", None);
        f.d.record_fs_manual_edit(ManualEditOp::Write, "src/auth.rs", None);

        let app = lock(&f.d.ctx.app);
        let task = app.task.as_ref().unwrap();
        let paths: Vec<_> = task.manual_edit_paths.iter().map(String::as_str).collect();
        assert_eq!(paths, vec!["src/auth.rs"]);
        let reasons = match &task.attribution {
            Attribution::Mixed { reasons } => reasons,
            Attribution::AgentOnly => panic!("成功写入必须使归因变为 mixed"),
        };
        assert_eq!(reasons.len(), 1, "同一路径重复保存不得扩大归因原因");
        drop(app);

        let stored = f.d.ctx.store.get_task("task-fs").unwrap().unwrap();
        assert_eq!(stored.manual_edit_paths, vec!["src/auth.rs"]);

        let events: Vec<_> = std::iter::from_fn(|| f.events.try_recv().ok())
            .filter_map(|outbound| match outbound {
                Outbound::Event(event) if event.event == "task.manual_edit" => Some(event),
                _ => None,
            })
            .collect();
        assert_eq!(events.len(), 2, "每次成功写入都必须产生过程事件");
        assert!(events.iter().all(|event| event.payload["path"] == "src/auth.rs"));

        lock(&f.d.ctx.app).task.as_mut().unwrap().state = halo_core::TaskState::ReviewReady;
        f.d.record_fs_manual_edit(ManualEditOp::Write, "src/later.rs", None);
        let app = lock(&f.d.ctx.app);
        assert!(
            !app.task.as_ref().unwrap().manual_edit_paths.contains("src/later.rs"),
            "review_ready 后的写入不应再改归因"
        );
    }

    #[test]
    fn manual_edit_reason_cap_keeps_one_overflow_summary() {
        let mut attribution = Attribution::AgentOnly;
        for index in 0..halo_core::limits::MANUAL_EDIT_REASONS_MAX {
            attribution = attribution_after_manual_edit(&attribution, true, &format!("path-{index}"));
        }
        attribution = attribution_after_manual_edit(&attribution, true, "overflow");
        attribution = attribution_after_manual_edit(&attribution, true, "overflow-again");

        let reasons = match attribution {
            Attribution::Mixed { reasons } => reasons,
            Attribution::AgentOnly => panic!("应保持 mixed"),
        };
        assert_eq!(
            reasons.len(),
            halo_core::limits::MANUAL_EDIT_REASONS_MAX + 1,
            "上限后只允许一条汇总说明"
        );
        assert_eq!(reasons.last().map(String::as_str), Some(MANUAL_EDIT_OVERFLOW_NOTE));
    }

    #[test]
    fn mark_verification_only_accepts_not_run() {
        let mut f = fixture();
        hello(&mut f);
        install_active_task(&f.d.ctx, "task-1", halo_core::TaskState::Running);
        let resp = f.d.dispatch(req(
            "task.mark_verification",
            json!({"task_id": "task-1", "status": "passed", "note": "x"}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::InvalidParams);

        let resp = f.d.dispatch(req(
            "task.mark_verification",
            json!({"task_id": "task-1", "status": "not_run", "note": "本次未运行测试"}),
        ));
        assert!(resp.ok);
        let app = lock(&f.d.ctx.app);
        let v = app.task.as_ref().unwrap().verification_user.clone().unwrap();
        assert_eq!(v.status, halo_core::VerificationStatus::NotRun);
        assert_eq!(v.source, halo_core::VerificationSource::UserMarked);
    }

    #[test]
    fn task_snapshot_returns_events_or_event_gap() {
        let mut f = fixture();
        hello(&mut f);
        for _ in 0..1500 {
            f.d.ctx.bus.emit(None, "trace.item", json!({}));
        }
        let resp = f.d.dispatch(req("task.snapshot", json!({"after_seq": 0})));
        assert_eq!(err_code(&resp), ErrorCode::EventGap);

        let last = f.d.ctx.bus.last_seq();
        let resp = f.d.dispatch(req("task.snapshot", json!({"after_seq": last - 3})));
        assert!(resp.ok);
        let result = resp.result.unwrap();
        assert_eq!(result["last_seq"], last);
        assert_eq!(result["events"].as_array().unwrap().len(), 3);
        assert_eq!(result["session_messages"], json!([]));
    }

    #[test]
    fn task_snapshot_rebuilds_the_redacted_active_session_record() {
        let mut f = fixture();
        hello(&mut f);
        install_active_task(&f.d.ctx, "task-session", halo_core::TaskState::WaitingDeveloper);

        let long = "z".repeat(halo_core::limits::TRACE_TEXT_MAX + 32);
        let message = crate::state::append_active_session_message(
            &f.d.ctx.app,
            &f.d.ctx.bus,
            "task-session",
            methods::task::TaskSessionMessageRole::Agent,
            &format!("Bearer secrettoken12345678 {long}"),
        )
        .expect("waiting_developer 是活动任务，应保留回复");
        assert!(message.truncated);

        let snapshot = f
            .d
            .dispatch(req("task.snapshot", json!({"after_seq": 0})));
        assert!(snapshot.ok, "{snapshot:?}");
        let result = snapshot.result.unwrap();
        assert_eq!(result["task"]["state"], "waiting_developer");
        let messages = result["session_messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "agent");
        assert_eq!(messages[0]["truncated"], true);
        let text = messages[0]["text"].as_str().unwrap();
        assert!(!text.contains("secrettoken12345678"));
        assert!(text.contains("[REDACTED]"));

        let status = f.d.dispatch(req("task.status", json!({})));
        assert!(status.ok, "{status:?}");
        assert!(status.result.unwrap()["task"].get("session_messages").is_none());
    }

    // ---------- review / delivery / handoff / history ----------

    fn seed_reviewable_task(ctx: &Ctx, task_id: &str, versions: u32) {
        ctx.store
            .put_task(&halo_store::TaskRecord {
                task_id: task_id.to_string(),
                agent: "pi".to_string(),
                title: "审查任务".to_string(),
                goal: "审查任务的详细目标".to_string(),
                state: "review_ready".to_string(),
                attribution: "agent_only".to_string(),
                manual_edit_paths: vec![],
                baseline_head: Some("abc".to_string()),
                baseline_captured_at: now_ts(),
                created_at: now_ts(),
                ended_at: None,
                cancel_mode: None,
            })
            .unwrap();
        for i in 1..=versions {
            ctx.store
                .append_evidence(
                    task_id,
                    &halo_store::EvidenceDraft {
                        outcome: "finished".to_string(),
                        attribution: "agent_only".to_string(),
                        attribution_reasons: vec![],
                        summary: format!("第 {i} 次运行"),
                        files: vec![halo_store::FileChangeDraft {
                            path: "src/auth.rs".to_string(),
                            change: "modified".to_string(),
                            diff: "+line".to_string(),
                            end_hash: None,
                        }],
                        verification_status: "passed".to_string(),
                        verification_detail: "cargo test 通过".to_string(),
                        verification_source: "agent".to_string(),
                        baseline_dirty_files: vec![],
                        created_at: now_ts(),
                    },
                )
                .unwrap();
        }
    }

    fn install_active_task(ctx: &Ctx, task_id: &str, state: halo_core::TaskState) {
        let task = crate::state::ActiveTask {
            task_id: task_id.to_string(),
            agent: AgentKind::Pi,
            title: "内存任务".to_string(),
            instructions: "详细目标说明".to_string(),
            state,
            attribution: halo_core::Attribution::AgentOnly,
            manual_edit_paths: Default::default(),
            baseline: halo_core::Baseline {
                head: None,
                tree: "tree".to_string(),
                dirty_files: vec![],
                captured_at: now_ts(),
            },
            created_at: now_ts(),
            ended_at: None,
            cancel_mode: None,
            latest_evidence_version: 0,
            verification_agent: None,
            verification_user: None,
            session_messages: vec![],
            cancel_tx: None,
        };
        ctx.store.put_task(&task.to_record()).unwrap();
        lock(&ctx.app).task = Some(task);
    }

    #[test]
    fn review_get_returns_latest_bundle_and_specific_version() {
        let mut f = fixture();
        hello(&mut f);
        seed_reviewable_task(&f.d.ctx, "task-r", 2);

        let resp = f.d.dispatch(req("review.get", json!({"task_id": "task-r"})));
        assert!(resp.ok, "{resp:?}");
        let bundle = resp.result.unwrap();
        assert_eq!(bundle["evidence_version"], 2);
        assert_eq!(bundle["is_latest"], true);
        assert_eq!(bundle["files"][0]["path"], "src/auth.rs");
        assert_eq!(bundle["verification"]["status"], "passed");

        let resp = f.d.dispatch(req("review.get", json!({"task_id": "task-r", "version": 1})));
        let bundle = resp.result.unwrap();
        assert_eq!(bundle["evidence_version"], 1);
        assert_eq!(bundle["is_latest"], false);

        let resp = f.d.dispatch(req("review.get", json!({"task_id": "task-r", "version": 9})));
        assert_eq!(err_code(&resp), ErrorCode::EvidenceNotFound);

        let resp = f.d.dispatch(req("review.get", json!({"task_id": "task-none"})));
        assert_eq!(err_code(&resp), ErrorCode::TaskNotFound);
    }

    #[test]
    fn review_get_on_running_task_is_not_reviewable() {
        let mut f = fixture();
        hello(&mut f);
        install_active_task(&f.d.ctx, "task-run", halo_core::TaskState::Running);
        let resp = f.d.dispatch(req("review.get", json!({"task_id": "task-run"})));
        assert_eq!(err_code(&resp), ErrorCode::TaskNotReviewable);
    }

    #[test]
    fn delivery_accept_enforces_latest_version_then_terminalizes() {
        let mut f = fixture();
        hello(&mut f);
        seed_reviewable_task(&f.d.ctx, "task-d", 2);

        // 旧版本 → EVIDENCE_NOT_LATEST
        let resp = f.d.dispatch(req(
            "delivery.accept",
            json!({"task_id": "task-d", "evidence_version": 1}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::EvidenceNotLatest);
        assert_eq!(resp.error.as_ref().unwrap().details["latest_version"], 2);

        // 最新版本 → accepted
        let resp = f.d.dispatch(req(
            "delivery.accept",
            json!({"task_id": "task-d", "evidence_version": 2}),
        ));
        assert!(resp.ok, "{resp:?}");
        let decision = resp.result.unwrap();
        assert_eq!(decision["decision"]["kind"], "accepted");
        assert_eq!(decision["decision"]["evidence_version"], 2);
        let rec = f.d.ctx.store.get_task("task-d").unwrap().unwrap();
        assert_eq!(rec.state, "accepted");
        assert!(rec.ended_at.is_some());

        // 终态后再拒绝 → TASK_NOT_REVIEWABLE
        let resp = f.d.dispatch(req(
            "delivery.reject",
            json!({"task_id": "task-d", "evidence_version": 2}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::TaskNotReviewable);
    }

    #[test]
    fn delivery_reject_records_reason_and_keeps_evidence() {
        let mut f = fixture();
        hello(&mut f);
        seed_reviewable_task(&f.d.ctx, "task-j", 1);
        let resp = f.d.dispatch(req(
            "delivery.reject",
            json!({"task_id": "task-j", "evidence_version": 1, "reason": "diff 不符合预期，且含 password=hunter2"}),
        ));
        assert!(resp.ok, "{resp:?}");
        let decision = resp.result.unwrap();
        assert_eq!(decision["decision"]["kind"], "rejected");
        let reason = decision["decision"]["reason"].as_str().unwrap();
        assert!(!reason.contains("hunter2"), "拒绝原因必须脱敏：{reason}");
        // 拒绝不删除证据
        assert!(f.d.ctx.store.latest_evidence("task-j").unwrap().is_some());
        assert_eq!(
            f.d.ctx.store.get_task("task-j").unwrap().unwrap().state,
            "rejected"
        );
    }

    #[test]
    fn delivery_on_running_task_is_task_still_running() {
        let mut f = fixture();
        hello(&mut f);
        install_active_task(&f.d.ctx, "task-run2", halo_core::TaskState::Running);
        // 给运行中的任务硬塞一个证据版本（模拟异常时序），仍不允许结论
        f.d.ctx
            .store
            .append_evidence(
                "task-run2",
                &halo_store::EvidenceDraft {
                    outcome: "finished".to_string(),
                    attribution: "agent_only".to_string(),
                    attribution_reasons: vec![],
                    summary: "s".to_string(),
                    files: vec![],
                    verification_status: "not_run".to_string(),
                    verification_detail: String::new(),
                    verification_source: "agent".to_string(),
                    baseline_dirty_files: vec![],
                    created_at: now_ts(),
                },
            )
            .unwrap();
        let resp = f.d.dispatch(req(
            "delivery.accept",
            json!({"task_id": "task-run2", "evidence_version": 1}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::TaskStillRunning);
    }

    #[test]
    fn handoff_preview_running_task_rejected_and_reviewable_flow_works() {
        let mut f = fixture();
        hello(&mut f);
        install_active_task(&f.d.ctx, "task-h-run", halo_core::TaskState::Running);
        let resp = f.d.dispatch(req(
            "handoff.preview",
            json!({"task_id": "task-h-run", "selected_files": null}),
        ));
        assert_eq!(err_code(&resp), ErrorCode::TaskStillRunning);

        seed_reviewable_task(&f.d.ctx, "task-h", 1);
        let resp = f.d.dispatch(req(
            "handoff.preview",
            json!({"task_id": "task-h", "selected_files": null}),
        ));
        assert!(resp.ok, "{resp:?}");
        let pkg = &resp.result.unwrap()["package"];
        assert_eq!(pkg["handoff_id"], Value::Null);
        assert_eq!(pkg["source_agent"], "pi");
        assert_eq!(pkg["selected_changes"][0]["path"], "src/auth.rs");

        let resp = f.d.dispatch(req(
            "handoff.create",
            json!({"task_id": "task-h", "target_agent": "opencode", "selected_files": ["src/auth.rs"]}),
        ));
        assert!(resp.ok, "{resp:?}");
        let result = resp.result.unwrap();
        let handoff_id = result["handoff_id"].as_str().unwrap();
        assert!(handoff_id.starts_with("ho-"));
        assert_eq!(result["package"]["target_agent"], "opencode");
        // 已持久化
        assert!(f.d.ctx.store.get_handoff(handoff_id).unwrap().is_some());
    }

    #[test]
    fn handoff_preview_after_restart_keeps_original_task_goal() {
        let mut f = fixture();
        hello(&mut f);
        install_active_task(&f.d.ctx, "task-restarted", halo_core::TaskState::ReviewReady);
        f.d.ctx
            .store
            .append_evidence(
                "task-restarted",
                &halo_store::EvidenceDraft {
                    outcome: "finished".to_string(),
                    attribution: "agent_only".to_string(),
                    attribution_reasons: vec![],
                    summary: "已完成".to_string(),
                    files: vec![],
                    verification_status: "passed".to_string(),
                    verification_detail: "受管应用已验证".to_string(),
                    verification_source: "agent".to_string(),
                    baseline_dirty_files: vec![],
                    created_at: now_ts(),
                },
            )
            .unwrap();

        // 模拟 Sidecar 重启：运行期 AppState 已丢失，只能从持久化任务记录恢复交接包。
        lock(&f.d.ctx.app).task = None;
        let resp = f.d.dispatch(req(
            "handoff.preview",
            json!({"task_id": "task-restarted", "selected_files": null}),
        ));

        assert!(resp.ok, "{resp:?}");
        assert_eq!(resp.result.unwrap()["package"]["goal"], "详细目标说明");
    }

    #[test]
    fn history_list_and_evidence() {
        let mut f = fixture();
        hello(&mut f);
        seed_reviewable_task(&f.d.ctx, "task-hist", 2);
        let resp = f.d.dispatch(req("history.list", json!({"limit": 10})));
        assert!(resp.ok);
        let result = resp.result.unwrap();
        assert_eq!(result["tasks"][0]["task_id"], "task-hist");
        assert_eq!(result["tasks"][0]["latest_evidence_version"], 2);

        let resp = f.d.dispatch(req("history.evidence", json!({"task_id": "task-hist"})));
        let versions = resp.result.unwrap()["versions"].clone();
        assert_eq!(versions.as_array().unwrap().len(), 2);
        // 摘要形式不含逐文件 diff 正文
        assert!(versions[0]["files"][0].get("diff").is_none());
        assert_eq!(versions[1]["is_latest"], true);

        let resp = f.d.dispatch(req("history.evidence", json!({"task_id": "task-none"})));
        assert_eq!(err_code(&resp), ErrorCode::TaskNotFound);
    }

    #[test]
    fn error_messages_are_chinese() {
        let mut f = fixture();
        let resp = f.d.dispatch(req("task.status", json!({})));
        let msg = &resp.error.as_ref().unwrap().message;
        assert!(
            msg.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "错误文案必须为中文：{msg}"
        );
    }
}
