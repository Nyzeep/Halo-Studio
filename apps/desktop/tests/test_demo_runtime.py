from pathlib import Path
import sys
import unittest


DESKTOP_ROOT = Path(__file__).resolve().parents[1]
if str(DESKTOP_ROOT) not in sys.path:
    sys.path.insert(0, str(DESKTOP_ROOT))


class DemoRuntimeTests(unittest.TestCase):
    def test_demo_runtime_emits_events_for_32_agents(self):
        from halo_desktop.demo_runtime import run_demo_agents

        events = run_demo_agents(agent_count=32)

        self.assertEqual(len(events), 32 * 7)
        self.assertEqual(events[0].kind, "run.state")
        self.assertEqual(events[-1].kind, "token.updated")

    def test_demo_runtime_uses_stable_run_ids_and_sequences(self):
        from halo_desktop.demo_runtime import run_demo_agents

        events = run_demo_agents(agent_count=4)
        for run_index in range(4):
            run_id = f"run-{run_index + 1}"
            sequence = [
                event.seq
                for event in events
                if event.run_id == run_id
            ]
            self.assertEqual(sequence, [1, 2, 3, 4, 5, 6, 7])


if __name__ == "__main__":
    unittest.main()
