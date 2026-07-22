from __future__ import annotations

import os
from pathlib import Path
import sys

from .app_controller import create_qml_controller


def main(argv: list[str] | None = None) -> int:
    os.environ.setdefault("QT_QUICK_CONTROLS_STYLE", "Basic")

    try:
        from PySide6.QtCore import QUrl
        from PySide6.QtGui import QGuiApplication
        from PySide6.QtQml import QQmlApplicationEngine
    except ModuleNotFoundError:
        print(
            "PySide6 is not installed. Install it with: "
            "python -m pip install -r apps/desktop/requirements.txt"
        )
        return 1

    app = QGuiApplication(argv or sys.argv)
    engine = QQmlApplicationEngine()
    runtime_mode = _runtime_mode_from_env()
    qml_controller = create_qml_controller(
        _default_project_root(),
        runtime_mode=runtime_mode,
    )
    engine.rootContext().setContextProperty("controller", qml_controller)

    qml_path = Path(__file__).resolve().parent / "qml" / "Main.qml"
    engine.load(QUrl.fromLocalFile(str(qml_path)))
    if not engine.rootObjects():
        return 2
    return int(app.exec())


def _default_project_root() -> Path:
    return Path(__file__).resolve().parents[3]


def _runtime_mode_from_env() -> str:
    mode = os.environ.get("HALO_RUNTIME_MODE", "demo").strip().lower()
    return mode if mode in {"demo", "ipc"} else "demo"


if __name__ == "__main__":
    raise SystemExit(main())
