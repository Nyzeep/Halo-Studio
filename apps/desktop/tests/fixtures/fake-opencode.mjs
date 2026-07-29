#!/usr/bin/env node

import { createServer } from "node:http";
import { timingSafeEqual } from "node:crypto";

// This intentionally implements only the local OpenCode server contract used
// by desktop integration tests. It accepts `serve --hostname 127.0.0.1 --port`
// and never listens on a non-loopback address.
//
// HALO_FAKE_OPENCODE_MODE (or HALO_FAKE_MODE) accepts comma-separated values:
// - health-401, health-500, health-500-once, version-mismatch
// - heartbeat (named SSE heartbeat events)
// - unexpected-exit (closes the server shortly after it starts)
// The aliases `unauthorized`, `server-error`, `wrong-version`, and `crash`
// are also accepted. HALO_FAKE_EXIT_DELAY_MS can tune the crash delay for a
// test, within a bounded range.

const OPENCODE_VERSION = "1.18.4";
const LOOPBACK_HOST = "127.0.0.1";

function readModes(prefix) {
  return (process.env[prefix] ?? "")
    .split(/[\s,;]+/u)
    .map((value) => value.trim().toLowerCase())
    .filter(Boolean);
}

const modes = new Set([
  ...readModes("HALO_FAKE_OPENCODE_MODE"),
  ...readModes("HALO_STUDIO_FAKE_OPENCODE_MODE"),
  ...readModes("FAKE_OPENCODE_MODE"),
  ...readModes("OPENCODE_FAKE_MODE"),
  ...readModes("HALO_FAKE_MODE"),
  ...readModes("HALO_STUDIO_TEST_MODE"),
  ...readModes("HALO_TEST_MODE"),
]);

function hasMode(...names) {
  return names.some((name) => modes.has(name));
}

function fail(message) {
  process.stderr.write(`fake-opencode: ${message}\n`);
  process.exitCode = 1;
}

function parseServeArguments(args) {
  if (args[0] !== "serve") return undefined;
  let hostname;
  let port;
  for (let index = 1; index < args.length; index += 1) {
    if (args[index] === "--hostname") hostname = args[index + 1];
    if (args[index] === "--port") port = args[index + 1];
  }
  if (hostname !== LOOPBACK_HOST || port === undefined || !/^\d+$/u.test(port)) return undefined;
  const parsedPort = Number(port);
  if (!Number.isInteger(parsedPort) || parsedPort < 0 || parsedPort > 65_535) return undefined;
  return { port: parsedPort };
}

function credentialsFromEnvironment() {
  const username = process.env.OPENCODE_SERVER_USERNAME;
  const password = process.env.OPENCODE_SERVER_PASSWORD;
  if (username !== "opencode" || password === undefined) return undefined;
  return { username, password };
}

function expectedAuthorization(credentials) {
  return `Basic ${Buffer.from(`${credentials.username}:${credentials.password}`, "utf8").toString("base64")}`;
}

function authorizationMatches(actual, expected) {
  if (typeof actual !== "string") return false;
  const actualBytes = Buffer.from(actual, "utf8");
  const expectedBytes = Buffer.from(expected, "utf8");
  return actualBytes.length === expectedBytes.length && timingSafeEqual(actualBytes, expectedBytes);
}

function sendJson(response, status, payload) {
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "cache-control": "no-store",
  });
  response.end(`${JSON.stringify(payload)}\n`);
}

function boundedExitDelay() {
  const parsed = Number(process.env.HALO_FAKE_EXIT_DELAY_MS ?? 250);
  if (!Number.isFinite(parsed)) return 250;
  return Math.max(10, Math.min(5_000, Math.floor(parsed)));
}

