import { useEffect, useMemo, useState } from "react";
import type { AgentId, TerminalSessionInfo } from "../../shared/agents";

export function useTerminalSession() {
  const [sessions, setSessions] = useState<TerminalSessionInfo[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);

  useEffect(() => {
    return window.halo.sessions.onExit(({ sessionId }) => {
      setSessions((current) => current.filter((session) => session.id !== sessionId));
      setActiveSessionId((current) => (current === sessionId ? null : current));
    });
  }, []);

  const activeSession = useMemo(
    () => sessions.find((session) => session.id === activeSessionId) ?? null,
    [activeSessionId, sessions]
  );

  async function start(agentId: AgentId, cwd: string) {
    const session = await window.halo.sessions.start({ agentId, cwd });
    setSessions((current) => [...current, session]);
    setActiveSessionId(session.id);
  }

  async function stop(sessionId: string) {
    await window.halo.sessions.stop(sessionId);
    setSessions((current) => current.filter((session) => session.id !== sessionId));
    setActiveSessionId((current) => (current === sessionId ? null : current));
  }

  return {
    sessions,
    activeSession,
    activeSessionId,
    setActiveSessionId,
    start,
    stop
  };
}
