import { ChevronDown, FolderOpen, HardDrive, History, PackageSearch, Search, Settings2, SlidersHorizontal } from "lucide-react";
import type { Workspace } from "@halo-studio/contracts";
import type { ActivityView } from "./ActivityBar.js";

export interface SideBarProps {
  readonly activeView: ActivityView;
  readonly workspace: Workspace | undefined;
  readonly loading: boolean;
  onOpenFolder(): void;
}

function workspaceName(workspace: Workspace | undefined): string {
  if (workspace === undefined) return "未打开文件夹";
  return workspace.realPath.split(/[\\/]/u).filter(Boolean).at(-1) ?? workspace.realPath;
}

export function SideBar({ activeView, workspace, loading, onOpenFolder }: SideBarProps): JSX.Element {
  const configuration = activeView === "config";
  const auxiliaryView = activeView === "search" || activeView === "history" || activeView === "settings";
  const auxiliaryLabel = activeView === "search" ? "搜索" : activeView === "history" ? "历史" : "设置";
  const AuxiliaryIcon = activeView === "search" ? Search : activeView === "history" ? History : Settings2;
  return (
    <div className="sidebar">
      <div className="panel-heading">
        <span>{configuration ? "配置" : activeView === "agent" ? "AGENT" : "工作区"}</span>
        <span className="panel-heading__actions">
          {workspace !== undefined ? (
            <button className="panel-heading__action" type="button" aria-label="打开文件夹" title="打开文件夹" disabled={loading} onClick={onOpenFolder}>
              <FolderOpen size={14} aria-hidden="true" />
            </button>
          ) : null}
          {configuration ? <SlidersHorizontal size={14} aria-hidden="true" /> : null}
        </span>
      </div>
      {configuration ? (
        <div className="sidebar__tree" aria-label="配置域导航">
          <div className="sidebar__workspace-context" title={workspace?.realPath}>
            <span>工作区</span>
            <strong>{workspaceName(workspace)}</strong>
          </div>
          <div className="sidebar__empty"><SlidersHorizontal size={16} aria-hidden="true" /><span>配置写入尚未开放</span></div>
        </div>
      ) : auxiliaryView ? (
        <div className="sidebar__tree" aria-label={`${auxiliaryLabel}导航`}>
          <div className="sidebar__group"><AuxiliaryIcon size={14} aria-hidden="true" /><span>{auxiliaryLabel}</span></div>
          <div className="sidebar__empty"><AuxiliaryIcon size={16} aria-hidden="true" /><span>暂无内容</span></div>
        </div>
      ) : activeView === "agent" ? (
        <div className="sidebar__tree" aria-label="Agent 导航">
          <div className="sidebar__group"><ChevronDown size={14} aria-hidden="true" /><span>运行时</span></div>
          <div className="sidebar__item"><HardDrive size={14} aria-hidden="true" /><span>Pi</span><small>受管</small></div>
          <div className="sidebar__item"><PackageSearch size={14} aria-hidden="true" /><span>OpenCode</span><small>本地</small></div>
        </div>
      ) : (
        <div className="sidebar__tree">
          <div className="sidebar__group"><ChevronDown size={14} aria-hidden="true" /><span>工作区</span></div>
          {workspace === undefined ? (
            <div className="sidebar__empty"><FolderOpen size={16} aria-hidden="true" /><span>{loading ? "正在读取工作区" : "未打开文件夹"}</span></div>
          ) : (
            <div className="sidebar__workspace-path" title={workspace.realPath}>{workspace.realPath}</div>
          )}
        </div>
      )}
    </div>
  );
}
