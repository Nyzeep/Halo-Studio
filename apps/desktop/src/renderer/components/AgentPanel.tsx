import {
  Bot,
  CircleAlert,
  Cpu,
  LoaderCircle,
  Play,
  RefreshCw,
  RotateCcw,
  ShieldCheck,
  Square,
} from "lucide-react";
import type { RuntimeBinding, RuntimeHealth } from "@halo-studio/contracts";

export interface AgentPanelProps {
  readonly bindings: readonly RuntimeBinding[];
  readonly loading: boolean;
  readonly error: string | undefined;
  readonly workspaceTrusted: boolean;
  readonly canStartPi: boolean;
  readonly canStopPi: boolean;
  readonly canRetryPi: boolean;
  onStartPi(): void;
  onStopPi(): void;
  onRetryPi(): void;
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

function piDetectionSummary(binding: RuntimeBinding | undefined): string {
  if (binding === undefined) return "未检测 / 待启动";
  const details = [
    binding.executable === undefined ? undefined : `可执行文件：${binding.executable}`,
    binding.version === undefined ? undefined : `版本：${binding.version}`,
  ].filter((detail): detail is string => detail !== undefined);
  if (details.length > 0) return details.join(" · ");
  if (binding.health === "unavailable") return "未检测到可启动的 Pi";
  return "未提供检测详情";
}

interface PiControlsProps {
  readonly binding: RuntimeBinding | undefined;
  readonly loading: boolean;
  readonly workspaceTrusted: boolean;
  readonly canStart: boolean;
  readonly canStop: boolean;
  readonly canRetry: boolean;
  onStart(): void;
  onStop(): void;
  onRetry(): void;
}

function PiControls({
  binding,
  loading,
  workspaceTrusted,
  canStart,
  canStop,
  canRetry,
  onStart,
  onStop,
  onRetry,
}: PiControlsProps): JSX.Element {
  if (binding === undefined) return <p className="agent-panel__notice">尚未请求 Pi 检测。</p>;
  if (!workspaceTrusted) {
    return <p className="agent-panel__notice">信任当前工作区后才可启动 Pi。</p>;
  }
  if (canStop) {
    return (
      <button className="agent-panel__action" type="button" aria-label="停止 Pi" disabled={loading} onClick={onStop}>
        <Square size={13} aria-hidden="true" />
        <span>停止 Pi</span>
      </button>
    );
  }
  if (canRetry) {
    return (
      <button className="agent-panel__action" type="button" aria-label="重试 Pi" disabled={loading} onClick={onRetry}>
        <RotateCcw size={14} aria-hidden="true" />
        <span>重试 Pi</span>
      </button>
    );
  }
  if (canStart) {
    return (
      <button className="agent-panel__action" type="button" aria-label="使用受管启动配置启动 Pi" disabled={loading} onClick={onStart}>
        <Play size={14} aria-hidden="true" />
        <span>使用受管启动配置启动 Pi</span>
      </button>
    );
  }
  if (binding.health === "starting") return <p className="agent-panel__notice">Pi 正在完成就绪检查。</p>;
  if (binding.health === "stopping") return <p className="agent-panel__notice">Pi 正在停止。</p>;
  return <></>;
}

export function AgentPanel({
  bindings,
  loading,
  error,
  workspaceTrusted,
  canStartPi,
  canStopPi,
  canRetryPi,
  onStartPi,
  onStopPi,
  onRetryPi,
  canStartOpenCode,
  onStartOpenCode,
  onRefresh,
}: AgentPanelProps): JSX.Element {
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
        <section className="agent-runtime-card" aria-label="Pi 受管运行时">
          <RuntimeRow label="Pi" binding={pi} />
          <p className="agent-runtime-card__detection">{piDetectionSummary(pi)}</p>
          <div className="pi-launch-boundary">
            <ShieldCheck size={14} aria-hidden="true" />
            <div>
              <strong>受管启动配置</strong>
              <span>模型、thinking 与 Provider 凭据仅由主进程解析，不会通过界面或 IPC 输入、展示。</span>
            </div>
          </div>
          <PiControls
            binding={pi}
            loading={loading}
            workspaceTrusted={workspaceTrusted}
            canStart={canStartPi}
            canStop={canStopPi}
            canRetry={canRetryPi}
            onStart={onStartPi}
            onStop={onStopPi}
            onRetry={onRetryPi}
          />
        </section>
        <div className="agent-panel__divider" aria-hidden="true" />
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
