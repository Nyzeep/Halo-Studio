import { Bot, CircleAlert, Cpu, LoaderCircle, Play, RefreshCw } from "lucide-react";
import type { RuntimeBinding, RuntimeHealth } from "@halo-studio/contracts";

export interface AgentPanelProps {
  readonly bindings: readonly RuntimeBinding[];
  readonly loading: boolean;
  readonly error: string | undefined;
  readonly canStartOpenCode: boolean;
  onStartOpenCode(): void;
  onRefresh(): void;
}

const healthLabels: Readonly<Record<RuntimeHealth, string>> = {
  unavailable: "不可用",
  detected: "已检测",
  installed: "已安装",
  starting: "启动中",
  ready: "已就绪",
  healthy: "运行正常",
  stopping: "停止中",
  stopped: "已停止",
  crashed: "已崩溃",
  "version-mismatch": "版本不匹配",
};

function RuntimeRow({ label, binding }: { readonly label: string; readonly binding: RuntimeBinding | undefined }): JSX.Element {
  const health = binding?.health;
  const Icon = label === "Pi" ? Bot : Cpu;
  return (
    <div className="agent-row">
      <Icon size={17} strokeWidth={1.7} aria-hidden="true" />
      <span className="agent-row__name">{label}</span>
      <span className={`agent-row__health${health === undefined ? " agent-row__health--unknown" : ` agent-row__health--${health}`}`}>
        {health === undefined ? "未探测" : healthLabels[health]}
      </span>
    </div>
  );
}

export function AgentPanel({ bindings, loading, error, canStartOpenCode, onStartOpenCode, onRefresh }: AgentPanelProps): JSX.Element {
  const pi = bindings.find((binding) => binding.agentKind === "pi");
  const openCode = bindings.find((binding) => binding.agentKind === "opencode");
  return (
    <div className="agent-panel">
      <div className="panel-heading">
        <span>AGENT</span>
        <span className="panel-heading__actions">
          {loading ? <LoaderCircle className="spin" size={14} aria-label="正在刷新" /> : null}
          <button className="panel-heading__action" type="button" aria-label="刷新运行时状态" title="刷新运行时状态" disabled={loading} onClick={onRefresh}>
            <RefreshCw size={14} aria-hidden="true" />
          </button>
        </span>
      </div>
      <div className="agent-panel__content">
        <RuntimeRow label="Pi" binding={pi} />
        <RuntimeRow label="OpenCode" binding={openCode} />
        {canStartOpenCode ? (
          <button className="agent-panel__action" type="button" disabled={loading} onClick={onStartOpenCode}>
            <Play size={14} aria-hidden="true" />
            <span>启动 OpenCode</span>
          </button>
        ) : null}
        {error === undefined ? null : <div className="agent-panel__error"><CircleAlert size={14} aria-hidden="true" /><span>{error}</span></div>}
      </div>
    </div>
  );
}
