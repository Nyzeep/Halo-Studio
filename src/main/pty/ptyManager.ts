import { randomUUID } from "node:crypto";
import os from "node:os";
import path from "node:path";
import * as pty from "node-pty";
import type { StartSessionRequest } from "../../shared/api.js";
import type { AgentId, TerminalSessionInfo } from "../../shared/agents.js";

interface PtyManagerEvents {
  onData(sessionId: string, data: string): void;
  onExit(sessionId: string, exitCode: number | null): void;
}

interface SessionRecord {
  info: TerminalSessionInfo;
  process: pty.IPty;
}

const commandByAgent: Record<AgentId, string> = {
  "claude-code": "claude",
  "codex-cli": "codex",
  opencode: "opencode",
  pi: "pi"
};

export class PtyManager {
  private readonly sessions = new Map<string, SessionRecord>();

  constructor(private readonly events: PtyManagerEvents) {}

  list(): TerminalSessionInfo[] {
    return Array.from(this.sessions.values()).map((session) => session.info);
  }

  async start(request: StartSessionRequest): Promise<TerminalSessionInfo> {
    const sessionId = randomUUID();
    const shell = commandByAgent[request.agentId];
    const cwd = path.resolve(request.cwd || os.homedir());

    const child = pty.spawn(shell, [], {
      name: "xterm-256color",
      cols: 100,
      rows: 32,
      cwd,
      env: { ...process.env }
    });

    const info: TerminalSessionInfo = {
      id: sessionId,
      agentId: request.agentId,
      title: shell,
      cwd,
      status: "running",
      createdAt: new Date().toISOString()
    };

    this.sessions.set(sessionId, { info, process: child });

    child.onData((data) => this.events.onData(sessionId, data));
    child.onExit(({ exitCode }) => {
      this.sessions.delete(sessionId);
      this.events.onExit(sessionId, exitCode);
    });

    return info;
  }

  write(sessionId: string, data: string): void {
    this.sessions.get(sessionId)?.process.write(data);
  }

  resize(sessionId: string, cols: number, rows: number): void {
    this.sessions.get(sessionId)?.process.resize(cols, rows);
  }

  stop(sessionId: string): void {
    this.sessions.get(sessionId)?.process.kill();
    this.sessions.delete(sessionId);
  }
}
