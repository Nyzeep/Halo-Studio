import { Activity, Database, KeyRound, Shield } from "lucide-react";
import type { AgentInfo, TerminalSessionInfo } from "../../shared/agents";
import { McpPreviewPanel } from "./McpPreviewPanel";

interface InspectorPanelProps {
  agents: AgentInfo[];
  activeSession: TerminalSessionInfo | null;
}

export function InspectorPanel({ agents, activeSession }: InspectorPanelProps) {
  const readyCount = agents.filter((agent) => agent.status === "ready").length;

  return (
    <aside className="h-full w-80 shrink-0 border-l border-white/5 bg-[#0a0814]/70 p-4 space-y-5 overflow-y-auto">
      {/* Session state segment */}
      <section className="space-y-3">
        <div className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-slate-400">
          <Activity size={14} className="text-purple-400" />
          运行会话状态
        </div>
        <div className="rounded-xl border border-white/5 bg-white/5 p-3.5 space-y-2">
          {activeSession ? (
            <>
              <div className="text-xs font-semibold text-purple-300">{activeSession.title}</div>
              <div className="break-all font-mono text-[10px] text-slate-500 leading-normal">
                {activeSession.cwd}
              </div>
            </>
          ) : (
            <div className="text-xs text-slate-500 leading-normal">
              暂无处于活动状态的 PTY 终端会话
            </div>
          )}
        </div>
      </section>

      {/* Embedded MCP configuration segment */}
      <McpPreviewPanel />

      {/* Additional telemetry indicators */}
      <section className="grid gap-2 pt-3 border-t border-white/5">
        <div className="flex items-center gap-2.5 rounded-xl border border-white/5 bg-white/5 p-3.5 text-xs text-slate-300">
          <Database size={14} className="text-purple-400 shrink-0" />
          <span>本地就绪：{readyCount}/{agents.length} Agent</span>
        </div>
        <div className="flex items-center gap-2.5 rounded-xl border border-white/5 bg-white/5 p-3.5 text-xs text-slate-400">
          <KeyRound size={14} className="text-slate-500 shrink-0" />
          <span>安全凭据中继未启用</span>
        </div>
      </section>
    </aside>
  );
}
