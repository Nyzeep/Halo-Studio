"""IPC 契约客户端：connection（纯 Python）与 client（Qt 包装）。

本包顶层只再导出 connection 层符号，保证不引入 Qt 依赖；
Qt 包装请显式 `from halo_studio.ipc.client import SidecarClient`。
"""

from .connection import (  # noqa: F401
    MAX_LINE_BYTES,
    PROTOCOL_VERSION,
    ConnectionClosedError,
    HelloNegotiationError,
    RequestError,
    RequestTimeoutError,
    SidecarConnection,
    SidecarError,
    default_sidecar_exe,
)
