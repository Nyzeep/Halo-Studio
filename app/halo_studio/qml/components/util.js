.pragma library

// 契约取值 → 中文文案与色调的纯函数映射；不做任何业务判断。

function textOr(value, fallback) {
    if (value === undefined || value === null)
        return fallback
    var s = String(value)
    return s.length > 0 ? s : fallback
}

function hasText(value) {
    return value !== undefined && value !== null && String(value).length > 0
}

function isTrue(value) {
    return value === true
}

function listOr(value) {
    return (value === undefined || value === null) ? [] : value
}

function connectionLabel(connected) {
    return connected === true ? "已连接" : "未连接"
}

function runtimeStateLabel(state) {
    switch (String(state)) {
    case "not_probed": return "未探测"
    case "probing": return "探测中"
    case "starting": return "启动中"
    case "ready": return "就绪"
    case "failed": return "失败"
    case "stopping": return "停止中"
    case "stopped": return "已停止"
    default: return "未知"
    }
}

function runtimeStateTone(state, theme) {
    switch (String(state)) {
    case "ready": return theme.ok
    case "failed": return theme.danger
    case "probing":
    case "starting":
    case "stopping": return theme.warn
    default: return theme.neutral
    }
}

function taskStateLabel(state) {
    switch (String(state)) {
    case "created": return "已创建"
    case "running": return "运行中"
    case "waiting_developer": return "等待开发者"
    case "awaiting_action": return "等待操作"
    case "finishing": return "收尾中"
    case "review_ready": return "待审查"
    case "accepted": return "已接受"
    case "rejected": return "已拒绝"
    case "cancelled": return "已取消"
    case "failed": return "失败"
    case "interrupted": return "已中断"
    default: return "无任务"
    }
}

function taskStateTone(state, theme) {
    switch (String(state)) {
    case "running":
    case "finishing": return theme.accent
    case "waiting_developer":
    case "awaiting_action":
    case "review_ready": return theme.warn
    case "accepted": return theme.ok
    case "rejected":
    case "failed": return theme.danger
    default: return theme.neutral
    }
}

function taskIsActive(state) {
    switch (String(state)) {
    case "created":
    case "running":
    case "waiting_developer":
    case "awaiting_action":
    case "finishing": return true
    default: return false
    }
}

function verificationLabel(status) {
    switch (String(status)) {
    case "passed": return "通过"
    case "failed": return "失败"
    case "not_run": return "未执行"
    default: return "未知"
    }
}

function verificationTone(status, theme) {
    switch (String(status)) {
    case "passed": return theme.ok
    case "failed": return theme.danger
    case "not_run": return theme.warn
    default: return theme.neutral
    }
}

function verificationSourceLabel(source) {
    switch (String(source)) {
    case "agent": return "来源：Agent 原生运行时"
    case "user_marked": return "来源：用户显式标记"
    default: return ""
    }
}

function changeLabel(change) {
    switch (String(change)) {
    case "modified": return "修改"
    case "added": return "新增"
    case "deleted": return "删除"
    case "renamed": return "重命名"
    default: return "变更"
    }
}

function changeTone(change, theme) {
    switch (String(change)) {
    case "modified": return theme.accent
    case "added": return theme.ok
    case "deleted": return theme.danger
    case "renamed": return theme.warn
    default: return theme.neutral
    }
}

function traceKindLabel(kind) {
    switch (String(kind)) {
    case "phase": return "阶段"
    case "agent_note": return "说明"
    case "file_hint": return "文件"
    case "action_request": return "操作请求"
    case "verification": return "验证"
    case "lifecycle": return "生命周期"
    default: return "轨迹"
    }
}

function traceKindTone(kind, theme) {
    switch (String(kind)) {
    case "phase": return theme.accent
    case "action_request": return theme.warn
    case "verification": return theme.ok
    default: return theme.neutral
    }
}

function sessionRoleLabel(role) {
    switch (String(role)) {
    case "user": return "开发者"
    case "agent":
    case "assistant": return "Agent"
    default: return "会话"
    }
}

function sessionRoleTone(role, theme) {
    switch (String(role)) {
    case "user": return theme.accent
    case "agent":
    case "assistant": return theme.ok
    default: return theme.neutral
    }
}

function cancelModeLabel(mode) {
    switch (String(mode)) {
    case "native": return "原生停止"
    case "forced": return "强制终止"
    default: return ""
    }
}

function agentLabel(agent) {
    switch (String(agent)) {
    case "pi": return "Pi"
    case "opencode": return "OpenCode"
    default: return "—"
    }
}

function attributionLabel(attribution) {
    switch (String(attribution)) {
    case "agent_only": return "仅 Agent"
    case "mixed": return "混合（存在人工介入）"
    default: return "未知"
    }
}

function outcomeLabel(outcome) {
    switch (String(outcome)) {
    case "finished": return "已结束"
    case "cancelled": return "已取消"
    case "failed": return "失败"
    case "interrupted": return "已中断"
    default: return "—"
    }
}

function decisionLabel(kind) {
    switch (String(kind)) {
    case "accepted": return "已接受"
    case "rejected": return "已拒绝"
    default: return "未知"
    }
}
