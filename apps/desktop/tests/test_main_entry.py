from pathlib import Path
import unittest


DESKTOP_ROOT = Path(__file__).resolve().parents[1]
MAIN_SOURCE = DESKTOP_ROOT / "halo_desktop" / "main.py"


class MainEntryTests(unittest.TestCase):
    def test_main_entry_keeps_qml_controller_alive_and_uses_basic_style(self):
        source = MAIN_SOURCE.read_text(encoding="utf-8")

        self.assertIn('QT_QUICK_CONTROLS_STYLE", "Basic"', source)
        self.assertIn("qml_controller = create_qml_controller", source)
        self.assertIn('setContextProperty("controller", qml_controller)', source)


if __name__ == "__main__":
    unittest.main()
