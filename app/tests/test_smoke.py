"""烟测与 QML 静态红线检查（docs/module-contracts.md 第 8 节）。

运行方式：cd "D:\\Halo Studio ultra\\app" 后
"D:\\Halo Studio ultra\\.venv\\Scripts\\python.exe" -m pytest tests/test_smoke.py -q
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import pytest

APP_DIR = Path(__file__).resolve().parents[1]
QML_DIR = APP_DIR / "halo_studio" / "qml"
VENV_PYTHON = Path(r"D:\Halo Studio ultra\.venv\Scripts\python.exe")

if str(APP_DIR) not in sys.path:
    sys.path.insert(0, str(APP_DIR))

REQUIRED_VIEWMODELS = (
    "AppViewModel",
    "WorkspaceViewModel",
    "ConfigViewModel",
    "RuntimeViewModel",
    "TaskViewModel",
    "TraceViewModel",
    "ReviewViewModel",
    "HandoffViewModel",
    "HistoryViewModel",
)

CONTEXT_PROPERTY_NAMES = (
    "appVM", "workspaceVM", "configVM", "runtimeVM", "taskVM",
    "traceVM", "reviewVM", "handoffVM", "historyVM",
)


def _python_exe() -> str:
    return str(VENV_PYTHON) if VENV_PYTHON.exists() else sys.executable


def _subprocess_env() -> dict:
    env = dict(os.environ)
    env["QT_QPA_PLATFORM"] = "offscreen"
    env["PYTHONPATH"] = str(APP_DIR) + os.pathsep + env.get("PYTHONPATH", "")
    env["PYTHONIOENCODING"] = "utf-8"
    return env


def _run(args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        [_python_exe(), *args],
        cwd=str(APP_DIR),
        env=_subprocess_env(),
        capture_output=True,
        encoding="utf-8",
        errors="replace",
        timeout=180,
    )


def _deps_ready() -> bool:
    """并行开发的 ipc/viewmodels 是否已就绪（未就绪时烟测按依赖缺口路径断言）。"""
    try:
        from halo_studio.ipc.client import SidecarClient  # noqa: F401
        import halo_studio.viewmodels as viewmodels_module
    except Exception:
        return False
    return all(hasattr(viewmodels_module, name) for name in REQUIRED_VIEWMODELS)


def test_smoke_main_ok():
    """--smoke：加载成功且根对象存在 → 输出 SMOKE-OK 且退出码 0。"""
    if not _deps_ready():
        pytest.skip("依赖缺口：halo_studio.ipc.client / halo_studio.viewmodels 尚未就绪（并行开发，集成阶段收口）")
    proc = _run(["-m", "halo_studio.main", "--smoke"])
    assert proc.returncode == 0, (
        f"smoke 退出码 {proc.returncode}\nstdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
    )
    assert "SMOKE-OK" in proc.stdout


def test_smoke_main_reports_missing_dependencies():
    """依赖缺失时 --smoke 必须以非 0 退出并给出明确中文报错（不得内置假 ViewModel 回退）。"""
    if _deps_ready():
        pytest.skip("依赖已就绪，本用例只覆盖并行开发期的缺口报错路径")
    proc = _run(["-m", "halo_studio.main", "--smoke"])
    assert proc.returncode != 0
    assert "依赖缺口" in (proc.stderr + proc.stdout)


# 独立于 ipc/viewmodels 的 QML 加载探针：用最小 QObject 桩充当 9 个上下文属性
# （测试替身仅存在于 app/tests/，符合模块契约第 0 节纪律）。
_PROBE_SOURCE = r"""
import os, sys
os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
from pathlib import Path
from PySide6.QtCore import QObject, QUrl
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine
from PySide6.QtQuickControls2 import QQuickStyle

QQuickStyle.setStyle("Fusion")
app = QGuiApplication(sys.argv)
engine = QQmlApplicationEngine()
names = ["appVM", "workspaceVM", "configVM", "runtimeVM", "taskVM",
         "traceVM", "reviewVM", "handoffVM", "historyVM"]
stubs = [QObject() for _ in names]
for name, stub in zip(names, stubs):
    engine.rootContext().setContextProperty(name, stub)
qml = Path.cwd() / "halo_studio" / "qml" / "Main.qml"
engine.load(QUrl.fromLocalFile(str(qml)))
ok = bool(engine.rootObjects())
print("QML-PROBE-OK" if ok else "QML-PROBE-FAIL", flush=True)
os._exit(0 if ok else 5)
"""


def test_qml_loads_with_stub_context():
    """Main.qml 必须能在 offscreen 下完成加载（根对象存在），与并行模块解耦。"""
    proc = _run(["-c", _PROBE_SOURCE])
    assert proc.returncode == 0, (
        f"QML 加载失败（退出码 {proc.returncode}）\nstdout:\n{proc.stdout}\nstderr:\n{proc.stderr}"
    )
    assert "QML-PROBE-OK" in proc.stdout


def test_qml_static_no_web_components():
    """红线：qml 目录不得出现 WebEngineView / WebView（含任何 WebEngine/WebView import）。"""
    files = [f for f in sorted(QML_DIR.rglob("*")) if f.is_file()]
    assert files, "qml 目录不应为空"
    for f in files:
        content = f.read_text(encoding="utf-8")
        for banned in ("WebEngine", "WebView"):
            assert banned not in content, f"{f} 含被禁字样 {banned}"


def test_review_diff_component_is_readonly():
    """红线：审查 Diff 组件必须 readOnly，且审查页确实使用该组件。"""
    diff_viewer = QML_DIR / "components" / "DiffViewer.qml"
    assert diff_viewer.exists(), "缺少只读 Diff 组件 components/DiffViewer.qml"
    assert "readOnly: true" in diff_viewer.read_text(encoding="utf-8")
    review = QML_DIR / "views" / "ReviewView.qml"
    assert review.exists(), "缺少审查视图 views/ReviewView.qml"
    assert "DiffViewer" in review.read_text(encoding="utf-8")


def test_main_qml_exposes_required_context_names():
    """Main.qml 与视图必须只经约定的 9 个上下文属性名访问视图模型。"""
    qml_text = "\n".join(
        f.read_text(encoding="utf-8") for f in sorted(QML_DIR.rglob("*.qml"))
    )
    for name in CONTEXT_PROPERTY_NAMES:
        assert name in qml_text, f"QML 未引用上下文属性 {name}"
