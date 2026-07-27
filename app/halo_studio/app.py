"""应用装配层：SidecarClient + 各 ViewModel 的创建与 QML 上下文注入。

装配假定（与 docs/module-contracts.md 第 8 节一致）：
- IPC 客户端来自 halo_studio.ipc.client.SidecarClient（Qt 信号接口）；
- 视图模型来自 halo_studio.viewmodels，构造签名统一为 ``VM(client)``，
  其中 client 为 viewmodels.base 约定的鸭子类型（request/subscribe）；
  本模块用 ContractClientBridge 把 SidecarClient 适配为该形状（纯转发，无任何模拟）。

并行开发期间依赖模块缺失时抛出 AppAssemblyError（中文说明依赖缺口）；
生产路径绝不回退到任何“假 ViewModel”。
"""

from __future__ import annotations

import sys
from dataclasses import dataclass
from typing import Any, Callable, Dict, Mapping

from PySide6.QtCore import QObject

#: QML 上下文属性名 → viewmodels 导出类名
VIEWMODEL_SPECS: tuple[tuple[str, str], ...] = (
    ("appVM", "AppViewModel"),
    ("workspaceVM", "WorkspaceViewModel"),
    ("configVM", "ConfigViewModel"),
    ("runtimeVM", "RuntimeViewModel"),
    ("taskVM", "TaskViewModel"),
    ("traceVM", "TraceViewModel"),
    ("reviewVM", "ReviewViewModel"),
    ("handoffVM", "HandoffViewModel"),
    ("historyVM", "HistoryViewModel"),
)


class AppAssemblyError(RuntimeError):
    """依赖缺口或装配失败；调用方应向用户展示中文原因后以非 0 退出。"""


class ContractClientBridge(QObject):
    """把 ipc.SidecarClient（Qt 信号接口）适配为 viewmodels.base 约定的鸭子类型。

    - ``request(method, params, on_ok, on_err)``：on_err 收到
      ``{"code", "message", "details"}``；连接层异常映射为客户端侧稳定 code
      （CONNECTION_CLOSED / REQUEST_TIMEOUT / IPC_ERROR），message 保持中文。
    - ``subscribe(event, handler)``：按事件名分发完整事件封包；断连以合成事件
      ``client.disconnected``（payload={"reason": …}）透出。
    回调线程边界由 SidecarClient 保证（全部已转主线程）。
    """

    def __init__(self, client, parent: QObject | None = None) -> None:
        super().__init__(parent)
        self._client = client
        self._handlers: Dict[str, list[Callable[[dict], None]]] = {}
        client.eventReceived.connect(self._dispatch_event)
        client.disconnected.connect(self._dispatch_disconnected)

    def subscribe(self, event: str, handler: Callable[[dict], None]) -> None:
        self._handlers.setdefault(event, []).append(handler)

    def request(
        self,
        method: str,
        params: Mapping[str, Any] | None = None,
        on_ok: Callable[[dict], None] | None = None,
        on_err: Callable[[dict], None] | None = None,
    ) -> None:
        def _on_result(result: dict) -> None:
            if on_ok is not None:
                on_ok(result)

        def _on_error(exc: Exception) -> None:
            if on_err is None:
                return
            code = getattr(exc, "code", None)
            details = getattr(exc, "details", None)
            if isinstance(code, str) and code:
                message = getattr(exc, "message", None) or str(exc)
                on_err({"code": code, "message": str(message),
                        "details": dict(details) if isinstance(details, dict) else {}})
                return
            fallback_code = "IPC_ERROR"
            if isinstance(exc, TimeoutError):
                fallback_code = "REQUEST_TIMEOUT"
            elif type(exc).__name__ == "ConnectionClosedError":
                fallback_code = "CONNECTION_CLOSED"
            on_err({"code": fallback_code, "message": str(exc), "details": {}})

        self._client.request(method, dict(params or {}),
                             on_result=_on_result, on_error=_on_error)

    # ---- 信号分发 ---------------------------------------------------------

    def _dispatch_event(self, envelope: dict) -> None:
        event = str((envelope or {}).get("event") or "")
        for handler in list(self._handlers.get(event, ())):
            handler(envelope)

    def _dispatch_disconnected(self, reason: str) -> None:
        envelope = {"event": "client.disconnected", "task_id": None,
                    "payload": {"reason": str(reason)}}
        for handler in list(self._handlers.get("client.disconnected", ())):
            handler(envelope)


