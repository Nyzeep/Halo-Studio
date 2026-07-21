import { Bot, FolderOpen, Play, ShieldCheck } from "lucide-react";
import type { AgentId, AgentInfo } from "../../shared/agents";

interface AgentRailProps {
  agents: AgentInfo[];
  loading: boolean;
  onLaunch(agentId: AgentId): void;
}

export function AgentRail({ agents, loading, onLaunch }: AgentRailProps) {
  return (
    <aside className="flex h-full w-72 shrink-0 flex-col glass-sidebar">
      {/* Top Brand Logo */}
      <div className="flex h-16 items-center gap-3 border-b border-white/5 px-5">
        <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-purple-500/10 text-purple-400 cosmic-glow-border">
          <Bot size={18} />
        </div>
        <div>
          <div className="text-sm font-bold bg-gradient-to-r from-violet-200 to-purple-300 bg-clip-text text-transparent">
            Halo Studio
          </div>
          <div className="text-[10px] text-slate-500 font-medium">多 Agent 本地编排面板</div>
        </div>
      </div>

      {/* Directory selection widget */}
      <button className="mx-4 mt-4 flex items-center gap-2.5 rounded-xl border border-white/5 bg-white/5 px-3.5 py-2.5 text-left text-xs text-slate-300 hover:bg-white/10 transition-all">
        <FolderOpen size={14} className="text-purple-400 shrink-0" />
        <span className="truncate font-mono">D:\Halo Studio</span>
      </button>

      {/* Agents Subheading */}
      <div className="mt-6 px-5 text-[10px] font-bold uppercase tracking-wider text-slate-500">
        已注册适配器 (Local Agents)
      </div>

      {/* Dynamic Agent list */}
      <div className="mt-3 flex-1 space-y-3 overflow-y-auto px-4">
        {loading ? (
          <div className="rounded-xl border border-white/5 bg-white/5 px-4 py-3 text-xs text-slate-500 animate-pulse">
            检测本地进程中...
          </div>
        ) : (
          agents.map((agent) => {
            const isReady = agent.status === "ready";
            return (
              <div
                key={agent.id}
                className="rounded-xl border border-white/5 bg-white/5 p-4 space-y-3 hover:border-purple-500/25 transition-all duration-300"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="truncate text-xs font-semibold text-slate-200">{agent.name}</div>
                    <div className="mt-1 truncate text-[10px] font-mono text-slate-500">
                      {agent.version ?? agent.command}
                    </div>
                  </div>
                  <span
                    className={`rounded-full px-2 py-0.5 text-[9px] font-bold ${
                      isReady ? "bg-emerald-500/10 text-emerald-400" : "bg-amber-500/10 text-amber-400"
                    }`}
                  >
                    {isReady ? "Ready" : "Missing"}
                  </span>
                </div>
                <button
                  className="btn-cosmic-gradient flex w-full items-center justify-center gap-2 rounded-xl py-2 text-xs font-semibold text-white disabled:pointer-events-none disabled:opacity-30"
                  onClick={() => onLaunch(agent.id)}
                >
                  <Play size={12} fill="currentColor" />
                  {isReady ? "启动子进程会话" : "启动模拟会话"}
                </button>
              </div>
            );
          })
        )}
      </div>

      {/* Quick Security Badge */}
      <div className="m-4 rounded-xl border border-purple-500/15 bg-purple-500/5 p-3 flex items-center gap-2.5 text-[10px] text-purple-300">
        <ShieldCheck size={14} className="text-purple-400 shrink-0" />
        <span>沙箱保护已启用，安全拦截越权写操作</span>
      </div>
    </aside>
  );
}
