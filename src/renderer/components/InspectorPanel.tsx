import { Activity, Database, KeyRound } from "lucide-react";
import type { AgentInfo, TerminalSessionInfo } from "../../shared/agents";
import { McpPreviewPanel } from "./McpPreviewPanel";

interface InspectorPanelProps {
  agents: AgentInfo[];
  activeSession: TerminalSessionInfo | null;
}

export function InspectorPanel({ agents, activeSession }: InspectorPanelProps) {
  const readyCount = agents.filter((agent) => agent.status === "ready").length;

  return (
    <aside className="h-full w-80 shrink-0 border-l border-halo-line bg-halo-panel p-4">
      <section>
        <div className="flex items-center gap-2 text-sm font-semibold text-slate-100">
          <Activity size={16} />
          会话状态
        </div>
        <div className="mt-3 rounded border border-halo-line bg-halo-panelSoft p-3 text-sm text-slate-300">
          {activeSession ? (
            <>
              <div>{activeSession.title}</div>
              <div className="mt-1 break-all text-xs text-slate-500">{activeSession.cwd}</div>
            </>
          ) : (
            "暂无运行会话"
          )}
        </div>
      </section>

      <McpPreviewPanel />

      <section className="mt-6 grid gap-3">
        <div className="flex items-center gap-2 rounded border border-halo-line bg-halo-panelSoft p-3 text-sm text-slate-300">
          <Database size={16} />
          已检测 Agent：{readyCount}/{agents.length}
        </div>
        <div className="flex items-center gap-2 rounded border border-halo-line bg-halo-panelSoft p-3 text-sm text-slate-300">
          <KeyRound size={16} />
          凭据服务未启用
        </div>
      </section>
    </aside>
  );
}
