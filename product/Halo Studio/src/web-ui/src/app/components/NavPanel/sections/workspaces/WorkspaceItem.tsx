import React, { lazy, Suspense, useCallback, useState } from 'react';
import { Copy, Folder, FolderOpen, FolderSearch, MoreHorizontal, Pencil, Plus } from 'lucide-react';
import { InputDialog, Tooltip } from '@/component-library';
import { useI18n } from '@/infrastructure/i18n';
import { useWorkspaceContext } from '@/infrastructure/contexts/WorkspaceContext';
import { systemAPI } from '@/infrastructure/api/service-api/SystemAPI';
import { notificationService } from '@/shared/notification-system';
import { createLogger } from '@/shared/utils/logger';
import type { WorkspaceInfo } from '@/shared/types';
import WorkbenchSessionsSection from '../sessions/WorkbenchSessionsSection';
import { useApp } from '@/app/hooks/useApp';
import { useSceneManager } from '@/app/hooks/useSceneManager';
import { useSessionModeStore } from '@/app/stores/sessionModeStore';
import { isHaloLocalCodingScope } from '@/infrastructure/runtime';
import { useStore } from 'zustand';
import {
  createWorkbenchRuntimeRequestId,
  workbenchRuntimeStore,
} from '@/infrastructure/workbench-runtime';

const MAX_WORKSPACE_NAME_CHARS = 80;
const log = createLogger('WorkspaceItem');
const LegacySessionsSection = lazy(() => import('../sessions/SessionsSection'));

function containsWorkspaceNameControlCharacter(value: string): boolean {
  return Array.from(value).some((character) => {
    const codePoint = character.codePointAt(0);
    return codePoint !== undefined && (codePoint <= 0x1f || codePoint === 0x7f);
  });
}

interface WorkspaceItemProps {
  workspace: WorkspaceInfo;
  isActive: boolean;
  isSingle?: boolean;
  draggable?: boolean;
  isDragging?: boolean;
  onDragStart?: React.DragEventHandler<HTMLDivElement>;
  onDragEnd?: React.DragEventHandler<HTMLDivElement>;
}

