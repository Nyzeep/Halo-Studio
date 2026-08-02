import type { WorkspaceInfo } from '@/shared/types';

type SetSessionMode = (mode: 'code') => void;

export async function createLegacyMainNavCodeSession(args: {
  target: WorkspaceInfo;
  currentWorkspaceId?: string;
  setActiveWorkspace: (workspaceId: string) => Promise<unknown>;
  setSessionMode: SetSessionMode;
  openSessionScene: () => void;
  switchToSessions: () => void;
}): Promise<void> {
  const [flowChat, sessionWorkspace] = await Promise.all([
    import('@/flow_chat/services/FlowChatManager'),
    import('@/app/utils/projectSessionWorkspace'),
  ]);
  const { resolveAgentTypeForSessionCreation } = await import(
    '@/flow_chat/services/flow-chat-manager'
  );

  args.setSessionMode('code');
  args.openSessionScene();
  args.switchToSessions();

  if (args.target.id !== args.currentWorkspaceId) {
    await args.setActiveWorkspace(args.target.id);
  }

  const effectiveMode = await resolveAgentTypeForSessionCreation('agentic', args.target);
  const reusableId = sessionWorkspace.findReusableEmptySessionId(args.target, effectiveMode);
  if (reusableId) {
    await flowChat.flowChatManager.switchChatSession(reusableId);
    return;
  }
  await flowChat.flowChatManager.createChatSession(
    sessionWorkspace.flowChatSessionConfigForWorkspace(args.target),
    effectiveMode,
  );
}

export async function createLegacyWorkspaceItemCodeSession(args: {
  workspace: WorkspaceInfo;
  activateWorkspace: () => Promise<void>;
  setActiveWorkspace: (workspaceId: string) => Promise<unknown>;
  setSessionMode: SetSessionMode;
  openSessionScene: () => void;
  switchToSessions: () => void;
}): Promise<void> {
  const [flowChat, sessionWorkspace, sessionActivation] = await Promise.all([
    import('@/flow_chat/services/FlowChatManager'),
    import('@/app/utils/projectSessionWorkspace'),
    import('@/flow_chat/services/sessionActivation'),
  ]);

  await args.activateWorkspace();
  args.setSessionMode('code');
  args.openSessionScene();
  args.switchToSessions();
  const sessionId = await flowChat.flowChatManager.createChatSession(
    sessionWorkspace.flowChatSessionConfigForWorkspace(args.workspace),
    'agentic',
  );
  await sessionActivation.openMainSession(sessionId, {
    workspaceId: args.workspace.id,
    activateWorkspace: args.setActiveWorkspace,
  });
}
