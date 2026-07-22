from pathlib import Path
import sys
import tempfile
import unittest


DESKTOP_ROOT = Path(__file__).resolve().parents[1]
if str(DESKTOP_ROOT) not in sys.path:
    sys.path.insert(0, str(DESKTOP_ROOT))


class AppControllerTests(unittest.TestCase):
    def test_controller_imports_and_serves_demo_data_without_pyside6(self):
        from halo_desktop.app_controller import AppController

        with tempfile.TemporaryDirectory() as temp_dir:
            controller = AppController(Path(temp_dir))
            agents = controller.list_agents()
            completions = controller.complete("/co", current_agent_id="codex-cli")
            events = controller.timeline_events(agent_count=4)

        self.assertGreaterEqual(len(agents), 1)
        self.assertEqual(completions[0]["name"], "/codex")
        self.assertEqual(len(events), 4 * 7)

    def test_controller_uses_builtin_manifest_commands_for_completion(self):
        from halo_desktop.app_controller import AppController

        project_root = Path(__file__).resolve().parents[3]
        controller = AppController(project_root)

        completions = controller.complete("/te", current_agent_id="codex-cli")

        self.assertEqual(completions[0]["name"], "/test")

    def test_controller_demo_mode_does_not_touch_injected_ipc_client(self):
        from halo_desktop.app_controller import AppController

        class ExplodingIpcClient:
            def read_events_until(self, *args, **kwargs):
                raise AssertionError("demo mode should not read IPC")

        with tempfile.TemporaryDirectory() as temp_dir:
            controller = AppController(
                Path(temp_dir),
                runtime_mode="demo",
                ipc_client=ExplodingIpcClient(),
            )
            events = controller.timeline_events(agent_count=4)

        self.assertEqual(len(events), 4 * 7)

    def test_controller_ipc_mode_maps_runtime_events_for_qml(self):
        from halo_desktop.app_controller import AppController

        class FakeIpcClient:
            def cached_events(self):
                return [
                    {
                        "type": "runtimeEvent",
                        "runId": "run-1",
                        "agentId": "codex-cli",
                        "seq": 1,
                        "kind": "message.delta",
                        "message": "hello",
                    }
                ]

        with tempfile.TemporaryDirectory() as temp_dir:
            controller = AppController(
                Path(temp_dir),
                runtime_mode="ipc",
                ipc_client=FakeIpcClient(),
            )
            events = controller.timeline_events(agent_count=4)

        self.assertEqual(events[0]["title"], "message.delta - codex-cli - run-1")
        self.assertEqual(events[0]["body"], "hello")


if __name__ == "__main__":
    unittest.main()