@dataclass
class AppContext:
    """装配结果：持有 client / bridge / viewmodels 的强引用，防止被 GC。"""

    client: Any
    bridge: ContractClientBridge
    viewmodels: Dict[str, Any]

    def shutdown(self) -> None:
        try:
            self.client.close()
        except Exception as exc:  # 关闭失败只记录，不阻塞退出
            print(f"[HALO-BOOT] 关闭 Sidecar 客户端时出错：{exc}", file=sys.stderr)


def assemble(engine) -> AppContext:
    """创建 SidecarClient 与 9 个 ViewModel，并经 setContextProperty 暴露给 QML。

    必须在 engine.load() 之前调用。Sidecar 进程不可用不是装配失败——
    连接结果经 appVM 的连接状态 / 不可用原因如实展示。
    """
    try:
        from halo_studio.ipc.client import SidecarClient
    except Exception as exc:
        raise AppAssemblyError(
            "依赖缺口：IPC 客户端（halo_studio.ipc.client.SidecarClient）尚未就绪，"
            f"无法装配应用。底层错误：{exc}"
        ) from exc

    try:
        import halo_studio.viewmodels as viewmodels_module
    except Exception as exc:
        raise AppAssemblyError(
            f"依赖缺口：视图模型包（halo_studio.viewmodels）导入失败。底层错误：{exc}"
        ) from exc

    missing = [cls_name for _, cls_name in VIEWMODEL_SPECS
               if not hasattr(viewmodels_module, cls_name)]
    if missing:
        raise AppAssemblyError(
            "依赖缺口：halo_studio.viewmodels 缺少 " + "、".join(missing)
            + "（并行开发中，集成阶段统一收口）。"
        )

    try:
        client = SidecarClient()
    except Exception as exc:
        raise AppAssemblyError(f"SidecarClient 初始化失败：{exc}") from exc

    bridge = ContractClientBridge(client)

    viewmodels: Dict[str, Any] = {}
    for prop_name, cls_name in VIEWMODEL_SPECS:
        vm_cls = getattr(viewmodels_module, cls_name)
        try:
            viewmodels[prop_name] = vm_cls(bridge)
        except Exception as exc:
            raise AppAssemblyError(
                f"{cls_name} 构造失败（装配假定签名 {cls_name}(client)）：{exc}"
            ) from exc

    root_context = engine.rootContext()
    for prop_name, _ in VIEWMODEL_SPECS:
        root_context.setContextProperty(prop_name, viewmodels[prop_name])

    # 连接建立后拉取初始状态（全部走契约方法，无业务旁路）
    def _on_connected(_result: dict) -> None:
        viewmodels["workspaceVM"].refresh()
        viewmodels["runtimeVM"].refresh()
        viewmodels["configVM"].refresh()
        viewmodels["taskVM"].refresh()
        viewmodels["traceVM"].refresh()
        viewmodels["historyVM"].list()

    client.connected.connect(_on_connected)

    # Sidecar 不可用不是致命错误：spawn/hello 失败经 disconnected 信号
    # → appVM.unavailableReason 如实显示原因。
    try:
        client.start()
    except Exception as exc:
        print(f"[HALO-BOOT] Sidecar 启动请求失败（界面将显示原因）：{exc}", file=sys.stderr)

    return AppContext(client=client, bridge=bridge, viewmodels=viewmodels)
