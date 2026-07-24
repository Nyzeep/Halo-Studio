import { useState } from "react";
import { WorkbenchLayout } from "@halo-studio/ui";
import { ActivityBar, type ActivityView } from "./components/ActivityBar.js";
import { AgentPanel } from "./components/AgentPanel.js";
import { BottomPanel } from "./components/BottomPanel.js";
import { EditorSurface } from "./components/EditorSurface.js";
import { SideBar } from "./components/SideBar.js";
import { StatusBar } from "./components/StatusBar.js";
import { TitleBar } from "./components/TitleBar.js";
import { TrustBanner } from "./components/TrustBanner.js";
import { defaultWorkbenchApi, type WorkbenchApi } from "./api.js";
import { useRuntimeStatus } from "./useRuntimeStatus.js";
import { useWorkspace } from "./useWorkspace.js";

export type { WorkbenchApi } from "./api.js";

export interface AppProps {
  readonly api?: WorkbenchApi;
}

function workspaceName(path: string | undefined): string | undefined {
  if (path === undefined) return undefined;
  return path.split(/[\\/]/u).filter(Boolean).at(-1) ?? path;
}

export function App({ api = defaultWorkbenchApi() }: AppProps): JSX.Element {
  const [activeView, setActiveView] = useState<ActivityView>("files");
  const [commandOpen, setCommandOpen] = useState(false);
  const workspaceState = useWorkspace(api);
  const runtimeState = useRuntimeStatus(api, workspaceState.workspace?.id);
  const workspace = workspaceState.workspace;
  const message = workspaceState.error ?? runtimeState.error;
  const openCodeHealth = runtimeState.bindings.find((binding) => binding.agentKind === "opencode")?.health;
  const canStartOpenCode = workspace?.trustState === "trusted"
    && openCodeHealth !== "healthy"
    && openCodeHealth !== "starting"
    && openCodeHealth !== "stopping";

  const openFolder = (): void => { void workspaceState.openFolder(); };
  const refreshRuntime = (): void => { void runtimeState.refresh(); };
  const startOpenCode = (): void => {
    if (workspace?.trustState !== "trusted") return;
    void runtimeState.startOpenCode(workspace.id);
  };
  const trustAndStart = (): void => {
    void (async () => {
      const trusted = await workspaceState.trustWorkspace();
      if (trusted === undefined) return;
      await runtimeState.refresh(trusted.id);
      await runtimeState.startOpenCode(trusted.id);
    })();
  };
  const selectCommand = (view: "files" | "agent" | "config"): void => {
    setActiveView(view);
    setCommandOpen(false);
  };

  return (
    <WorkbenchLayout
      titleBar={(
        <TitleBar
          commandOpen={commandOpen}
          onToggleCommand={() => setCommandOpen((open) => !open)}
          onSelectCommand={selectCommand}
        />
      )}
      activityBar={<ActivityBar activeView={activeView} onSelect={setActiveView} />}
      sideBar={(
        <SideBar
          activeView={activeView}
          workspace={workspace}
          loading={workspaceState.loading}
          onOpenFolder={openFolder}
        />
      )}
      editor={(
        <div className="editor-shell">
          {workspace !== undefined ? <TrustBanner workspace={workspace} loading={workspaceState.loading} onTrustAndStart={trustAndStart} /> : null}
          <EditorSurface activeView={activeView} workspace={workspace} loading={workspaceState.loading} onOpenFolder={openFolder} />
        </div>
      )}
      auxiliaryBar={(
        <AgentPanel
          bindings={runtimeState.bindings}
          loading={runtimeState.loading}
          error={message}
          canStartOpenCode={canStartOpenCode}
          onStartOpenCode={startOpenCode}
          onRefresh={refreshRuntime}
        />
      )}
      bottomPanel={<BottomPanel workspaceName={workspaceName(workspace?.realPath)} message={message} />}
      statusBar={<StatusBar workspace={workspace} />}
    />
  );
}
