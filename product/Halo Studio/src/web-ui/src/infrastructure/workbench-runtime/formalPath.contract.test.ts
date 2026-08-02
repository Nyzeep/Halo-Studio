import { describe, expect, it } from 'vitest';

import appSource from '../../app/App.tsx?raw';
import appLayoutSource from '../../app/layout/AppLayout.tsx?raw';
import mainNavSource from '../../app/components/NavPanel/MainNav.tsx?raw';
import sessionTitleSource from '../../app/hooks/useCurrentSessionTitle.ts?raw';
import sceneViewportSource from '../../app/scenes/SceneViewport.tsx?raw';
import workbenchSessionSceneSource from '../../app/scenes/session/WorkbenchSessionScene.tsx?raw';
import deferredStartupSource from '../../app/startup/deferredStartupSystems.ts?raw';
import chatInputSource from '../../flow_chat/components/ChatInput.tsx?raw';
import permissionHookSource from '../../app/hooks/usePermissionRequestNotify.ts?raw';
import dialogCompletionHookSource from '../../app/hooks/useDialogCompletionNotify.ts?raw';
import workspaceItemSource from '../../app/components/NavPanel/sections/workspaces/WorkspaceItem.tsx?raw';
import workspaceProviderSource from '../contexts/WorkspaceProvider.tsx?raw';
import workingCopySource from '../../app/scenes/git/views/WorkingCopyView.tsx?raw';
import gitAgentHookSource from '../../tools/git/hooks/useGitAgent.ts?raw';
import clientSource from './client.ts?raw';
import lifecycleSource from './lifecycle.ts?raw';
import typesSource from './types.ts?raw';

describe('Halo workbench formal path', () => {
  it('keeps transport details and secrets behind the infrastructure client', () => {
    for (const forbidden of [
      'Authorization',
      'Bearer ',
      'EventSource',
      'WebSocket',
      'fetch(',
      '127.0.0.1',
      'localhost:',
      'sidecar',
    ]) {
      expect(clientSource).not.toContain(forbidden);
    }
    expect(typesSource).toContain('pi-rpc');
    expect(clientSource).toContain('HALO_WORKBENCH_RUNTIME_SNAPSHOT_COMMAND');
  });

  it('starts the workbench projection only for Halo Tauri and gates legacy owners', () => {
    expect(appSource).toContain('isHaloLocalCodingScope() && isTauriRuntime()');
    expect(appSource).toContain('workbenchRuntimeStore.getState().start()');
    expect(appSource).toContain('includeAgentExtensions: !isHaloLocalCodingScope()');
    expect(deferredStartupSource).toContain('if (includeAgentExtensions)');
    expect(appLayoutSource).toContain('if (isHaloLocalCodingScope()) return;');
    expect(appLayoutSource).not.toContain("import { FlowChatManager }");
    expect(permissionHookSource).toContain('if (isHaloLocalCodingScope()) return;');
    expect(mainNavSource).toContain('workbenchRuntimeStore');
    expect(mainNavSource).not.toContain('flowChatManager');
    expect(workspaceItemSource).toContain('WorkbenchSessionsSection');
    expect(workspaceItemSource).not.toContain('flowChatManager');
  });

  it('keeps the Halo composition seam out of the legacy session graph', () => {
    expect(appSource).not.toContain("import AskUserAnnouncer from './components/NavPanel/AskUserAnnouncer'");
    expect(appSource).toContain("lazy(() => import('./components/NavPanel/AskUserAnnouncer'))");

    expect(sceneViewportSource).not.toContain("import SessionScene from './session/SessionScene'");
    expect(sceneViewportSource).toContain("lazy(() => import('./session/SessionScene'))");
    expect(sceneViewportSource).toContain("import('./session/WorkbenchSessionScene')");
    expect(sceneViewportSource).not.toContain("from '@/flow_chat");

    expect(workbenchSessionSceneSource).toContain('workbenchRuntimeStore');
    expect(workbenchSessionSceneSource).not.toContain('submitIntent');
    expect(workbenchSessionSceneSource).not.toMatch(/<(input|textarea|button)\b/);

    expect(mainNavSource).not.toContain("import NavSearchDialog from './NavSearchDialog'");
    expect(mainNavSource).toContain("lazy(() => import('./NavSearchDialog'))");
    expect(mainNavSource).toContain('!isHaloLocalCodingScope()');

    expect(dialogCompletionHookSource).not.toContain("import { agentAPI } from '@/infrastructure/api'");
    expect(dialogCompletionHookSource).not.toContain("import { flowChatStore } from '@/flow_chat/store/FlowChatStore'");
    expect(dialogCompletionHookSource).toContain('if (isHaloLocalCodingScope()) return;');

    expect(chatInputSource).toContain('const legacyExecutionEnabled = !isHaloLocalCodingScope();');
    expect(chatInputSource).toContain('disabled={!legacyExecutionEnabled}');

    expect(workingCopySource).toContain("import { isHaloLocalCodingScope } from '@/infrastructure/runtime';");
    expect(workingCopySource).toContain('const legacyGitAiEnabled = !isHaloLocalCodingScope();');
    expect(workingCopySource).toContain('{legacyGitAiEnabled && (');
    expect(workingCopySource).toContain('if (!legacyGitAiEnabled) return;');
    expect(gitAgentHookSource).toContain('enabled?: boolean;');
    expect(gitAgentHookSource).toContain('if (!enabled) return;');

    expect(appSource).not.toContain(
      "from '../flow_chat/components/toolbar-mode/ToolbarModeProvider'"
    );
    expect(appSource).toContain(
      "import('../flow_chat/components/toolbar-mode/ToolbarModeProvider')"
    );
    expect(permissionHookSource).not.toContain("from '@/infrastructure/api'");
    expect(permissionHookSource).toContain(
      "import('@/infrastructure/api/service-api/AgentAPI')"
    );
    expect(sessionTitleSource).not.toContain(
      "from '../../flow_chat/store/FlowChatStore'"
    );
    expect(sessionTitleSource).toContain(
      "import('../../flow_chat/store/FlowChatStore')"
    );
    expect(appLayoutSource).not.toContain("from '@/infrastructure/api'");
    expect(workspaceProviderSource).toContain('submitWorkbenchRuntimeCloseIntent');
  });

  it('closes the active runtime before workspace transitions', () => {
    expect(workspaceProviderSource).toContain('closeWorkbenchRuntimeBeforeActiveWorkspaceTransition');
    expect(workspaceProviderSource).toContain(
      'activeWorkspace.workspaceKind === WorkspaceKind.Remote'
    );
    expect(lifecycleSource).toContain(
      'activeWorkspace.workspaceKind === WorkspaceKind.Remote'
    );
    for (const transition of [
      'openWorkspace',
      'closeWorkspace',
      'closeWorkspaceById',
      'switchWorkspace',
      'setActiveWorkspace',
    ]) {
      expect(workspaceProviderSource).toContain(transition);
    }
  });
});
