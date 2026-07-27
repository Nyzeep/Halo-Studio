import { useMemo, useState } from "react";
import { WorkbenchLayout } from "@halo-studio/ui";
import { ActivityBar, type ActivityView } from "./components/ActivityBar.js";
import { AgentPanel } from "./components/AgentPanel.js";
import { BottomPanel } from "./components/BottomPanel.js";
import { EditorSurface } from "./components/EditorSurface.js";
import { SideBar } from "./components/SideBar.js";
import { StatusBar } from "./components/StatusBar.js";
import { SessionWorkbench } from "./components/SessionWorkbench.js";
import { TitleBar } from "./components/TitleBar.js";
import { TrustBanner } from "./components/TrustBanner.js";
import { defaultWorkbenchApi, type WorkbenchApi } from "./api.js";
import { useRuntimeStatus } from "./useRuntimeStatus.js";
import { useManagedSessions } from "./useManagedSessions.js";
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
  const piHealth = runtimeState.bindings.find((binding) => binding.agentKind === "pi")?.health;
  const openCodeHealth = runtimeState.bindings.find((binding) => binding.agentKind === "opencode")?.health;
  const workspaceTrusted = workspace?.trustState === "trusted";
  const sessionAgentKinds = useMemo(() => runtimeState.bindings
    .filter((binding) => binding.capabilities.sessions.supported)
    .map((binding) => binding.agentKind), [runtimeState.bindings]);
  const sessionState = useManagedSessions(
    api,
    workspace?.id,
    workspaceTrusted === true && activeView === "agent",
    sessionAgentKinds,
  );
  const canStartPi = workspaceTrusted
    && (piHealth === "detected" || piHealth === "stopped");
  const canStopPi = workspaceTrusted
    && (piHealth === "ready" || piHealth === "starting");
  const canRetryPi = workspaceTrusted
    && (piHealth === "crashed" || piHealth === "unavailable");
  const canStartOpenCode = workspaceTrusted
    && openCodeHealth !== "healthy"
    && openCodeHealth !== "starting"
    && openCodeHealth !== "stopping";

  const openFolder = (): void => { void workspaceState.openFolder(); };
  const refreshRuntime = (): void => { void runtimeState.refresh(); };
  const startOpenCode = (): void => {
    if (!workspaceTrusted || workspace === undefined) return;
    void runtimeState.startOpenCode(workspace.id);
  };
  const startPi = (): void => {
    if (!canStartPi || workspace === undefined) return;
    void runtimeState.startPi(workspace.id);
  };
  const stopPi = (): void => {
    if (!canStopPi || workspace === undefined) return;
    void runtimeState.stopPi(workspace.id);
  };
  const retryPi = (): void => {
    if (!canRetryPi || workspace === undefined) return;
    void runtimeState.retryPi(workspace.id);
  };
  const trustWorkspace = (): void => {
    void (async () => {
      const trusted = await workspaceState.trustWorkspace();
      if (trusted === undefined) return;
      await runtimeState.refresh(trusted.id);
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
          {workspace !== undefined ? <TrustBanner workspace={workspace} loading={workspaceState.loading} onTrust={trustWorkspace} /> : null}
          {workspace !== undefined && workspaceTrusted && activeView === "agent" ? (
            <SessionWorkbench
              sessions={sessionState.sessions}
              selectedSession={sessionState.selectedSession}
              messages={sessionState.messages}
              commands={sessionState.commands}
              draft={sessionState.draft}
              commandFilter={sessionState.commandFilter}
              loading={sessionState.loading}
              messagesLoading={sessionState.messagesLoading}
              commandsLoading={sessionState.commandsLoading}
              sending={sessionState.sending}
              aborting={sessionState.aborting}
              error={sessionState.error}
              canCreateSession={sessionState.canCreateSession}
              onSelectSession={(session) => { void sessionState.selectSession(session); }}
              onCreateSession={(agentKind) => { void sessionState.createSession(agentKind); }}
              onDraftChange={sessionState.setDraft}
              onCommandFilterChange={sessionState.setCommandFilter}
              onSend={(session, draft) => { void sessionState.send(session, draft); }}
              onAbort={(session) => { void sessionState.abort(session); }}
            />
          ) : (
            <EditorSurface activeView={activeView} workspace={workspace} loading={workspaceState.loading} onOpenFolder={openFolder} />
          )}
        </div>
      )}
      auxiliaryBar={(
        <AgentPanel
          bindings={runtimeState.bindings}
          loading={runtimeState.loading}
          error={message}
          workspaceTrusted={workspaceTrusted}
          canStartPi={canStartPi}
          canStopPi={canStopPi}
          canRetryPi={canRetryPi}
          onStartPi={startPi}
          onStopPi={stopPi}
          onRetryPi={retryPi}
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
