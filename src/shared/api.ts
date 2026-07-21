import type { AgentId, AgentInfo, TerminalSessionInfo } from "./agents.js";

export interface StartSessionRequest {
  agentId: AgentId;
  cwd: string;
}

export interface HaloApi {
  agents: {
    detectAll(): Promise<AgentInfo[]>;
  };
  sessions: {
    start(request: StartSessionRequest): Promise<TerminalSessionInfo>;
    stop(sessionId: string): Promise<void>;
    write(sessionId: string, data: string): Promise<void>;
    resize(sessionId: string, cols: number, rows: number): Promise<void>;
    onData(callback: (event: { sessionId: string; data: string }) => void): () => void;
    onExit(callback: (event: { sessionId: string; exitCode: number | null }) => void): () => void;
  };
}

declare global {
  interface Window {
    halo: HaloApi;
  }
}
