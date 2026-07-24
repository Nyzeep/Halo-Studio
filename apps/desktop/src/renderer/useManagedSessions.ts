import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  AgentKind,
  CommandDescriptor,
  SessionMessage,
  SessionSummary,
} from "@halo-studio/contracts";

import type { SessionSelection } from "./components/SessionWorkbench.js";
import { publicRequestMessage, unwrapEnvelope, type WorkbenchApi } from "./api.js";

export interface ManagedSessionsState {
  readonly sessions: readonly SessionSummary[];
  readonly selectedSession: SessionSelection | undefined;
  readonly messages: readonly SessionMessage[];
  readonly commands: readonly CommandDescriptor[];
  readonly draft: string;
  readonly commandFilter: string;
  readonly loading: boolean;
  readonly messagesLoading: boolean;
  readonly commandsLoading: boolean;
  readonly sending: boolean;
  readonly aborting: boolean;
  readonly error: string | undefined;
  canCreateSession(agentKind: AgentKind): boolean;
  selectSession(session: SessionSummary): Promise<void>;
  createSession(agentKind: AgentKind): Promise<void>;
  send(session: SessionSummary, message: string): Promise<void>;
  abort(session: SessionSummary): Promise<void>;
  setDraft(value: string): void;
  setCommandFilter(value: string): void;
}

function selectionOf(session: SessionSummary): SessionSelection {
  return { agentKind: session.agentKind, sessionId: session.sessionId };
}

function sameSelection(
  left: SessionSelection | undefined,
  right: SessionSelection | undefined,
): boolean {
  return left?.agentKind === right?.agentKind && left?.sessionId === right?.sessionId;
}

function findSelected(
  sessions: readonly SessionSummary[],
  selection: SessionSelection | undefined,
): SessionSummary | undefined {
  if (selection !== undefined) {
    const existing = sessions.find((session) => sameSelection(selectionOf(session), selection));
    if (existing !== undefined) return existing;
  }
  return sessions.find((session) => session.active) ?? sessions[0];
}

function replaceSession(
  sessions: readonly SessionSummary[],
  next: SessionSummary,
): readonly SessionSummary[] {
  return [...sessions.filter((session) => !sameSelection(selectionOf(session), selectionOf(next))), next];
}

function clientRequestId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") return crypto.randomUUID();
  const fragment = Math.random().toString(16).slice(2).padEnd(12, "0").slice(0, 12);
  return `00000000-0000-4000-8000-${fragment}`;
}

/**
 * Renderer-only view state for the bounded managed-session API. Native
 * sessions, credentials, and transport details remain exclusively in Main.
 */
