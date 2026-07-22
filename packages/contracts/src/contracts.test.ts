import { describe, expect, it } from "vitest";

import {
  agentCapabilitiesSchema,
  agentEventEnvelopeSchema,
  agentKindSchema,
  appErrorSchema,
  capabilitySchema,
  commandDescriptorSchema,
  ipcContracts,
  runtimeBindingSchema,
  workspaceIdSchema,
  type DataOf,
  type InputOf,
  type IpcChannel,
} from "./index.js";

const workspaceId = "a".repeat(64);
const selectionId = "13ebf428-5647-4a32-ae2e-55304b4e3e9f";
const fingerprint = "b".repeat(64);

const supportedCapability = {
  supported: true,
  channel: "rpc",
  restartRequired: false,
} as const;

const capabilities = {
  sessions: supportedCapability,
  streamingMessages: supportedCapability,
  toolEvents: supportedCapability,
  permissions: supportedCapability,
  diff: supportedCapability,
  commands: supportedCapability,
  mcp: supportedCapability,
  skills: supportedCapability,
  prompts: supportedCapability,
  extensions: supportedCapability,
  packages: supportedCapability,
  models: supportedCapability,
  usage: supportedCapability,
};

const workspace = {
  id: workspaceId,
  rootPath: "D:\\Workspace",
  realPath: "D:\\Workspace",
  trustState: "untrusted",
} as const;

const trustedWorkspace = {
  ...workspace,
  trustState: "trusted",
} as const;

const runtimeBinding = {
  agentKind: "pi",
  source: "system",
  executable: "C:\\Tools\\pi.exe",
  version: "1.2.3",
  health: "healthy",
  capabilities,
} as const;

const stoppedRuntimeBinding = {
  ...runtimeBinding,
  health: "stopped",
} as const;

const configPreview = {
  previewId: "preview-1",
  targetId: "pi:user-settings",
  fingerprint,
  unifiedDiff: "--- a/settings.json\n+++ b/settings.json",
  restartRequired: ["pi"],
} satisfies DataOf<"config.preview">;

const configCommitResult = {
  backupId: "backup-1",
  targetId: "pi:user-settings",
  fingerprint,
} as const;

const configRollbackResult = {
  backupId: "backup-1",
  targetId: "pi:user-settings",
  fingerprint: "c".repeat(64),
} as const;

const storageHealth = {
  mode: "read-write",
  schemaVersion: 1,
  diagnostics: [],
} satisfies DataOf<"storage.health">;

function ipcFixture<TChannel extends IpcChannel>(
  channel: TChannel,
  request: InputOf<TChannel>,
  data: DataOf<TChannel>,
) {
  return { channel, request, data };
}

const validIpcFixtures = [
  ipcFixture(
    "workspace.pick",
    {},
    {
      selectionId,
      displayPath: "D:\\Workspace",
    },
  ),
  ipcFixture("workspace.open", { selectionId }, workspace),
  ipcFixture("workspace.snapshot", {}, [workspace]),
  ipcFixture(
    "workspace.trust",
    { workspaceId, trustState: "trusted" },
    trustedWorkspace,
  ),
  ipcFixture("runtime.probe", { workspaceId }, [runtimeBinding]),
  ipcFixture(
    "runtime.start",
    { workspaceId, agentKind: "pi" },
    runtimeBinding,
  ),
  ipcFixture(
    "runtime.stop",
    { workspaceId, agentKind: "pi" },
    stoppedRuntimeBinding,
  ),
  ipcFixture("runtime.snapshot", {}, [runtimeBinding]),
  ipcFixture(
    "config.preview",
    {
      targetId: "pi:user-settings",
      operations: [
        {
          op: "set",
          path: ["provider", "model"],
          value: "gpt-5",
        },
        {
          op: "remove",
          path: ["obsolete", 0],
        },
      ],
    },
    configPreview,
  ),
  ipcFixture(
    "config.commit",
    { previewId: configPreview.previewId },
    configCommitResult,
  ),
  ipcFixture(
    "config.rollback",
    { backupId: configCommitResult.backupId },
    configRollbackResult,
  ),
  ipcFixture("storage.health", {}, storageHealth),
];

