import { Bot, FolderOpen, Play, Settings2 } from "lucide-react";
import type { AgentId, AgentInfo } from "../../shared/agents";

interface AgentRailProps {
  agents: AgentInfo[];
  loading: boolean;
  onLaunch(agentId: AgentId): void;
}

export function AgentRail({ agents, loading, onLaunch }: AgentRailProps) {
  return (
    <aside className="flex h-full w-72 shrink-0 flex-col border-r border-halo-line bg-halo-panel">
      <div className="flex h-16 items-center gap-3 border-b border-halo-line px-5">
        <div className="flex h-9 w-9 items-center justify-center rounded bg-halo-cyan/15 text-halo-cyan">
          <Bot size={20} />
        </div>
        <div>
          <div className="text-sm font-semibold">Halo Studio</div>
          <div className="text-xs text-slate-400">多 Agent 工作台</div>
        </div>
      </div>

      <button className="mx-4 mt-4 flex items-center gap-2 rounded border border-halo-line bg-halo-panelSoft px-3 py-2 text-left text-sm text-slate-200">
        <FolderOpen size={16} />
        D:\Halo Studio
      </button>

      <div className="mt-5 px-4 text-xs font-medium uppercase text-slate-500">Agents</div>
      <div className="mt-2 flex-1 space-y-2 overflow-auto px-3">
        {loading ? (
          <div className="rounded border border-halo-line bg-halo-panelSoft px-3 py-3 text-sm text-slate-400">
            检测中...
          </div>
        ) : (
          agents.map((agent) => (
            <div key={agent.id} className="rounded border border-halo-line bg-halo-panelSoft p-3">
              <div className="flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-medium text-slate-100">{agent.name}</div>
                  <div className="mt-1 truncate text-xs text-slate-500">{agent.version ?? agent.command}</div>
                </div>
                <span className={agent.status === "ready" ? "text-halo-green" : "text-halo-amber"}>
                  {agent.status === "ready" ? "Ready" : "Missing"}
                </span>
              </div>
              <button
                className="mt-3 flex w-full items-center justify-center gap-2 rounded bg-halo-cyan px-3 py-2 text-sm font-medium text-slate-950 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
                disabled={agent.status !== "ready"}
                onClick={() => onLaunch(agent.id)}
              >
                <Play size={15} />
                启动
              </button>
            </div>
          ))
        )}
      </div>

      <button className="m-4 flex items-center gap-2 rounded border border-halo-line px-3 py-2 text-sm text-slate-300">
        <Settings2 size={16} />
        配置中心
      </button>
    </aside>
  );
}