export function useManagedSessions(
  api: WorkbenchApi | undefined,
  workspaceId: string | undefined,
  enabled: boolean,
  availableAgentKinds: readonly AgentKind[],
): ManagedSessionsState {
  const [sessions, setSessions] = useState<readonly SessionSummary[]>([]);
  const [selectedSession, setSelectedSession] = useState<SessionSelection>();
  const [messages, setMessages] = useState<readonly SessionMessage[]>([]);
  const [commands, setCommands] = useState<readonly CommandDescriptor[]>([]);
  const [draft, setDraft] = useState("");
  const [commandFilter, setCommandFilter] = useState("");
  const [loading, setLoading] = useState(false);
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [commandsLoading, setCommandsLoading] = useState(false);
  const [sending, setSending] = useState(false);
  const [aborting, setAborting] = useState(false);
  const [error, setError] = useState<string>();
  const requestVersion = useRef(0);
  const selectedRef = useRef<SessionSelection>();
  const availableKey = [...availableAgentKinds].sort().join("\u0000");
  const available = useMemo(() => new Set(availableAgentKinds), [availableKey]);

  const reset = useCallback((): void => {
    requestVersion.current += 1;
    selectedRef.current = undefined;
    setSessions([]);
    setSelectedSession(undefined);
    setMessages([]);
    setCommands([]);
    setDraft("");
    setCommandFilter("");
    setLoading(false);
    setMessagesLoading(false);
    setCommandsLoading(false);
    setSending(false);
    setAborting(false);
    setError(undefined);
  }, []);

  const loadHistory = useCallback(async (
    session: SessionSummary,
    version: number,
  ): Promise<void> => {
    if (api === undefined || workspaceId === undefined) return;
    setMessagesLoading(true);
    try {
      const history = unwrapEnvelope(await api.sessions.history({
        workspaceId,
        agentKind: session.agentKind,
        sessionId: session.sessionId,
      }));
      if (version !== requestVersion.current) return;
      setMessages(history.messages);
      setSessions((current) => replaceSession(current, history.session));
      setError(undefined);
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
    } finally {
      if (version === requestVersion.current) setMessagesLoading(false);
    }
  }, [api, workspaceId]);

  const loadCommands = useCallback(async (
    agentKind: AgentKind,
    version: number,
  ): Promise<void> => {
    if (api === undefined || workspaceId === undefined) return;
    setCommandsLoading(true);
    try {
      const next = unwrapEnvelope(await api.commands.list({ workspaceId, agentKind }));
      if (version !== requestVersion.current) return;
      setCommands(next);
      setError(undefined);
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
    } finally {
      if (version === requestVersion.current) setCommandsLoading(false);
    }
  }, [api, workspaceId]);

  const refresh = useCallback(async (): Promise<void> => {
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    if (!enabled || api === undefined || workspaceId === undefined || available.size === 0) {
      reset();
      return;
    }
    setLoading(true);
    try {
      const next = unwrapEnvelope(await api.sessions.snapshot({ workspaceId }));
      if (version !== requestVersion.current) return;
      const selected = findSelected(next, selectedRef.current);
      const selection = selected === undefined ? undefined : selectionOf(selected);
      selectedRef.current = selection;
      setSessions(next);
      setSelectedSession(selection);
      setError(undefined);
      if (selected === undefined) {
        setMessages([]);
        setCommands([]);
      } else {
        void loadHistory(selected, version);
        void loadCommands(selected.agentKind, version);
      }
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
    } finally {
      if (version === requestVersion.current) setLoading(false);
    }
  }, [api, available, enabled, loadCommands, loadHistory, reset, workspaceId]);

  useEffect(() => {
    void refresh();
    return () => { requestVersion.current += 1; };
  }, [refresh]);

  useEffect(() => {
    if (!enabled || api === undefined || workspaceId === undefined || available.size === 0) return undefined;
    return api.sessions.subscribe((event) => {
      if (event.workspaceId !== workspaceId) return;
      void refresh();
    });
  }, [api, available, enabled, refresh, workspaceId]);

  const selectSession = useCallback(async (session: SessionSummary): Promise<void> => {
    if (api === undefined || workspaceId === undefined || !available.has(session.agentKind)) return;
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    setMessagesLoading(true);
    setCommandsLoading(true);
    try {
      const selected = unwrapEnvelope(await api.sessions.select({
        workspaceId,
        agentKind: session.agentKind,
        sessionId: session.sessionId,
      }));
      if (version !== requestVersion.current) return;
      const selection = selectionOf(selected);
      selectedRef.current = selection;
      setSelectedSession(selection);
      setSessions((current) => replaceSession(current, selected));
      setError(undefined);
      await Promise.all([loadHistory(selected, version), loadCommands(selected.agentKind, version)]);
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
      if (version === requestVersion.current) {
        setMessagesLoading(false);
        setCommandsLoading(false);
      }
    }
  }, [api, available, loadCommands, loadHistory, workspaceId]);

  const createSession = useCallback(async (agentKind: AgentKind): Promise<void> => {
    if (api === undefined || workspaceId === undefined || !available.has(agentKind)) return;
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    setLoading(true);
    try {
      const created = unwrapEnvelope(await api.sessions.create({ workspaceId, agentKind }));
      if (version !== requestVersion.current) return;
      const selection = selectionOf(created);
      selectedRef.current = selection;
      setSelectedSession(selection);
      setSessions((current) => replaceSession(current, created));
      setMessages([]);
      setCommands([]);
      setDraft("");
      setCommandFilter("");
      setError(undefined);
      await Promise.all([loadHistory(created, version), loadCommands(created.agentKind, version)]);
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
    } finally {
      if (version === requestVersion.current) setLoading(false);
    }
  }, [api, available, loadCommands, loadHistory, workspaceId]);

  const send = useCallback(async (session: SessionSummary, message: string): Promise<void> => {
    if (api === undefined || workspaceId === undefined || !available.has(session.agentKind)) return;
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    setSending(true);
    try {
      const result = unwrapEnvelope(await api.sessions.send({
        workspaceId,
        agentKind: session.agentKind,
        sessionId: session.sessionId,
        message,
        clientRequestId: clientRequestId(),
      }));
      if (version !== requestVersion.current) return;
      const selection = selectionOf(result.session);
      selectedRef.current = selection;
      setSelectedSession(selection);
      setSessions((current) => replaceSession(current, result.session));
      setDraft("");
      setError(undefined);
      await loadHistory(result.session, version);
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
    } finally {
      if (version === requestVersion.current) setSending(false);
    }
  }, [api, available, loadHistory, workspaceId]);

  const abort = useCallback(async (session: SessionSummary): Promise<void> => {
    if (api === undefined || workspaceId === undefined || !available.has(session.agentKind)) return;
    const version = requestVersion.current + 1;
    requestVersion.current = version;
    setAborting(true);
    try {
      const stopped = unwrapEnvelope(await api.sessions.abort({
        workspaceId,
        agentKind: session.agentKind,
        sessionId: session.sessionId,
      }));
      if (version !== requestVersion.current) return;
      setSessions((current) => replaceSession(current, stopped));
      setError(undefined);
      await loadHistory(stopped, version);
    } catch (requestError) {
      if (version === requestVersion.current) setError(publicRequestMessage(requestError));
    } finally {
      if (version === requestVersion.current) setAborting(false);
    }
  }, [api, available, loadHistory, workspaceId]);

  return {
    sessions,
    selectedSession,
    messages,
    commands,
    draft,
    commandFilter,
    loading,
    messagesLoading,
    commandsLoading,
    sending,
    aborting,
    error,
    canCreateSession: (agentKind) => available.has(agentKind),
    selectSession,
    createSession,
    send,
    abort,
    setDraft,
    setCommandFilter,
  };
}