describe("Agent and capability contracts", () => {
  it("accepts only Pi and OpenCode agent kinds", () => {
    expect(agentKindSchema.safeParse("pi").success).toBe(true);
    expect(agentKindSchema.safeParse("opencode").success).toBe(true);
    expect(agentKindSchema.safeParse("codex").success).toBe(false);
    expect(agentKindSchema.safeParse("claude-code").success).toBe(false);
    expect(agentKindSchema.safeParse("another-agent").success).toBe(false);
  });

  it("requires structured capability metadata", () => {
    expect(capabilitySchema.safeParse(true).success).toBe(false);
    expect(
      capabilitySchema.safeParse({
        supported: true,
        channel: "websocket",
        restartRequired: false,
      }).success,
    ).toBe(false);
    expect(
      capabilitySchema.safeParse({ supported: true, channel: "rpc" }).success,
    ).toBe(false);
  });

  it("keeps support state consistent with the capability channel", () => {
    expect(
      capabilitySchema.safeParse({
        supported: false,
        channel: "rpc",
        restartRequired: false,
      }).success,
    ).toBe(false);
    expect(
      capabilitySchema.safeParse({
        supported: true,
        channel: "unavailable",
        restartRequired: false,
      }).success,
    ).toBe(false);
    expect(
      capabilitySchema.safeParse({
        supported: false,
        channel: "unavailable",
        restartRequired: false,
      }).success,
    ).toBe(true);
  });

  it("requires exactly all thirteen capability keys", () => {
    const result = agentCapabilitiesSchema.parse(capabilities);
    expect(Object.keys(result).sort()).toEqual(Object.keys(capabilities).sort());
    expect(Object.keys(result)).toHaveLength(13);
    expect(
      agentCapabilitiesSchema.safeParse({ ...capabilities, usage: undefined })
        .success,
    ).toBe(false);
    expect(
      agentCapabilitiesSchema.safeParse({ ...capabilities, terminal: capabilities.sessions })
        .success,
    ).toBe(false);
  });

  it("validates a complete runtime binding and its finite source and health", () => {
    const binding = {
      agentKind: "pi",
      source: "system",
      executable: "C:\\Tools\\pi.exe",
      version: "1.2.3",
      health: "healthy",
      capabilities,
    };

    expect(runtimeBindingSchema.safeParse(binding).success).toBe(true);
    expect(
      runtimeBindingSchema.safeParse({ ...binding, source: "downloaded" }).success,
    ).toBe(false);
    expect(
      runtimeBindingSchema.safeParse({ ...binding, health: "running" }).success,
    ).toBe(false);
    expect(
      runtimeBindingSchema.safeParse({ ...binding, capabilities: true }).success,
    ).toBe(false);
  });

  it("uses a lowercase SHA-256 workspace identifier", () => {
    expect(workspaceIdSchema.safeParse(workspaceId).success).toBe(true);
    expect(workspaceIdSchema.safeParse("A".repeat(64)).success).toBe(false);
    expect(workspaceIdSchema.safeParse("a".repeat(63)).success).toBe(false);
    expect(workspaceIdSchema.safeParse(crypto.randomUUID()).success).toBe(false);
  });
});

describe("public errors", () => {
  it("accepts JSON-safe errors without exposing stacks", () => {
    const error = {
      code: "WorkspaceUntrusted",
      message: "Workspace is not trusted",
      retryable: false,
      action: "Review workspace trust",
      details: { workspaceId, attempts: 1, causes: ["policy"] },
    };

    expect(appErrorSchema.parse(error)).toEqual(error);
    expect(appErrorSchema.safeParse({ ...error, code: "Unknown" }).success).toBe(
      false,
    );
    expect(appErrorSchema.safeParse({ ...error, stack: "secret" }).success).toBe(
      false,
    );
    expect(
      appErrorSchema.safeParse({ ...error, details: { secret: undefined } })
        .success,
    ).toBe(false);
  });
});

describe("agent event envelopes", () => {
  const envelope = {
    eventId: "13ebf428-5647-4a32-ae2e-55304b4e3e9f",
    workspaceId,
    sessionId: "native-session-1",
    sequence: 0,
    timestamp: "2026-07-22T00:00:00.000Z",
  };

  it("validates Pi RPC and OpenCode SSE payloads", () => {
    expect(
      agentEventEnvelopeSchema.safeParse({
        ...envelope,
        agentKind: "pi",
        payload: { protocol: "pi-rpc", type: "agent_start", data: { id: 1 } },
      }).success,
    ).toBe(true);
    expect(
      agentEventEnvelopeSchema.safeParse({
        ...envelope,
        agentKind: "opencode",
        payload: {
          protocol: "opencode-sse",
          type: "message.part.updated",
          unknown: true,
        },
      }).success,
    ).toBe(true);
  });

  it("keeps agent kind correlated with its native protocol", () => {
    expect(
      agentEventEnvelopeSchema.safeParse({
        ...envelope,
        agentKind: "pi",
        payload: { protocol: "opencode-sse", type: "event" },
      }).success,
    ).toBe(false);
    expect(
      agentEventEnvelopeSchema.safeParse({
        ...envelope,
        agentKind: "opencode",
        payload: { protocol: "pi-rpc", type: "event" },
      }).success,
    ).toBe(false);
  });

  it("rejects malformed common envelope fields", () => {
    const valid = {
      ...envelope,
      agentKind: "pi",
      payload: { protocol: "pi-rpc", type: "event" },
    };

    expect(
      agentEventEnvelopeSchema.safeParse({ ...valid, sequence: -1 }).success,
    ).toBe(false);
    expect(
      agentEventEnvelopeSchema.safeParse({ ...valid, timestamp: "today" }).success,
    ).toBe(false);
    expect(
      agentEventEnvelopeSchema.safeParse({ ...valid, extra: true }).success,
    ).toBe(false);
  });
});

