#!/usr/bin/env node

import { StringDecoder } from "node:string_decoder";

// This intentionally implements only the tiny Pi surface exercised by desktop
// integration tests. Modes are opt-in and never affect production discovery.
//
// HALO_FAKE_PI_MODE (or HALO_FAKE_MODE) accepts a comma-separated list:
// - invalid-json: write malformed JSONL after the next command
// - stderr: emit a deterministic stderr line at startup
// - eof: close stdout after the next command without replying
// - agent-retry: emit `agent_end` with `willRetry: true` for a prompt
// - agent-settled: emit `agent_settled` for a prompt
// `HALO_FAKE_TEST_MODE=1` / `HALO_STUDIO_TEST_MODE=1` only marks state data as
// test-mode data, allowing a caller to make that fact observable without a
// second executable.

const PI_VERSION = "0.81.1";
const GET_STATE_BATCH_DELAY_MS = 8;

function readModes(prefix) {
  return (process.env[prefix] ?? "")
    .split(/[\s,;]+/u)
    .map((value) => value.trim().toLowerCase())
    .filter(Boolean);
}

const modes = new Set([
  ...readModes("HALO_FAKE_PI_MODE"),
  ...readModes("HALO_STUDIO_FAKE_PI_MODE"),
  ...readModes("FAKE_PI_MODE"),
  ...readModes("PI_FAKE_MODE"),
  ...readModes("HALO_FAKE_MODE"),
  ...readModes("HALO_STUDIO_TEST_MODE"),
  ...readModes("HALO_TEST_MODE"),
]);
const testMode = ["1", "true", "yes"].includes(
  (process.env.HALO_FAKE_TEST_MODE ?? process.env.HALO_STUDIO_TEST_MODE ?? process.env.HALO_TEST_MODE ?? "").toLowerCase(),
);

function hasMode(...names) {
  return names.some((name) => modes.has(name));
}

function writeJson(value) {
  if (!process.stdout.writableEnded) process.stdout.write(`${JSON.stringify(value)}\n`);
}

function endOutput(forceExit = false) {
  const finished = () => {
    process.exitCode = 0;
    if (forceExit) process.exit(0);
  };
  if (!process.stdout.writableEnded) process.stdout.end(finished);
  else finished();
  process.exitCode = 0;
}

function unsupportedInvocation() {
  process.stderr.write("fake-pi only supports --version or --mode rpc\n");
  process.exitCode = 1;
}

function response(command, data = {}) {
  return {
    type: "response",
    ...(typeof command.id === "string" && command.id.length > 0 ? { id: command.id } : {}),
    command: typeof command.type === "string" ? command.type : "unknown",
    success: true,
    data,
  };
}

function invalidCommand(command) {
  return {
    type: "response",
    ...(typeof command?.id === "string" && command.id.length > 0 ? { id: command.id } : {}),
    command: typeof command?.type === "string" ? command.type : "unknown",
    success: false,
    error: { code: "invalid-command" },
  };
}

async function runRpc() {
  if (hasMode("stderr", "emit-stderr")) process.stderr.write("fake-pi: deterministic test stderr\n");

  const decoder = new StringDecoder("utf8");
  let buffer = "";
  let closed = false;
  let malformedWritten = false;
  let getStateTimer;
  const pendingGetState = [];
  let sessionNumber = 1;
  let messages = [];

  const sessionState = () => ({
    sessionId: `fake-pi-session-${sessionNumber}`,
    sessionName: `Fake Pi ${sessionNumber}`,
    isStreaming: false,
    isCompacting: false,
    messageCount: messages.length,
    pendingMessageCount: 0,
  });

  const close = (forceExit = false) => {
    if (closed) return;
    closed = true;
    if (getStateTimer !== undefined) clearTimeout(getStateTimer);
    endOutput(forceExit);
  };

  const respond = (command) => {
    if (closed) return;
    if (hasMode("invalid-json", "malformed-json") && !malformedWritten) {
      malformedWritten = true;
      process.stdout.write('{"type":"response"\n');
      close(true);
      return;
    }
    if (hasMode("eof", "close-stdout")) {
      close(true);
      return;
    }

    if (!command || typeof command !== "object" || Array.isArray(command) || typeof command.type !== "string") {
      writeJson(invalidCommand(command));
      return;
    }

    if (command.type === "get_state") {
      writeJson(response(command, sessionState()));
      return;
    }

    if (command.type === "new_session") {
      sessionNumber += 1;
      messages = [];
      writeJson(response(command, { cancelled: false }));
      return;
    }

    if (command.type === "get_messages") {
      writeJson(response(command, { messages }));
      return;
    }

    if (command.type === "get_commands") {
      writeJson(response(command, {
        commands: [{
          name: "compact",
          description: "Compact the current fake session.",
          source: "prompt",
        }],
      }));
      return;
    }

    if (command.type === "prompt") {
      messages.push({ role: "user", content: command.message });
      messages.push({ role: "assistant", content: "Fake Pi accepted the prompt." });
      writeJson({ type: "agent_start", data: { source: "fake-pi" } });
      writeJson(response(command, { accepted: true }));
      if (hasMode("agent-retry", "agent-end-retry", "retry")) {
        writeJson({ type: "agent_end", data: { willRetry: true } });
      }
      if (hasMode("agent-settled", "settled")) writeJson({ type: "agent_settled", data: {} });
      return;
    }

    if (command.type === "steer" || command.type === "abort") {
      writeJson(response(command, { accepted: true }));
      return;
    }

    writeJson(invalidCommand(command));
  };

  const flushGetState = () => {
    if (getStateTimer !== undefined) clearTimeout(getStateTimer);
    getStateTimer = undefined;
    // A short batch window intentionally reverses concurrent get_state replies.
    // It verifies clients route by request id rather than FIFO response order.
    const batch = pendingGetState.splice(0).reverse();
    for (const command of batch) respond(command);
  };

  const scheduleGetState = () => {
    if (getStateTimer !== undefined) return;
    getStateTimer = setTimeout(flushGetState, GET_STATE_BATCH_DELAY_MS);
  };

  const handleLine = (line) => {
    if (closed || line.trim() === "") return;
    let command;
    try {
      command = JSON.parse(line);
    } catch {
      writeJson(invalidCommand(undefined));
      return;
    }
    if (command && typeof command === "object" && !Array.isArray(command) && command.type === "get_state") {
      pendingGetState.push(command);
      scheduleGetState();
      return;
    }
    respond(command);
  };

  const consume = (text) => {
    buffer += text;
    let newline = buffer.indexOf("\n");
    while (newline >= 0) {
      const line = buffer.slice(0, newline).replace(/\r$/u, "");
      buffer = buffer.slice(newline + 1);
      handleLine(line);
      newline = buffer.indexOf("\n");
    }
  };

  process.stdin.on("data", (chunk) => consume(decoder.write(chunk)));
  process.stdin.on("end", () => {
    const tail = decoder.end();
    if (tail.length > 0) consume(tail);
    if (buffer.trim().length > 0) handleLine(buffer.replace(/\r$/u, ""));
    flushGetState();
    close();
  });
  process.stdin.on("error", close);
  process.stdin.resume();
}

const argumentsAfterNode = process.argv.slice(2);
if (argumentsAfterNode.includes("--version")) {
  process.stdout.write(`${PI_VERSION}\n`);
} else {
  const modeIndex = argumentsAfterNode.indexOf("--mode");
  if (modeIndex < 0 || argumentsAfterNode[modeIndex + 1] !== "rpc") unsupportedInvocation();
  else await runRpc();
}
