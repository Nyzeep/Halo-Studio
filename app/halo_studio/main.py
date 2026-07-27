"""Halo Studio 入口：QGuiApplication + QQmlApplicationEngine 加载 Main.qml。

--smoke：加载成功且根对象存在 → stdout 打印 SMOKE-OK 并以退出码 0 结束；
失败返回非 0。Sidecar 不可用不属于烟测失败（界面如实显示原因即可）。
"""

from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

from PySide6.QtCore import QUrl
from PySide6.QtGui import QGuiApplication
from PySide6.QtQml import QQmlApplicationEngine
from PySide6.QtQuickControls2 import QQuickStyle

from halo_studio.app import AppAssemblyError, assemble

EXIT_OK = 0
EXIT_ASSEMBLY_FAILED = 3
EXIT_QML_FAILED = 4

QML_MAIN = Path(__file__).resolve().parent / "qml" / "Main.qml"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="python -m halo_studio.main",
        description="Halo Studio —— Pi/OpenCode 可验证编码交付原生工作台",
    )
    parser.add_argument("--smoke", action="store_true",
                        help="烟测模式：QML 加载成功后打印 SMOKE-OK 并退出")
    args, qt_args = parser.parse_known_args(sys.argv[1:] if argv is None else argv)

    QQuickStyle.setStyle("Fusion")
    app = QGuiApplication([sys.argv[0], *qt_args])
    app.setApplicationName("Halo Studio")
    app.setOrganizationName("HaloStudio")
    app.setApplicationVersion("0.1.0")

    engine = QQmlApplicationEngine()

    try:
        context = assemble(engine)
    except AppAssemblyError as exc:
        print(f"[HALO-BOOT] 应用装配失败：{exc}", file=sys.stderr)
        return EXIT_ASSEMBLY_FAILED

    engine.load(QUrl.fromLocalFile(str(QML_MAIN)))
    if not engine.rootObjects():
        print(f"[HALO-BOOT] QML 加载失败：根对象不存在（{QML_MAIN}）", file=sys.stderr)
        context.shutdown()
        return EXIT_QML_FAILED

    if args.smoke:
        context.shutdown()
        print("SMOKE-OK", flush=True)
        # 烟测契约要求确定性退出码 0：优雅关闭已完成，直接终止进程，
        # 避免 Qt/线程清理顺序影响退出码。
        os._exit(EXIT_OK)

    exit_code = app.exec()
    context.shutdown()
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