const serve = parseServeArguments(process.argv.slice(2));
const credentials = credentialsFromEnvironment();
if (!serve) {
  fail("only 'serve --hostname 127.0.0.1 --port <port>' is supported");
} else if (!credentials) {
  fail("OPENCODE_SERVER_USERNAME=opencode and OPENCODE_SERVER_PASSWORD are required");
} else {
  const expectedAuth = expectedAuthorization(credentials);
  const sseClients = new Map();
  const sockets = new Set();
  const sessions = new Map();
  const sessionMessages = new Map();
  let healthRequests = 0;
  let sessionNumber = 0;
  let closing = false;
  let unexpectedExitTimer;

  const closeSseClients = () => {
    for (const [response, heartbeat] of sseClients) {
      clearInterval(heartbeat);
      if (!response.writableEnded) response.end();
    }
    sseClients.clear();
  };

  const shutdown = (code = 0, force = false) => {
    if (closing) return;
    closing = true;
    if (unexpectedExitTimer !== undefined) clearTimeout(unexpectedExitTimer);
    closeSseClients();
    for (const socket of sockets) socket.destroy();
    server.close(() => {
      process.exitCode = code;
      if (force) process.exit(code);
    });
    // A keep-alive socket cannot be allowed to keep a fake test server alive.
    const fallback = setTimeout(() => {
      process.exitCode = code;
      if (force) process.exit(code);
    }, 250);
    fallback.unref();
  };

  const writeSse = (response, event, data) => {
    if (response.writableEnded || response.destroyed) return;
    response.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
  };

  const sessionInfo = (session) => ({
    id: session.id,
    title: session.title,
    time: { updated: session.updated },
  });

  const broadcast = (type, properties) => {
    for (const response of sseClients.keys()) {
      writeSse(response, "message", {
        directory: process.cwd(),
        payload: { type, properties },
      });
    }
  };

  const createSession = () => {
    sessionNumber += 1;
    const session = {
      id: `fake-opencode-session-${sessionNumber}`,
      title: `Fake OpenCode ${sessionNumber}`,
      updated: Date.now(),
    };
    sessions.set(session.id, session);
    sessionMessages.set(session.id, []);
    return session;
  };

  const authorize = (request, response) => {
    if (authorizationMatches(request.headers.authorization, expectedAuth)) return true;
    response.writeHead(401, {
      "content-type": "application/json; charset=utf-8",
      "www-authenticate": 'Basic realm="opencode"',
      "cache-control": "no-store",
    });
    response.end('{"error":"unauthorized"}\n');
    return false;
  };

  const server = createServer((request, response) => {
    const pathname = new URL(request.url ?? "/", `http://${LOOPBACK_HOST}`).pathname;
    if (!authorize(request, response)) return;
    if (pathname === "/global/health") {
      if (request.method !== "GET") {
        sendJson(response, 405, { error: "method-not-allowed" });
        return;
      }
      healthRequests += 1;
      if (hasMode("health-401", "unauthorized")) {
        sendJson(response, 401, { error: "forced-unauthorized" });
        return;
      }
      const forceOneServerError = hasMode("health-500-once") && healthRequests === 1;
      if (hasMode("health-500", "server-error") || forceOneServerError) {
        sendJson(response, 500, { error: "forced-server-error" });
        return;
      }
      sendJson(response, 200, {
        version: hasMode("version-mismatch", "wrong-version") ? "0.0.0-test" : OPENCODE_VERSION,
      });
      return;
    }
    if (pathname === "/global/event") {
      if (request.method !== "GET") {
        sendJson(response, 405, { error: "method-not-allowed" });
        return;
      }
      response.writeHead(200, {
        "content-type": "text/event-stream; charset=utf-8",
        "cache-control": "no-cache, no-transform",
        connection: "keep-alive",
      });
      response.flushHeaders?.();
      writeSse(response, "connected", { type: "server.connected", source: "fake-opencode" });
      let heartbeat;
      if (hasMode("heartbeat", "sse-heartbeat")) {
        const emitHeartbeat = () => writeSse(response, "heartbeat", { type: "server.heartbeat" });
        emitHeartbeat();
        heartbeat = setInterval(emitHeartbeat, 100);
      }
      sseClients.set(response, heartbeat);
      const remove = () => {
        const interval = sseClients.get(response);
        if (interval !== undefined) clearInterval(interval);
        sseClients.delete(response);
      };
      request.once("aborted", remove);
      response.once("close", remove);
      return;
    }
    if (pathname === "/session" && request.method === "GET") {
      sendJson(response, 200, [...sessions.values()].map(sessionInfo));
      return;
    }
    if (pathname === "/session" && request.method === "POST") {
      const session = createSession();
      sendJson(response, 200, sessionInfo(session));
      broadcast("session.created", { info: sessionInfo(session) });
      return;
    }
    const sessionMatch = /^\/session\/([^/]+)(?:\/(message|prompt_async|abort))?$/u.exec(pathname);
    if (sessionMatch) {
      const sessionId = decodeURIComponent(sessionMatch[1]);
      const action = sessionMatch[2];
      const session = sessions.get(sessionId);
      if (!session) {
        sendJson(response, 404, { error: "missing-session" });
        return;
      }
      if (action === undefined && request.method === "GET") {
        sendJson(response, 200, sessionInfo(session));
        return;
      }
      if (action === "message" && request.method === "GET") {
        sendJson(response, 200, sessionMessages.get(sessionId) ?? []);
        return;
      }
      if (action === "abort" && request.method === "POST") {
        sendJson(response, 200, true);
        broadcast("session.idle", { sessionID: sessionId });
        return;
      }
      if (action === "prompt_async" && request.method === "POST") {
        let body = "";
        request.setEncoding("utf8");
        request.on("data", (chunk) => { body += chunk; });
        request.once("end", () => {
          let text = "";
          try {
            const parsed = JSON.parse(body);
            const part = Array.isArray(parsed.parts) ? parsed.parts.find((candidate) => candidate?.type === "text") : undefined;
            if (typeof part?.text === "string") text = part.text;
          } catch {
            sendJson(response, 400, { error: "invalid-prompt" });
            return;
          }
          if (text.length === 0) {
            sendJson(response, 400, { error: "empty-prompt" });
            return;
          }
          const ordinal = (sessionMessages.get(sessionId) ?? []).length;
          const messageId = `${sessionId}-message-${ordinal + 1}`;
          const message = {
            info: { sessionID: sessionId, id: messageId, role: "user" },
            parts: [{ type: "text", sessionID: sessionId, messageID: messageId, text }],
          };
          const messages = sessionMessages.get(sessionId) ?? [];
          messages.push(message);
          sessionMessages.set(sessionId, messages);
          session.updated = Date.now();
          response.writeHead(204, { "cache-control": "no-store" });
          response.end();
          broadcast("session.status", { sessionID: sessionId, status: { type: "busy" } });
          broadcast("message.updated", { info: message.info });
          broadcast("message.part.updated", { part: message.parts[0], delta: text });
        });
        return;
      }
    }
    sendJson(response, 404, { error: "not-found" });
  });

  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.once("close", () => sockets.delete(socket));
  });
  server.once("error", (error) => {
    fail(error instanceof Error ? error.message : "server error");
    shutdown(1, true);
  });
  server.listen({ host: LOOPBACK_HOST, port: serve.port }, () => {
    const address = server.address();
    if (!address || typeof address === "string") {
      fail("server did not provide a TCP address");
      shutdown(1, true);
      return;
    }
    // This exact line is the production adapter's readiness protocol.
    process.stdout.write(`opencode server listening on http://${LOOPBACK_HOST}:${address.port}\n`);
    if (hasMode("unexpected-exit", "crash")) {
      unexpectedExitTimer = setTimeout(() => shutdown(0, true), boundedExitDelay());
    }
  });
  process.stdin.on("end", () => shutdown(0));
  process.stdin.on("error", () => shutdown(0));
  process.stdin.resume();
}
