import { X } from "lucide-react";
import type { TerminalSessionInfo } from "../../shared/agents";

interface SessionTabsProps {
  sessions: TerminalSessionInfo[];
  activeSessionId: string | null;
  onSelect(sessionId: string): void;
  onClose(sessionId: string): void;
}

export function SessionTabs({ sessions, activeSessionId, onSelect, onClose }: SessionTabsProps) {
  return (
    <div className="flex h-12 items-center gap-2 border-b border-halo-line bg-halo-panel px-3">
      {sessions.length === 0 ? (
        <div className="text-sm text-slate-500">选择左侧 Agent 启动会话</div>
      ) : (
        sessions.map((session) => (
          <button
            key={session.id}
            className={`flex h-8 items-center gap-2 rounded border px-3 text-sm ${
              session.id === activeSessionId
                ? "border-halo-cyan bg-halo-cyan/10 text-halo-cyan"
                : "border-halo-line bg-halo-panelSoft text-slate-300"
            }`}
            onClick={() => onSelect(session.id)}
          >
            {session.title}
            <X
              size={14}
              onClick={(event) => {
                event.stopPropagation();
                onClose(session.id);
              }}
            />
          </button>
        ))
      )}
    </div>
  );
}
