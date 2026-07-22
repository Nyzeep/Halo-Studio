from pathlib import Path
import sys
import tempfile
import textwrap
import unittest


DESKTOP_ROOT = Path(__file__).resolve().parents[1]
if str(DESKTOP_ROOT) not in sys.path:
    sys.path.insert(0, str(DESKTOP_ROOT))


class PluginRegistryTests(unittest.TestCase):
    def test_registry_loads_agent_manifest_from_project_plugins(self):
        from halo_desktop.plugin_registry import PluginRegistry

        with tempfile.TemporaryDirectory() as temp_dir:
            project_root = Path(temp_dir)
            manifest_dir = project_root / "plugins" / "agents" / "codex-cli"
            manifest_dir.mkdir(parents=True)
            (manifest_dir / "agent.toml").write_text(
                textwrap.dedent(
                    """
                    id = "codex-cli"
                    name = "Codex CLI"
                    provider = "openai"
                    transport = "pty"
                    command = "codex"
                    capabilities = ["code", "review"]

                    [[commands]]
                    name = "/codex"
                    description = "Switch to Codex"
                    args = ["--continue"]

                    [[commands]]
                    name = "/review"
                    description = "Review changes"
                    args = ["--strict"]
                    """
                ).strip(),
                encoding="utf-8",
            )

            agents = PluginRegistry(project_root).load_agents()

        self.assertEqual([agent.id for agent in agents], ["codex-cli"])
        self.assertEqual(agents[0].transport, "pty")
        self.assertEqual(agents[0].capabilities, ("code", "review"))
        self.assertEqual(agents[0].commands, ("/codex", "/review"))

    def test_builtin_agents_are_loaded_from_phase_one_manifests(self):
        from halo_desktop.plugin_registry import PluginRegistry

        project_root = Path(__file__).resolve().parents[3]
        agents = PluginRegistry(project_root).load_agents()

        self.assertEqual(
            {agent.id for agent in agents},
            {"claude-code", "codex-cli", "opencode", "pi"},
        )
        self.assertTrue(all(agent.transport == "pty" for agent in agents))
        self.assertIn("/codex", next(agent.commands for agent in agents if agent.id == "codex-cli"))

    def test_registry_returns_empty_list_when_manifest_folder_is_missing(self):
        from halo_desktop.plugin_registry import PluginRegistry

        with tempfile.TemporaryDirectory() as temp_dir:
            agents = PluginRegistry(Path(temp_dir)).load_agents()

        self.assertEqual(agents, [])


if __name__ == "__main__":
    unittest.main()
