import {
  sessionEventSchema,
  type AgentEventEnvelope,
  type AgentKind,
  type CommandDescriptor,
  type SessionHistory,
  type SessionSendResult,
  type SessionSummary,
} from "@halo-studio/contracts";

export interface ManagedSessionAdapter {
  readonly agentKind: AgentKind;
  snapshot(): Promise<readonly SessionSummary[]>;
  create(): Promise<SessionSummary>;
  get(sessionId: string): Promise<SessionSummary>;
  history(sessionId: string): Promise<SessionHistory>;
  send(sessionId: string, message: string): Promise<void>;
  abort(sessionId: string): Promise<void>;
  commands(): Promise<readonly CommandDescriptor[]>;
}

export type SessionAdapterResolver = (
  workspaceId: string,
  agentKind: AgentKind,
) => ManagedSessionAdapter | undefined;

export interface SessionCoordinator {
  snapshot(workspaceId: string): Promise<readonly SessionSummary[]>;
  create(workspaceId: string, agentKind: AgentKind): Promise<SessionSummary>;
  select(workspaceId: string, agentKind: AgentKind, sessionId: string): Promise<SessionSummary>;
  history(workspaceId: string, agentKind: AgentKind, sessionId: string): Promise<SessionHistory>;
  send(
    workspaceId: string,
    agentKind: AgentKind,
    sessionId: string,
    message: string,
    clientRequestId: string,
  ): Promise<SessionSendResult>;
  abort(workspaceId: string, agentKind: AgentKind, sessionId: string): Promise<SessionSummary>;
  commands(workspaceId: string, agentKind: AgentKind): Promise<readonly CommandDescriptor[]>;
  publish(event: AgentEventEnvelope): void;
  subscribe(listener: (event: AgentEventEnvelope) => void): () => void;
  discardWorkspace(workspaceId: string): void;
}

function selectionKey(workspaceId: string, agentKind: AgentKind): string {
  return `${workspaceId}\u0000${agentKind}`;
}

function requestKey(
  workspaceId: string,
  agentKind: AgentKind,
  sessionId: string,
  clientRequestId: string,
): string {
  return `${workspaceId}\u0000${agentKind}\u0000${sessionId}\u0000${clientRequestId}`;
}

function requireAdapter(
  resolver: SessionAdapterResolver,
  workspaceId: string,
  agentKind: AgentKind,
): ManagedSessionAdapter {
  const adapter = resolver(workspaceId, agentKind);
  if (adapter === undefined) {
    const error = new Error("Managed session runtime is unavailable.");
    Object.defineProperty(error, "code", { value: "RuntimeUnavailable" });
    throw error;
  }
  return adapter;
}

function withActive(summary: SessionSummary, active: boolean): SessionSummary {
  return { ...summary, active };
}

function normalizeCommands(
  agentKind: AgentKind,
  commands: readonly CommandDescriptor[],
): readonly CommandDescriptor[] {
  const unique = new Map<string, CommandDescriptor>();
  for (const command of commands) {
    if (command.agentKind !== agentKind) continue;
    if (!command.name.startsWith("/")) continue;
    unique.set(command.name, { ...command });
  }
  return [...unique.values()].sort((left, right) => left.name.localeCompare(right.name));
}

/**
 * Owns only desktop selection, duplicate-request suppression, and fixed event
 * fan-out. Native session protocol and credentials remain behind adapters.
 */
export function createSessionCoordinator(resolveAdapter: SessionAdapterResolver): SessionCoordinator {
  const selected = new Map<string, string>();
  const completedRequests = new Map<string, Promise<SessionSendResult>>();
  const listeners = new Set<(event: AgentEventEnvelope) => void>();

  const markSelection = (workspaceId: string, agentKind: AgentKind, sessionId: string): void => {
    selected.set(selectionKey(workspaceId, agentKind), sessionId);
  };
  const markedSummary = (
    workspaceId: string,
    agentKind: AgentKind,
    summary: SessionSummary,
  ): SessionSummary => withActive(
    summary,
    selected.get(selectionKey(workspaceId, agentKind)) === summary.sessionId,
  );

  return {
    async snapshot(workspaceId) {
      const results: SessionSummary[] = [];
      for (const agentKind of ["pi", "opencode"] as const) {
        const adapter = resolveAdapter(workspaceId, agentKind);
        if (adapter === undefined) continue;
        const sessions = await adapter.snapshot();
        const key = selectionKey(workspaceId, agentKind);
        const current = selected.get(key);
        const exists = current !== undefined && sessions.some((session) => session.sessionId === current);
        if (!exists && sessions.length > 0) markSelection(workspaceId, agentKind, sessions[0]!.sessionId);
        if (!exists && sessions.length === 0) selected.delete(key);
        results.push(...sessions.map((session) => markedSummary(workspaceId, agentKind, session)));
      }
      return results;
    },
    async create(workspaceId, agentKind) {
      const summary = await requireAdapter(resolveAdapter, workspaceId, agentKind).create();
      markSelection(workspaceId, agentKind, summary.sessionId);
      return withActive(summary, true);
    },
    async select(workspaceId, agentKind, sessionId) {
      const summary = await requireAdapter(resolveAdapter, workspaceId, agentKind).get(sessionId);
      markSelection(workspaceId, agentKind, sessionId);
      return withActive(summary, true);
    },
    async history(workspaceId, agentKind, sessionId) {
      const adapter = requireAdapter(resolveAdapter, workspaceId, agentKind);
      const history = await adapter.history(sessionId);
      markSelection(workspaceId, agentKind, history.session.sessionId);
      return {
        ...history,
        session: withActive(history.session, true),
        messages: history.messages.map((message) => ({ ...message })),
      };
    },
    async send(workspaceId, agentKind, sessionId, message, clientRequestId) {
      const key = requestKey(workspaceId, agentKind, sessionId, clientRequestId);
      const existing = completedRequests.get(key);
      if (existing !== undefined) return existing;
      const result = (async (): Promise<SessionSendResult> => {
        const adapter = requireAdapter(resolveAdapter, workspaceId, agentKind);
        const session = await adapter.get(sessionId);
        markSelection(workspaceId, agentKind, session.sessionId);
        await adapter.send(session.sessionId, message);
        return {
          session: withActive(session, true),
          clientRequestId,
          accepted: true,
        };
      })();
      completedRequests.set(key, result);
      try {
        return await result;
      } catch (error) {
        completedRequests.delete(key);
        throw error;
      }
    },
    async abort(workspaceId, agentKind, sessionId) {
      const adapter = requireAdapter(resolveAdapter, workspaceId, agentKind);
      const summary = await adapter.get(sessionId);
      markSelection(workspaceId, agentKind, summary.sessionId);
      await adapter.abort(summary.sessionId);
      return withActive(summary, true);
    },
    async commands(workspaceId, agentKind) {
      const commands = await requireAdapter(resolveAdapter, workspaceId, agentKind).commands();
      return normalizeCommands(agentKind, commands);
    },
    publish(event) {
      const parsed = sessionEventSchema.safeParse(event);
      if (!parsed.success) return;
      for (const listener of listeners) {
        try { listener(parsed.data); } catch { /* Renderer listeners are isolated. */ }
      }
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    discardWorkspace(workspaceId) {
      for (const key of [...selected.keys()]) {
        if (key.startsWith(`${workspaceId}\u0000`)) selected.delete(key);
      }
      for (const key of [...completedRequests.keys()]) {
        if (key.startsWith(`${workspaceId}\u0000`)) completedRequests.delete(key);
      }
    },
  };
}
