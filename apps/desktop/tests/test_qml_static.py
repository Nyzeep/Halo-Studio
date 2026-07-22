from pathlib import Path
import unittest


DESKTOP_ROOT = Path(__file__).resolve().parents[1]
QML_ROOT = DESKTOP_ROOT / "halo_desktop" / "qml"


def read_main_qml():
    return (QML_ROOT / "Main.qml").read_text(encoding="utf-8")


def read_all_qml():
    qml_files = sorted(QML_ROOT.rglob("*.qml"))
    if not qml_files:
        raise AssertionError("No QML files found")
    return "\n".join(path.read_text(encoding="utf-8") for path in qml_files)


class QmlStaticTests(unittest.TestCase):
    def test_qml_avoids_expensive_animation_patterns(self):
        qml_text = read_all_qml()
        banned = [
            "ParticleSystem",
            "ShaderEffect",
            "DropShadow",
            "FastBlur",
            "NumberAnimation on x",
            "NumberAnimation on y",
        ]

        for token in banned:
            self.assertNotIn(token, qml_text)

    def test_main_qml_contains_required_workspace_regions(self):
        main_qml = read_main_qml()

        for token in [
            "AgentSidebar",
            "WorkflowTimeline",
            "InspectorPanel",
            "CommandComposer",
        ]:
            self.assertIn(token, main_qml)

    def test_debug_terminal_drawer_is_collapsed_by_default(self):
        qml_text = read_all_qml()

        self.assertIn("property bool debugDrawerOpen: false", qml_text)
        self.assertIn("Debug", qml_text)


if __name__ == "__main__":
    unittest.main()
