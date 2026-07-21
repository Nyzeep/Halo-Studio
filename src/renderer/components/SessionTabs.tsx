import { X, Terminal } from "lucide-react";
import type { TerminalSessionInfo } from "../../shared/agents";

interface SessionTabsProps {
  sessions: TerminalSessionInfo[];
  activeSessionId: string | null;
  onSelect(sessionId: string): void;
  onClose(sessionId: string): void;
}

export function SessionTabs({ sessions, activeSessionId, onSelect, onClose }: SessionTabsProps) {
  return (
    <div className="flex h-12 items-center gap-2 border-b border-white/5 bg-[#0a0814]/80 px-4">
      {sessions.length === 0 ? (
        <div className="flex items-center gap-2 text-xs font-medium text-slate-500">
          <Terminal size={12} />
          <span>选择左侧 Agent 启动本地沙箱终端，或通过 Dashboard 快捷胶囊开启会话</span>
        </div>
      ) : (
        sessions.map((session) => {
          const isActive = session.id === activeSessionId;
          return (
            <button
              key={session.id}
              className={`group flex h-8 items-center gap-2 rounded-lg border px-3 text-xs font-semibold transition-all duration-200 ${
                isActive
                  ? "border-purple-500/40 bg-purple-500/10 text-purple-300 shadow-[0_0_15px_rgba(168,85,247,0.15)]"
                  : "border-white/5 bg-white/5 text-slate-400 hover:border-white/10 hover:text-slate-200"
              }`}
              onClick={() => onSelect(session.id)}
            >
              <Terminal size={11} className={isActive ? "text-purple-400" : "text-slate-500"} />
              <span>{session.title}</span>
              <X
                size={12}
                className="ml-1 opacity-40 hover:opacity-100 hover:text-red-400 transition-opacity"
                onClick={(event) => {
                  event.stopPropagation();
                  onClose(session.id);
                }}
              />
            </button>
          );
        })
      )}
    </div>
  );
}
