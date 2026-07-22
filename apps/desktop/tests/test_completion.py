from pathlib import Path
import sys
import unittest


DESKTOP_ROOT = Path(__file__).resolve().parents[1]
if str(DESKTOP_ROOT) not in sys.path:
    sys.path.insert(0, str(DESKTOP_ROOT))


class CompletionTests(unittest.TestCase):
    def test_slash_completion_prioritizes_current_agent(self):
        from halo_desktop.completion import complete_commands, default_commands

        commands = default_commands()
        result = complete_commands(
            "/co",
            commands,
            current_agent_id="codex-cli",
            recent=(),
            favorites=("/codex",),
        )

        self.assertGreater(len(result), 0)
        self.assertEqual(result[0].name, "/codex")

    def test_completion_suggests_arguments_after_command_name(self):
        from halo_desktop.completion import complete_commands, default_commands

        result = complete_commands(
            "/codex --",
            default_commands(),
            current_agent_id="codex-cli",
            recent=(),
            favorites=(),
        )
        names = [candidate.name for candidate in result]

        self.assertIn("--continue", names)
        self.assertIn("--model", names)
        self.assertIn("--sandbox", names)

    def test_completion_suggests_arguments_after_command_space(self):
        from halo_desktop.completion import complete_commands, default_commands

        result = complete_commands(
            "/codex ",
            default_commands(),
            current_agent_id="codex-cli",
            recent=(),
            favorites=(),
        )
        names = [candidate.name for candidate in result]

        self.assertIn("--continue", names)
        self.assertIn("--model", names)


if __name__ == "__main__":
    unittest.main()
