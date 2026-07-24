import { Code2, FolderOpen, History, Search, Settings2 } from "lucide-react";
import type { Workspace } from "@halo-studio/contracts";
import type { ActivityView } from "./ActivityBar.js";

export interface EditorSurfaceProps {
  readonly activeView: ActivityView;
  readonly workspace: Workspace | undefined;
  readonly loading: boolean;
  onOpenFolder(): void;
}

function workspaceName(workspace: Workspace): string {
  return workspace.realPath.split(/[\\/]/u).filter(Boolean).at(-1) ?? workspace.realPath;
}

function viewLabel(view: ActivityView): string {
  return view === "search" ? "搜索" : view === "history" ? "历史" : "设置";
}

export function EditorSurface({ activeView, workspace, loading, onOpenFolder }: EditorSurfaceProps): JSX.Element {
  if (workspace === undefined) {
    return (
      <div className="editor-empty">
        <Code2 size={34} strokeWidth={1.35} aria-hidden="true" />
        <h1>开始工作</h1>
        <button type="button" disabled={loading} onClick={onOpenFolder}>
          <FolderOpen size={16} aria-hidden="true" />
          <span>打开文件夹</span>
        </button>
      </div>
    );
  }

  if (activeView === "search" || activeView === "history" || activeView === "settings") {
    const Icon = activeView === "search" ? Search : activeView === "history" ? History : Settings2;
    return (
      <div className="editor-empty editor-empty--view">
        <Icon size={34} strokeWidth={1.35} aria-hidden="true" />
        <h1>{viewLabel(activeView)}</h1>
        <span>暂无内容</span>
      </div>
    );
  }

  const configuration = activeView === "config";
  const agentView = activeView === "agent";
  return (
    <div className="editor-surface">
      <div className="editor-tabs" aria-label="编辑器标签">
        <div className="editor-tab editor-tab--active">
          <Code2 size={14} aria-hidden="true" />
          <span>{configuration ? "配置概览" : agentView ? "Agent 状态" : workspaceName(workspace)}</span>
        </div>
      </div>
      <div className="editor-content">
        <div className="editor-content__path" title={workspace.realPath}>{workspace.realPath}</div>
        <div className="editor-content__focus">
          <span>{configuration ? "配置域" : agentView ? "Agent" : "开发域"}</span>
          <strong>{configuration || agentView ? workspaceName(workspace) : workspace.realPath}</strong>
        </div>
      </div>
    </div>
  );
}
