"""首批工作台命令的集中注册。"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass
class BuiltinCommandActions:
    layout: object
    palette: object
    editor: object
    workspace: object
    task: object
    review: object
    handoff: object
    task_context: object | None = None
    review_jump: object | None = None


def register_builtin_commands(registry, actions: BuiltinCommandActions) -> None:
    register = registry.register
    register("palette.commands", "显示所有命令", "面板", lambda: actions.palette.open(">"), "Ctrl+Shift+P")
    register("palette.quickOpen", "快速打开文件", "面板", lambda: actions.palette.open(""), "Ctrl+P", "hasWorkspace")
    register("view.explorer", "显示资源管理器", "视图", lambda: actions.layout.show_view("explorer"), "Ctrl+Shift+E")
    register("view.tasks", "显示任务视图", "视图", lambda: actions.layout.show_view("tasks"), "Ctrl+Shift+A")
    register("view.review", "显示交付审查", "视图", lambda: actions.layout.show_view("review"), "Ctrl+Shift+R")
    register("view.handoff", "显示交接视图", "视图", lambda: actions.layout.show_view("handoff"))
    register("view.config", "显示启动配置", "视图", lambda: actions.layout.show_view("config"), "Ctrl+,")
    register("view.history", "显示交付历史", "视图", lambda: actions.layout.show_view("history"))
    register("view.toggleSidebar", "切换侧栏", "视图", actions.layout.toggle_sidebar, "Ctrl+B")
    register("view.toggleBottomPanel", "切换底部面板", "视图", actions.layout.toggle_bottom_panel, "Ctrl+J")
    register("editor.save", "保存文件", "编辑器", actions.editor.save, "Ctrl+S", "hasActiveEditor")
    register("editor.saveAll", "保存全部文件", "编辑器", actions.editor.saveAll, "Ctrl+Alt+S", "hasActiveEditor")
    register("editor.closeTab", "关闭当前标签", "编辑器", _close_active(actions.editor), "Ctrl+W", "hasActiveEditor")
    register("editor.closeAllTabs", "关闭全部标签", "编辑器", actions.editor.requestCloseAll, when="hasActiveEditor")
    register("editor.nextTab", "下一个标签", "编辑器", actions.editor.nextTab, "Ctrl+Tab", "hasActiveEditor")
    register("editor.previousTab", "上一个标签", "编辑器", actions.editor.prevTab, "Ctrl+Shift+Tab", "hasActiveEditor")
    register("editor.find", "在文件中查找", "编辑器", lambda: actions.editor.search.open(False), "Ctrl+F", "hasActiveEditor")
    register("workspace.open", "打开工作区", "工作区", lambda: actions.layout.show_view("tasks"))
    register("workspace.trust", "信任当前工作区", "工作区", actions.workspace.trust)
    register("workspace.close", "关闭工作区", "工作区", actions.workspace.close)
    register("task.create", "新建 Agent 任务", "任务", lambda: actions.layout.show_view("tasks"), "Ctrl+Shift+N", "hasWorkspace && !taskRunning")
    register("task.cancel", "取消当前任务", "任务", actions.task.cancel, when="taskRunning")
    if actions.task_context is not None:
        register(
            "task.addFileToContext",
            "将活动文件加入任务上下文",
            "任务",
            actions.task_context.addActiveFile,
            when="hasWorkspace",
        )
        register(
            "task.addSelectionToContext",
            "将活动选区加入任务上下文",
            "任务",
            actions.task_context.addActiveEditorSelection,
            when="hasWorkspace && hasActiveEditor",
        )
    register("task.markVerificationNotRun", "标记验证未执行", "任务", lambda: actions.task.markVerificationNotRun(""), when="hasWorkspace")
    register("review.openLatest", "打开最新交付审查", "审查", lambda: actions.layout.show_view("review"), when="hasWorkspace")
    if actions.review_jump is not None:
        register(
            "review.openInEditor",
            "在编辑器中打开当前审查文件",
            "审查",
            actions.review_jump.openCurrent,
            when="hasWorkspace",
        )
    register("review.acceptDelivery", "接受当前交付", "审查", lambda: actions.layout.show_view("review"), when="hasWorkspace && !taskRunning")
    register("review.rejectDelivery", "拒绝当前交付", "审查", lambda: actions.layout.show_view("review"), when="hasWorkspace && !taskRunning")
    register("handoff.preview", "预览交接包", "交接", lambda: actions.layout.show_view("handoff"), when="hasWorkspace && !taskRunning")
    register("handoff.create", "创建交接", "交接", lambda: actions.layout.show_view("handoff"), when="hasWorkspace && !taskRunning")


def _close_active(editor):
    def callback() -> None:
        if editor.activeDocumentId:
            editor.closeTab(editor.activeDocumentId)

    return callback
