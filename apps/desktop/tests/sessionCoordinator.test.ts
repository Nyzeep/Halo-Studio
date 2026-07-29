import { describe, expect, it, vi } from "vitest";

import type {
  AgentEventEnvelope,
  AgentKind,
  CommandDescriptor,
  SessionHistory,
  SessionSummary,
} from "@halo-studio/contracts";
import {
  createSessionCoordinator,
  type ManagedSessionAdapter,
} from "../src/main/sessionCoordinator.js";

const workspaceId = "a".repeat(64);

function session(agentKind: AgentKind, sessionId: string, active = false): SessionSummary {
  return { agentKind, sessionId, title: sessionId, active };
}

function adapter(agentKind: AgentKind, sessions: SessionSummary[]): ManagedSessionAdapter {
  return {
    agentKind,
    snapshot: vi.fn(async () => sessions),
    create: vi.fn(async () => {
      const next = session(agentKind, `${agentKind}-created`);
      sessions.push(next);
      return next;
    }),
    get: vi.fn(async (sessionId: string) => {
      const found = sessions.find((candidate) => candidate.sessionId === sessionId);
      if (found === undefined) {
        const error = new Error("missing");
        Object.defineProperty(error, "code", { value: "ProtocolViolation" });
        throw error;
      }
      return found;
    }),
    history: vi.fn(async (sessionId: string): Promise<SessionHistory> => ({
      session: await Promise.resolve(session(agentKind, sessionId)),
      messages: [{ agentKind, sessionId, ordinal: 0, role: "user", text: "hello" }],
    })),
    send: vi.fn(async () => undefined),
    abort: vi.fn(async () => undefined),
    commands: vi.fn(async (): Promise<readonly CommandDescriptor[]> => [
      {
        name: "/compact",
        agentKind,
        source: "native",
        channel: agentKind === "pi" ? "rpc" : "http",
        allowedWhileRunning: false,
        mutatesGlobalDefaults: false,
        tuiOnly: false,
      },
    ]),
  };
}

describe("managed session coordinator", () => {
  it("selects the first real session and keeps per-agent selections separate", async () => {
    const pi = adapter("pi", [session("pi", "pi-1"), session("pi", "pi-2")]);
    const openCode = adapter("opencode", [session("opencode", "oc-1")]);
    const coordinator = createSessionCoordinator((_, kind) => kind === "pi" ? pi : openCode);

    await expect(coordinator.snapshot(workspaceId)).resolves.toEqual([
      session("pi", "pi-1", true),
      session("pi", "pi-2", false),
      session("opencode", "oc-1", true),
    ]);
    await expect(coordinator.select(workspaceId, "pi", "pi-2")).resolves.toEqual(session("pi", "pi-2", true));
    await expect(coordinator.snapshot(workspaceId)).resolves.toEqual([
      session("pi", "pi-1", false),
      session("pi", "pi-2", true),
      session("opencode", "oc-1", true),
    ]);
  });

  it("deduplicates a client retry without replaying a native prompt", async () => {
    const pi = adapter("pi", [session("pi", "pi-1")]);
    const coordinator = createSessionCoordinator((_, kind) => kind === "pi" ? pi : undefined);
    const requestId = "13ebf428-5647-4a32-ae2e-55304b4e3e9f";

    await expect(Promise.all([
      coordinator.send(workspaceId, "pi", "pi-1", "hello", requestId),
      coordinator.send(workspaceId, "pi", "pi-1", "hello", requestId),
    ])).resolves.toEqual([
      { session: session("pi", "pi-1", true), clientRequestId: requestId, accepted: true },
      { session: session("pi", "pi-1", true), clientRequestId: requestId, accepted: true },
    ]);
    expect(pi.send).toHaveBeenCalledTimes(1);
  });

  it("only publishes schema-valid fixed agent events", () => {
    const coordinator = createSessionCoordinator(() => undefined);
    const listener = vi.fn();
    const unsubscribe = coordinator.subscribe(listener);
    const valid: AgentEventEnvelope = {
      eventId: "13ebf428-5647-4a32-ae2e-55304b4e3e9f",
      workspaceId,
      sequence: 0,
      timestamp: "2026-07-24T00:00:00.000Z",
      agentKind: "pi",
      payload: { protocol: "pi-rpc", type: "agent_start" },
    };

    coordinator.publish(valid);
    coordinator.publish({ ...valid, workspaceId: "not-a-workspace" } as AgentEventEnvelope);
    expect(listener).toHaveBeenCalledTimes(1);
    unsubscribe();
    coordinator.publish(valid);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it("does not offer a session adapter where no managed runtime exists", async () => {
    const coordinator = createSessionCoordinator(() => undefined);
    await expect(coordinator.create(workspaceId, "pi")).rejects.toMatchObject({ code: "RuntimeUnavailable" });
  });
});
