from __future__ import annotations

import io
import queue
import subprocess
from pathlib import Path
import sys
import unittest


DESKTOP_ROOT = Path(__file__).resolve().parents[1]
if str(DESKTOP_ROOT) not in sys.path:
    sys.path.insert(0, str(DESKTOP_ROOT))


class FakeStdin:
    def __init__(self) -> None:
        self.lines: list[str] = []
        self.flush_count = 0

    def write(self, value: str) -> int:
        self.lines.append(value.rstrip("\n"))
        return len(value)

    def flush(self) -> None:
        self.flush_count += 1


class FakeProcess:
    def __init__(self, stdout_lines: list[str] | None = None) -> None:
        self.stdin = FakeStdin()
        self.stdout = io.StringIO("".join(stdout_lines or []))


class IpcClientTests(unittest.TestCase):
    def test_ipc_client_serializes_create_run_command(self):
        from halo_desktop.ipc_client import IpcClient

        process = FakeProcess()
        client = IpcClient(process)

        client.create_run("run-1", "codex-cli", "hello")

        self.assertEqual(
            process.stdin.lines[0],
            '{"type":"createRun","runId":"run-1","agentId":"codex-cli","prompt":"hello"}',
        )
        self.assertEqual(process.stdin.flush_count, 1)

    def test_ipc_client_reads_runtime_events_until_predicate_matches(self):
        from halo_desktop.ipc_client import IpcClient

        process = FakeProcess(
            [
                '{"type":"runtimeEvent","runId":"run-1","agentId":"codex-cli","seq":1,"kind":"run.state","message":"running"}\n',
                '{"type":"runtimeEvent","runId":"run-1","agentId":"codex-cli","seq":2,"kind":"message.delta","message":"hello"}\n',
            ]
        )
        client = IpcClient(process)

        events = client.read_events_until(
            lambda event: event["type"] == "runtimeEvent" and event["seq"] == 2,
            max_lines=8,
        )

        self.assertEqual([event["seq"] for event in events], [1, 2])

    def test_ipc_client_serializes_snapshot_and_shutdown_commands(self):
        from halo_desktop.ipc_client import IpcClient

        process = FakeProcess()
        client = IpcClient(process)

        client.get_snapshot("run-1")
        client.shutdown()

        self.assertEqual(
            process.stdin.lines,
            [
                '{"type":"getSnapshot","runId":"run-1"}',
                '{"type":"shutdown"}',
            ],
        )

    def test_ipc_client_timeout_does_not_call_blocking_readline_when_queue_is_empty(self):
        from halo_desktop.ipc_client import IpcClient

        class BlockingStdout:
            def readline(self):
                raise AssertionError("readline should stay on the reader thread")

        process = FakeProcess()
        process.stdout = BlockingStdout()
        client = IpcClient(process, event_queue=queue.Queue())

        events = client.read_events_until(lambda event: False, timeout_seconds=0.01)

        self.assertEqual(events, [])

    def test_ipc_client_reader_converts_malformed_json_to_error_event(self):
        from halo_desktop.ipc_client import IpcClient

        process = FakeProcess(["not json\n"])
        client = IpcClient(process)

        events = client.read_events_until(
            lambda event: event["type"] == "error",
            timeout_seconds=0.2,
        )

        self.assertEqual(events[0]["type"], "error")
        self.assertIn("invalid sidecar json", events[0]["message"])

    def test_start_sidecar_discards_stderr_to_avoid_pipe_backpressure(self):
        from halo_desktop import ipc_client

        captured: dict[str, object] = {}

        class FakePopen:
            def __init__(self, *args, **kwargs):
                captured.update(kwargs)
                self.stdin = FakeStdin()
                self.stdout = io.StringIO("")

        original_popen = ipc_client.subprocess.Popen
        try:
            ipc_client.subprocess.Popen = FakePopen
            ipc_client.IpcClient.start_sidecar("halo-runtime.exe")
        finally:
            ipc_client.subprocess.Popen = original_popen

        self.assertEqual(captured["stderr"], subprocess.DEVNULL)


if __name__ == "__main__":
    unittest.main()