describe("IPC contracts", () => {
  const expectedChannels = [
    "workspace.pick",
    "workspace.open",
    "workspace.snapshot",
    "workspace.trust",
    "runtime.probe",
    "runtime.start",
    "runtime.stop",
    "runtime.snapshot",
    "config.preview",
    "config.commit",
    "config.rollback",
    "storage.health",
  ];

  it("exports exactly the first-phase business channels", () => {
    expect(Object.keys(ipcContracts).sort()).toEqual(expectedChannels.sort());
    for (const forbidden of [
      "shell.exec",
      "fs.read",
      "fs.write",
      "sql.query",
      "terminal.write",
    ]) {
      expect(forbidden in ipcContracts).toBe(false);
    }
  });

  it("accepts a typed request, data payload, and success response for every channel", () => {
    expect(validIpcFixtures.map(({ channel }) => channel).sort()).toEqual(
      Object.keys(ipcContracts).sort(),
    );

    for (const fixture of validIpcFixtures) {
      const contract = ipcContracts[fixture.channel];
      expect(contract.request.parse(fixture.request)).toEqual(fixture.request);
      expect(contract.data.parse(fixture.data)).toEqual(fixture.data);
      expect(
        contract.response.parse({ ok: true, data: fixture.data }),
      ).toEqual({ ok: true, data: fixture.data });
    }
  });

  it("rejects invalid requests and responses for every channel", () => {
    for (const contract of Object.values(ipcContracts)) {
      expect(contract.request.safeParse({ forbidden: true }).success).toBe(false);
      expect(
        contract.response.safeParse({ ok: true, data: Symbol("invalid") }).success,
      ).toBe(false);
      expect(
        contract.response.safeParse({
          ok: false,
          error: {
            code: "NotAnAppError",
            message: "invalid",
            retryable: false,
          },
        }).success,
      ).toBe(false);
    }
  });

  it("accepts only selection handles when opening a workspace", () => {
    const open = ipcContracts["workspace.open"].request;
    expect(
      open.safeParse({ selectionId: "13ebf428-5647-4a32-ae2e-55304b4e3e9f" })
        .success,
    ).toBe(true);
    expect(open.safeParse({ rootPath: "D:\\arbitrary" }).success).toBe(false);
  });

  it("probes the runtime set without accepting a renderer-selected agent", () => {
    const probe = ipcContracts["runtime.probe"].request;

    expect(probe.safeParse({}).success).toBe(true);
    expect(probe.safeParse({ workspaceId }).success).toBe(true);
    expect(probe.safeParse({ agentKind: "pi" }).success).toBe(false);
  });

  it("limits config writes to target identifiers and structured operations", () => {
    const preview = ipcContracts["config.preview"].request;
    expect(
      preview.safeParse({
        targetId: "pi:user-settings",
        operations: [
          { op: "set", path: ["provider", "model"], value: "gpt-5" },
          { op: "remove", path: ["obsolete", 0] },
        ],
      }).success,
    ).toBe(true);
    expect(
      preview.safeParse({
        targetPath: "C:\\Users\\name\\settings.json",
        operations: [],
      }).success,
    ).toBe(false);
    expect(
      preview.safeParse({
        targetId: "pi:user-settings",
        operations: [{ op: "set", path: ["key"], value: undefined }],
      }).success,
    ).toBe(false);
  });

  it("validates success and serializable error envelopes at runtime", () => {
    const storage = ipcContracts["storage.health"];
    expect(
      storage.response.safeParse({
        ok: true,
        data: { mode: "read-only-recovery", schemaVersion: 0, diagnostics: [] },
      }).success,
    ).toBe(true);
    expect(
      storage.response.safeParse({
        ok: false,
        error: {
          code: "MigrationFailed",
          message: "Migration failed",
          retryable: true,
        },
      }).success,
    ).toBe(true);
    expect(
      storage.response.safeParse({
        ok: false,
        error: {
          code: "MigrationFailed",
          message: "Migration failed",
          retryable: true,
          stack: "must not cross IPC",
        },
      }).success,
    ).toBe(false);
  });
});

describe("command descriptors", () => {
  it("validates native command metadata without extra fields", () => {
    const descriptor = {
      name: "/compact",
      argumentHint: "[instructions]",
      agentKind: "pi",
      source: "native",
      channel: "rpc",
      allowedWhileRunning: false,
      mutatesGlobalDefaults: false,
      tuiOnly: false,
    };

    expect(commandDescriptorSchema.parse(descriptor)).toEqual(descriptor);
    expect(
      commandDescriptorSchema.safeParse({ ...descriptor, source: "plugin" }).success,
    ).toBe(false);
    expect(
      commandDescriptorSchema.safeParse({ ...descriptor, description: "extra" })
        .success,
    ).toBe(false);
  });
});
