from __future__ import annotations

import json
from pathlib import Path
import queue
import subprocess
import threading
import time
from typing import Any, Callable, TextIO


RuntimeEventPredicate = Callable[[dict[str, Any]], bool]


class IpcClient:
    def __init__(
        self,
        process: Any,
        *,
        event_queue: "queue.Queue[dict[str, Any]] | None" = None,
    ) -> None:
        if getattr(process, "stdin", None) is None:
            raise ValueError("process must expose stdin")
        if getattr(process, "stdout", None) is None:
            raise ValueError("process must expose stdout")
        self._process = process
        self._stdin: TextIO = process.stdin
        self._stdout: TextIO = process.stdout
        self._events: list[dict[str, Any]] = []
        self._event_queue = event_queue
        self._reader_thread: threading.Thread | None = None

    @classmethod
    def start_sidecar(
        cls,
        binary_path: str | Path,
        *,
        cwd: str | Path | None = None,
    ) -> "IpcClient":
        process = subprocess.Popen(
            [str(binary_path)],
            cwd=str(cwd) if cwd else None,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            encoding="utf-8",
            bufsize=1,
        )
        return cls(process)

    def create_run(self, run_id: str, agent_id: str, prompt: str) -> None:
        self._send(
            {
                "type": "createRun",
                "runId": run_id,
                "agentId": agent_id,
                "prompt": prompt,
            }
        )

    def get_snapshot(self, run_id: str) -> None:
        self._send({"type": "getSnapshot", "runId": run_id})

    def shutdown(self) -> None:
        self._send({"type": "shutdown"})

    def read_events_until(
        self,
        predicate: RuntimeEventPredicate,
        *,
        max_lines: int = 100,
        timeout_seconds: float | None = None,
    ) -> list[dict[str, Any]]:
        events: list[dict[str, Any]] = []
        deadline = (
            time.monotonic() + timeout_seconds
            if timeout_seconds is not None
            else None
        )

        for _ in range(max_lines):
            if deadline is not None and time.monotonic() >= deadline:
                break

            event = self._read_next_event(deadline)
            if event is None:
                break

            self._events.append(event)
            events.append(event)
            if predicate(event):
                break

        return events

    def cached_events(self) -> list[dict[str, Any]]:
        return list(self._events)

    def _send(self, command: dict[str, Any]) -> None:
        line = json.dumps(command, ensure_ascii=False, separators=(",", ":"))
        self._stdin.write(line + "\n")
        self._stdin.flush()

    def _read_next_event(self, deadline: float | None) -> dict[str, Any] | None:
        event_queue = self._ensure_reader_queue()
        timeout = None
        if deadline is not None:
            timeout = max(0.0, deadline - time.monotonic())

        try:
            return event_queue.get(timeout=timeout)
        except queue.Empty:
            return None

    def _ensure_reader_queue(self) -> "queue.Queue[dict[str, Any]]":
        if self._event_queue is None:
            self._event_queue = queue.Queue()
            self._reader_thread = threading.Thread(
                target=self._read_stdout_forever,
                name="halo-runtime-reader",
                daemon=True,
            )
            self._reader_thread.start()
        return self._event_queue

    def _read_stdout_forever(self) -> None:
        while True:
            line = self._stdout.readline()
            if not line:
                break
            try:
                event = json.loads(line)
            except json.JSONDecodeError as error:
                event = {
                    "type": "error",
                    "message": f"invalid sidecar json: {error.msg}",
                }
            self._event_queue.put(event)
