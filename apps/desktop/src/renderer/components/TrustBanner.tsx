import { ShieldCheck, ShieldAlert } from "lucide-react";
import type { Workspace } from "@halo-studio/contracts";

export interface TrustBannerProps {
  readonly workspace: Workspace;
  readonly loading: boolean;
  onTrustAndStart(): void;
}

export function TrustBanner({ workspace, loading, onTrustAndStart }: TrustBannerProps): JSX.Element {
  if (workspace.trustState === "trusted") return <></>;
  return (
    <section className="trust-banner" aria-label="工作区信任">
      <ShieldAlert size={18} aria-hidden="true" />
      <div className="trust-banner__copy">
        <strong>此工作区尚未信任</strong>
        <span>信任允许加载项目配置，不等于系统沙箱。</span>
      </div>
      <button type="button" disabled={loading} onClick={onTrustAndStart}>
        <ShieldCheck size={16} aria-hidden="true" />
        <span>信任并启动</span>
      </button>
    </section>
  );
}
