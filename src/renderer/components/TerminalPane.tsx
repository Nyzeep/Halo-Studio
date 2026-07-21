import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { useEffect, useRef } from "react";
import type { TerminalSessionInfo } from "../../shared/agents";

interface TerminalPaneProps {
  session: TerminalSessionInfo | null;
}

export function TerminalPane({ session }: TerminalPaneProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!hostRef.current || !session) {
      return undefined;
    }

    const terminal = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: "'JetBrains Mono', 'Fira Code', Cascadia Mono, monospace",
      theme: {
        background: "#070512",
        foreground: "#f1f5f9",
        cursor: "#a855f7",
        black: "#070512",
        red: "#ef4444",
        green: "#10b981",
        yellow: "#f59e0b",
        blue: "#6366f1",
        magenta: "#a855f7",
        cyan: "#06b6d4",
        white: "#f1f5f9",
        brightBlack: "#4b5563",
        brightRed: "#f87171",
        brightGreen: "#34d399",
        brightYellow: "#fbbf24",
        brightBlue: "#818cf8",
        brightMagenta: "#c084fc",
        brightCyan: "#22d3ee",
        brightWhite: "#ffffff"
      }
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(hostRef.current);
    fitAddon.fit();
    terminal.focus();

    const disposeData = window.halo.sessions.onData(({ sessionId, data }) => {
      if (sessionId === session.id) {
        terminal.write(data);
      }
    });

    const onData = terminal.onData((data) => {
      void window.halo.sessions.write(session.id, data);
    });

    const resizeObserver = new ResizeObserver(() => {
      fitAddon.fit();
      void window.halo.sessions.resize(session.id, terminal.cols, terminal.rows);
    });
    resizeObserver.observe(hostRef.current);

    return () => {
      disposeData();
      onData.dispose();
      resizeObserver.disconnect();
      terminal.dispose();
    };
  }, [session]);

  if (!session) {
    return (
      <div className="relative flex h-full w-full items-center justify-center bg-[#070512] text-xs text-slate-500">
        <div className="starfield" />
        <div className="relative z-10 text-center space-y-2.5">
          <p className="font-semibold text-slate-400">子进程沙箱就绪 · 终端空闲</p>
          <p className="text-[11px] text-slate-600">请在左侧 Agent 面板点击“启动”或者返回 Dashboard 开启新会话</p>
        </div>
      </div>
    );
  }

  return (
    <div className="relative h-full w-full bg-[#070512] p-4">
      <div ref={hostRef} className="h-full w-full overflow-hidden" />
    </div>
  );
}
