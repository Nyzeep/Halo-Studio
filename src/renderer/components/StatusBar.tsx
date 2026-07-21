import type { TerminalSessionInfo } from "../../shared/agents";

interface StatusBarProps {
  activeSession: TerminalSessionInfo | null;
}

export function StatusBar({ activeSession }: StatusBarProps) {
  return (
    <footer className="flex h-8 items-center justify-between border-t border-halo-line bg-halo-panel px-4 text-xs text-slate-500">
      <span>Halo Studio · Windows Preview</span>
      <span>{activeSession ? `${activeSession.title} · ${activeSession.status}` : "Idle"}</span>
    </footer>
  );
}
