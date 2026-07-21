import { AgentRail } from "./components/AgentRail";
import { InspectorPanel } from "./components/InspectorPanel";
import { SessionTabs } from "./components/SessionTabs";
import { StatusBar } from "./components/StatusBar";
import { TerminalPane } from "./components/TerminalPane";
import { UtilityStrip } from "./components/UtilityStrip";
import { useAgents } from "./hooks/useAgents";
import { useTerminalSession } from "./hooks/useTerminalSession";

const defaultWorkspace = "D:\\Halo Studio";

export function App() {
  const { agents, loading } = useAgents();
  const { sessions, activeSession, activeSessionId, setActiveSessionId, start, stop } = useTerminalSession();

  return (
    <div className="flex h-full min-h-0 bg-halo-bg text-slate-100">
      <AgentRail agents={agents} loading={loading} onLaunch={(agentId) => void start(agentId, defaultWorkspace)} />
      <main className="flex min-w-0 flex-1 flex-col">
        <SessionTabs
          sessions={sessions}
          activeSessionId={activeSessionId}
          onSelect={setActiveSessionId}
          onClose={(sessionId) => void stop(sessionId)}
        />
        <UtilityStrip />
        <div className="min-h-0 flex-1">
          <TerminalPane session={activeSession} />
        </div>
        <StatusBar activeSession={activeSession} />
      </main>
      <InspectorPanel agents={agents} activeSession={activeSession} />
    </div>
  );
}
