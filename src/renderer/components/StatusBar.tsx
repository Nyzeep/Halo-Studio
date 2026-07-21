import type { TerminalSessionInfo } from "../../shared/agents";

interface StatusBarProps {
  activeSession: TerminalSessionInfo | null;
}

export function StatusBar({ activeSession }: StatusBarProps) {
  return (
    <footer className="flex h-8 items-center justify-between border-t border-white/5 bg-[#05030a] px-5 text-[10px] font-medium text-slate-500">
      <span className="flex items-center gap-1.5">
        <span className="h-1.5 w-1.5 rounded-full bg-purple-500 animate-pulse" />
        <span>Halo Studio · Local Sandbox Console</span>
      </span>
      <span>
        {activeSession ? (
          <span className="text-purple-400 font-mono">
            Active: {activeSession.title} · PID: {activeSession.status}
          </span>
        ) : (
          "Idle · Standing By"
        )}
      </span>
    </footer>
  );
}