const WorkspaceItem: React.FC<WorkspaceItemProps> = ({
  workspace,
  isActive,
  draggable = false,
  isDragging = false,
  onDragStart,
  onDragEnd,
}) => {
  const { t } = useI18n('common');
  const { setActiveWorkspace, closeWorkspaceById, renameWorkspace } = useWorkspaceContext();
  const { switchLeftPanelTab } = useApp();
  const { openScene } = useSceneManager();
  const setSessionMode = useSessionModeStore(s => s.setMode);
  const runtimeSnapshot = useStore(workbenchRuntimeStore, state => state.snapshot);
  const legacyNavigationEnabled = !isHaloLocalCodingScope();
  const canOpenWorkbenchSession = Boolean(workspace.rootPath);
  const [menuOpen, setMenuOpen] = useState(false);
  const [renameDialogOpen, setRenameDialogOpen] = useState(false);
  const [sessionsCollapsed, setSessionsCollapsed] = useState(false);
  const canCreateWorkbenchSession = runtimeSnapshot?.phase === 'ready'
    && runtimeSnapshot.workspace?.workspaceId === workspace.id;

  const activateWorkspace = useCallback(async () => {
    if (!isActive) {
      await setActiveWorkspace(workspace.id);
    }
  }, [isActive, setActiveWorkspace, workspace.id]);

  const handleCardNameClick = useCallback(async () => {
    if (!isActive) {
      await setActiveWorkspace(workspace.id);
      setSessionsCollapsed(false);
      return;
    }
    setSessionsCollapsed(prev => !prev);
  }, [isActive, setActiveWorkspace, workspace.id]);

  const handleOpenFiles = useCallback(async () => {
    try {
      await activateWorkspace();
      openScene('file-viewer');
      switchLeftPanelTab('files');
    } catch (error) {
      notificationService.error(
        error instanceof Error ? error.message : t('nav.items.project'),
        { duration: 4000 }
      );
    }
  }, [activateWorkspace, openScene, switchLeftPanelTab, t]);

  const handleCreateCodeSession = useCallback(async () => {
    setMenuOpen(false);
    try {
      if (legacyNavigationEnabled) {
        const { createLegacyWorkspaceItemCodeSession } = await import('../../legacyCodeSession');
        await createLegacyWorkspaceItemCodeSession({
          workspace,
          activateWorkspace,
          setActiveWorkspace,
          setSessionMode,
          openSessionScene: () => openScene('session'),
          switchToSessions: () => switchLeftPanelTab('sessions'),
        });
        return;
      }

      await activateWorkspace();
      openScene('session');
      switchLeftPanelTab('sessions');
      if (!canCreateWorkbenchSession) return;
      const requestId = createWorkbenchRuntimeRequestId('create-session');
      await workbenchRuntimeStore.getState().submitIntent({
        requestId,
        // A navigation-created session is one explicit Halo task. Reusing the
        // request id keeps the task identity stable across request retries
        // while the runtime owns the session id and Pi history path.
        intent: { type: 'createSession', taskId: requestId, mode: 'standard' },
      });
    } catch {
      log.error('Failed to create code session');
    }
  }, [
    activateWorkspace,
    canCreateWorkbenchSession,
    legacyNavigationEnabled,
    openScene,
    setActiveWorkspace,
    setSessionMode,
    switchLeftPanelTab,
    workspace,
  ]);

  const handleCopyWorkspacePath = useCallback(async () => {
    setMenuOpen(false);
    try {
      await systemAPI.setClipboard(workspace.rootPath);
      notificationService.success(t('contextMenu.status.copyPathSuccess'), { duration: 2200 });
    } catch (error) {
      notificationService.error(
        error instanceof Error ? error.message : t('errors:contextMenu.copyPathFailed'),
        { duration: 4000 }
      );
    }
  }, [t, workspace.rootPath]);

  const handleReveal = useCallback(async () => {
    setMenuOpen(false);
    try {
      await systemAPI.showInFolder(workspace.rootPath);
    } catch (error) {
      notificationService.error(
        error instanceof Error ? error.message : t('nav.workspaces.actions.reveal'),
        { duration: 4000 }
      );
    }
  }, [t, workspace.rootPath]);

  const handleCloseWorkspace = useCallback(async () => {
    setMenuOpen(false);
    try {
      await closeWorkspaceById(workspace.id);
    } catch (error) {
      notificationService.error(
        error instanceof Error ? error.message : t('nav.workspaces.closeFailed'),
        { duration: 4000 }
      );
    }
  }, [closeWorkspaceById, t, workspace.id]);

  const validateWorkspaceName = useCallback((value: string): string | null => {
    const normalizedName = value.trim();
    if (!normalizedName) {
      return t('nav.workspaces.renameDialog.validation.required');
    }
    if (containsWorkspaceNameControlCharacter(normalizedName)) {
      return t('nav.workspaces.renameDialog.validation.invalidCharacters');
    }
    if (Array.from(normalizedName).length > MAX_WORKSPACE_NAME_CHARS) {
      return t('nav.workspaces.renameDialog.validation.tooLong', {
        max: MAX_WORKSPACE_NAME_CHARS,
      });
    }
    return null;
  }, [t]);

  const handleRenameWorkspace = useCallback(async (name: string) => {
    const normalizedName = name.trim();
    if (normalizedName === workspace.name) {
      return;
    }
    try {
      await renameWorkspace(workspace.id, normalizedName);
      notificationService.success(t('nav.workspaces.renamed'), { duration: 2500 });
    } catch (error) {
      notificationService.error(
        error instanceof Error ? error.message : t('nav.workspaces.renameFailed'),
        { duration: 4000 }
      );
    }
  }, [renameWorkspace, t, workspace.id, workspace.name]);

  return (
    <div
      className={`halo-nav-panel__workspace-item${isActive ? ' is-active' : ''}${isDragging ? ' is-dragging' : ''}`}
      data-testid="nav-workspace-item"
      data-workspace-id={workspace.id}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
    >
      <div
        className="halo-nav-panel__workspace-item-card"
        role="button"
        tabIndex={0}
        onClick={() => { void activateWorkspace(); }}
        onKeyDown={(event) => {
          if (event.key === 'Enter' || event.key === ' ') {
            event.preventDefault();
            void activateWorkspace();
          }
        }}
      >
        <div className="halo-nav-panel__workspace-item-main">
          <FolderOpen size={15} aria-hidden="true" />
          <div className="halo-nav-panel__workspace-item-name-cluster">
            <div className="halo-nav-panel__workspace-item-name-stack">
              <div className="halo-nav-panel__workspace-item-name-row">
                <Tooltip content={workspace.rootPath} placement="right" followCursor>
                  <button
                    type="button"
                    className="halo-nav-panel__workspace-item-name-btn"
                    onClick={event => {
                      event.stopPropagation();
                      void handleCardNameClick();
                    }}
                    data-testid="nav-workspace-name-btn"
                    data-workspace-id={workspace.id}
                  >
                    <span className="halo-nav-panel__workspace-item-name-line">
                      <span className="halo-nav-panel__workspace-item-label">{workspace.name}</span>
                    </span>
                  </button>
                </Tooltip>
              </div>
            </div>
          </div>
        </div>

        <div className="halo-nav-panel__workspace-item-actions" onClick={event => event.stopPropagation()}>
          <div className="halo-nav-panel__workspace-item-menu">
            <Tooltip content={t('nav.items.project')} placement="right" followCursor>
              <button
                type="button"
                className="halo-nav-panel__workspace-item-menu-trigger"
                onClick={() => { void handleOpenFiles(); }}
                data-testid="nav-workspace-files-btn"
                data-workspace-id={workspace.id}
              >
                <Folder size={13} />
              </button>
            </Tooltip>
            <button
              type="button"
              className={`halo-nav-panel__workspace-item-menu-trigger${menuOpen ? ' is-open' : ''}`}
              onClick={() => setMenuOpen(open => !open)}
              data-testid="nav-workspace-menu-btn"
              data-workspace-id={workspace.id}
              aria-expanded={menuOpen}
            >
              <MoreHorizontal size={13} />
            </button>

            {menuOpen && (
              <div
                className="halo-nav-panel__workspace-item-menu-popover"
                role="menu"
                data-testid="nav-workspace-item-menu"
                data-workspace-id={workspace.id}
              >
                <button
                  type="button"
                  className="halo-nav-panel__workspace-item-menu-item"
                  onClick={() => { void handleCreateCodeSession(); }}
                  disabled={!legacyNavigationEnabled && !canOpenWorkbenchSession}
                  data-testid="nav-workspace-menu-create-code-session"
                >
                  <Plus size={13} />
                  <span className="halo-nav-panel__workspace-item-menu-label">{t('shared:agents.code')}</span>
                </button>
                <button
                  type="button"
                  className="halo-nav-panel__workspace-item-menu-item"
                  onClick={() => {
                    setMenuOpen(false);
                    setRenameDialogOpen(true);
                  }}
                  data-testid="nav-workspace-menu-rename"
                >
                  <Pencil size={13} />
                  <span className="halo-nav-panel__workspace-item-menu-label">
                    {t('nav.workspaces.actions.rename')}
                  </span>
                </button>
                <button
                  type="button"
                  className="halo-nav-panel__workspace-item-menu-item"
                  onClick={() => { void handleCopyWorkspacePath(); }}
                  data-testid="nav-workspace-menu-copy-path"
                >
                  <Copy size={13} />
                  <span className="halo-nav-panel__workspace-item-menu-label">{t('nav.workspaces.actions.copyPath')}</span>
                </button>
                <button
                  type="button"
                  className="halo-nav-panel__workspace-item-menu-item"
                  onClick={() => { void handleReveal(); }}
                  data-testid="nav-workspace-menu-reveal"
                >
                  <FolderSearch size={13} />
                  <span className="halo-nav-panel__workspace-item-menu-label">{t('nav.workspaces.actions.reveal')}</span>
                </button>
                <div className="halo-nav-panel__workspace-item-menu-divider" />
                <button
                  type="button"
                  className="halo-nav-panel__workspace-item-menu-item is-danger"
                  onClick={() => { void handleCloseWorkspace(); }}
                  data-testid="nav-workspace-menu-close"
                >
                  <FolderOpen size={13} />
                  <span className="halo-nav-panel__workspace-item-menu-label">{t('nav.workspaces.actions.close')}</span>
                </button>
              </div>
            )}
          </div>
        </div>
      </div>

      <div
        className={`halo-nav-panel__workspace-item-sessions${sessionsCollapsed ? ' is-collapsed' : ''}`}
        data-testid="nav-workspace-session-region"
        data-workspace-id={workspace.id}
      >
        {legacyNavigationEnabled ? (
          <Suspense fallback={null}>
            <LegacySessionsSection
              workspaceId={workspace.id}
              workspacePath={workspace.rootPath}
              remoteConnectionId={null}
              remoteSshHost={null}
              isActiveWorkspace={isActive}
              isVisible={!sessionsCollapsed}
            />
          </Suspense>
        ) : (
          <WorkbenchSessionsSection
            workspaceId={workspace.id}
            isActiveWorkspace={isActive}
          />
        )}
      </div>

      <InputDialog
        isOpen={renameDialogOpen}
        onClose={() => setRenameDialogOpen(false)}
        onConfirm={(name) => { void handleRenameWorkspace(name); }}
        title={t('nav.workspaces.renameDialog.title')}
        description={t('nav.workspaces.renameDialog.description')}
        placeholder={t('nav.workspaces.renameDialog.placeholder')}
        defaultValue={workspace.name}
        confirmText={t('actions.save')}
        cancelText={t('actions.cancel')}
        validator={validateWorkspaceName}
        required={false}
      />
    </div>
  );
};

export default WorkspaceItem;
