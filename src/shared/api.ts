import type { AgentId, AgentInfo, TerminalSessionInfo } from "./agents.js";
import type {
  ConfigRollbackRequest,
  ConfigRollbackResult,
  ConfigBackupEntry,
  ConfirmedConfigWriteRequest,
  ConfigWriteRequest,
  ConfigWriteResult,
  RealConfigWritePlan,
  RealConfigWritePlanRequest
} from "./config.js";
import type { McpConfigPreview, McpServerConfig } from "./mcp.js";

export interface StartSessionRequest {
  agentId: AgentId;
  cwd: string;
}

export interface HaloApi {
  agents: {
    detectAll(): Promise<AgentInfo[]>;
  };
  config: {
    applyDemoWrite(request: ConfigWriteRequest): Promise<ConfigWriteResult>;
    applyConfirmedWrite(request: ConfirmedConfigWriteRequest): Promise<ConfigWriteResult>;
    listDemoBackups(targetPath: string): Promise<ConfigBackupEntry[]>;
    planRealWrite(request: RealConfigWritePlanRequest): Promise<RealConfigWritePlan>;
    rollbackWrite(request: ConfigRollbackRequest): Promise<ConfigRollbackResult>;
  };
  mcp: {
    planProjectMcpWrite(workspaceRoot: string, preview: McpConfigPreview): Promise<RealConfigWritePlan>;
    previewConfig(server: McpServerConfig): Promise<McpConfigPreview[]>;
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
