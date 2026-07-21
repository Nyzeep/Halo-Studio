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
      fontFamily: "Cascadia Mono, Consolas, monospace",
      theme: {
        background: "#0b0f14",
        foreground: "#dbeafe",
        cursor: "#22d3ee"
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
      <div className="flex h-full items-center justify-center bg-halo-bg text-sm text-slate-500">
        启动一个 Agent 后，终端会显示在这里。
      </div>
    );
  }

  return <div ref={hostRef} className="h-full w-full overflow-hidden bg-halo-bg p-3" />;
}
