import { CheckCircle2, Folder, ShieldAlert } from "lucide-react";
import type { Workspace } from "@halo-studio/contracts";

export interface StatusBarProps {
  readonly workspace: Workspace | undefined;
}

export function StatusBar({ workspace }: StatusBarProps): JSX.Element {
  const trusted = workspace?.trustState === "trusted";
  return (
    <div className="statusbar" role="status" aria-label="状态栏">
      <span className="statusbar__item">
        <Folder size={13} aria-hidden="true" />
        <span>{workspace === undefined ? "未打开工作区" : workspace.realPath}</span>
      </span>
      {workspace === undefined ? null : (
        <span className={`statusbar__item${trusted ? " statusbar__item--trusted" : " statusbar__item--warning"}`}>
          {trusted ? <CheckCircle2 size={13} aria-hidden="true" /> : <ShieldAlert size={13} aria-hidden="true" />}
          <span>{trusted ? "已信任" : "未信任"}</span>
        </span>
      )}
    </div>
  );
}
