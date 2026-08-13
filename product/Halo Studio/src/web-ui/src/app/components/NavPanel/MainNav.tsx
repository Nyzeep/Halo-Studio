/**
 * MainNav - Halo local-coding navigation sidebar.
 */

import React, { lazy, Suspense, useCallback, useState, useMemo, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { Plus, FolderOpen, FolderPlus, History, Check, Search, FileCode2, GitBranch } from 'lucide-react';
import { Tooltip } from '@/component-library';
import { useApp } from '../../hooks/useApp';
import { useSceneManager } from '../../hooks/useSceneManager';
import { useI18n } from '@/infrastructure/i18n/hooks/useI18n';
import type { SceneTabId } from '../SceneBar/types';
import SectionHeader from './components/SectionHeader';
import WorkspaceListSection from './sections/workspaces/WorkspaceListSection';
import WorkbenchSessionsSection from './sections/sessions/WorkbenchSessionsSection';
import { useSessionModeStore } from '../../stores/sessionModeStore';
import { useSceneStore } from '../../stores/sceneStore';
import { workspaceManager } from '@/infrastructure/services/business/workspaceManager';
import { useWorkspaceContext } from '@/infrastructure/contexts/WorkspaceContext';
import { createLogger } from '@/shared/utils/logger';
import { notificationService } from '@/shared/notification-system';
import { isRemoteWorkspace } from '@/shared/types';
import { isHaloLocalCodingScope } from '@/infrastructure/runtime';
import { getRecentWorkspaceLineParts } from '@/shared/utils/recentWorkspaceDisplay';
import { computeFixedPopoverPosition } from '@/shared/utils/fixedPopoverViewport';
import { useShortcut } from '@/infrastructure/hooks/useShortcut';
import { ALL_SHORTCUTS } from '@/shared/constants/shortcuts';
import { useStore } from 'zustand';
import {
  createWorkbenchRuntimeRequestId,
  workbenchRuntimeStore,
} from '@/infrastructure/workbench-runtime';

import './NavPanel.scss';

const NavSearchDialog = lazy(() => import('./NavSearchDialog'));
const LegacySessionsSection = lazy(() => import('./sections/sessions/SessionsSection'));
const NAV_TOGGLE_SEARCH_DEF = ALL_SHORTCUTS.find((d) => d.id === 'nav.toggleSearch')!;

const log = createLogger('MainNav');

interface MainNavProps {
  isDeparting?: boolean;
  anchorNavSceneId?: SceneTabId | null;
}

const MainNav: React.FC<MainNavProps> = ({
  isDeparting: _isDeparting = false,
  anchorNavSceneId: _anchorNavSceneId = null,
}) => {
  const { switchLeftPanelTab } = useApp();
  const { openScene } = useSceneManager();
  const activeTabId = useSceneStore(s => s.activeTabId);
  const { t } = useI18n('common');
  const {
    currentWorkspace,
    loading: workspaceLoading,
    recentWorkspaces,
    normalWorkspacesList,
    openWorkspace,
    switchWorkspace,
    setActiveWorkspace,
  } = useWorkspaceContext();

  const [expandedSections, setExpandedSections] = useState<Set<string>>(
    () => new Set(['sessions', 'workspace'])
  );

  const workspaceMenuButtonRef = useRef<HTMLButtonElement | null>(null);
  const workspaceMenuRef = useRef<HTMLDivElement | null>(null);
  const [workspaceMenuOpen, setWorkspaceMenuOpen] = useState(false);
  const [workspaceMenuClosing, setWorkspaceMenuClosing] = useState(false);
  const [workspaceMenuPos, setWorkspaceMenuPos] = useState({ top: 0, left: 0 });
  const [searchOpen, setSearchOpen] = useState(false);
  const setSessionMode = useSessionModeStore(s => s.setMode);
  const runtimeSnapshot = useStore(workbenchRuntimeStore, state => state.snapshot);
  const legacyNavigationEnabled = !isHaloLocalCodingScope();

  const localProjectWorkspaces = useMemo(
    () => normalWorkspacesList.filter(workspace => !isRemoteWorkspace(workspace)),
    [normalWorkspacesList]
  );
  const localRecentWorkspaces = useMemo(
    () => recentWorkspaces.filter(workspace => !isRemoteWorkspace(workspace)),
    [recentWorkspaces]
  );
  const sessionTargetWorkspace = useMemo(
    () => localProjectWorkspaces.find(workspace => workspace.id === currentWorkspace?.id)
      ?? localProjectWorkspaces[0],
    [currentWorkspace?.id, localProjectWorkspaces],
  );
  const canCreateWorkbenchSession = runtimeSnapshot?.phase === 'ready'
    && runtimeSnapshot.workspace?.workspaceId === sessionTargetWorkspace?.id;

  const toggleSection = useCallback((id: string) => {
    setExpandedSections(prev => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }, []);

  const closeWorkspaceMenu = useCallback(() => {
    setWorkspaceMenuClosing(true);
    window.setTimeout(() => {
      setWorkspaceMenuOpen(false);
      setWorkspaceMenuClosing(false);
    }, 150);
  }, []);

  const updateWorkspaceMenuPos = useCallback(() => {
    const btn = workspaceMenuButtonRef.current;
    if (!btn || !workspaceMenuOpen) return;
    const rect = btn.getBoundingClientRect();
    const viewportPadding = 8;
    const gap = 6;
    const fallbackWidth = 300;
    const fallbackHeight = 420;

    const apply = () => {
      const menuEl = workspaceMenuRef.current;
      const w = menuEl?.offsetWidth ?? fallbackWidth;
      const h = menuEl?.offsetHeight ?? fallbackHeight;
      setWorkspaceMenuPos(computeFixedPopoverPosition(rect, w, h, gap, viewportPadding));
    };

    apply();
    requestAnimationFrame(apply);
  }, [workspaceMenuOpen]);

  const openWorkspaceMenu = useCallback(async () => {
    try {
      await workspaceManager.cleanupInvalidWorkspaces();
    } catch (error) {
      log.warn('Failed to cleanup invalid workspaces before opening workspace menu', { error });
    }
    const rect = workspaceMenuButtonRef.current?.getBoundingClientRect();
    if (!rect) return;
    setWorkspaceMenuPos(computeFixedPopoverPosition(rect, 300, 420, 6, 8));
    setWorkspaceMenuOpen(true);
    setWorkspaceMenuClosing(false);
  }, []);

  const toggleWorkspaceMenu = useCallback(() => {
    if (workspaceMenuOpen) {
      closeWorkspaceMenu();
      return;
    }
    void openWorkspaceMenu();
  }, [closeWorkspaceMenu, openWorkspaceMenu, workspaceMenuOpen]);

  const toggleNavSearch = useCallback(() => {
    if (!legacyNavigationEnabled) return;
    setSearchOpen((v) => !v);
  }, [legacyNavigationEnabled]);

  useShortcut(
    NAV_TOGGLE_SEARCH_DEF.id,
    NAV_TOGGLE_SEARCH_DEF.config,
    toggleNavSearch,
    { priority: 5, description: NAV_TOGGLE_SEARCH_DEF.descriptionKey }
  );

  useEffect(() => {
    if (!legacyNavigationEnabled) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      if (
        !e.altKey ||
        e.ctrlKey ||
        e.metaKey ||
        e.shiftKey ||
        e.key.toLowerCase() !== 'f'
      ) {
        return;
      }
      e.preventDefault();
      toggleNavSearch();
    };
    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [legacyNavigationEnabled, toggleNavSearch]);

  const handleCreateCodeSession = useCallback(async () => {
    const target = sessionTargetWorkspace;
    if (!target) {
      notificationService.warning(t('nav.sessions.needProjectWorkspaceForSession'), { duration: 4500 });
      return;
    }
    try {
      if (legacyNavigationEnabled) {
        const { createLegacyMainNavCodeSession } = await import('./legacyCodeSession');
        await createLegacyMainNavCodeSession({
          target,
          currentWorkspaceId: currentWorkspace?.id,
          setActiveWorkspace,
          setSessionMode,
          openSessionScene: () => openScene('session'),
          switchToSessions: () => switchLeftPanelTab('sessions'),
        });
        return;
      }

      const snapshot = workbenchRuntimeStore.getState().snapshot;
      if (snapshot?.phase !== 'ready' || snapshot.workspace?.workspaceId !== target.id) return;
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
    currentWorkspace?.id,
    legacyNavigationEnabled,
    openScene,
    sessionTargetWorkspace,
    setActiveWorkspace,
    setSessionMode,
    switchLeftPanelTab,
    t,
  ]);

  const handleOpenFiles = useCallback(() => {
    openScene('file-viewer');
    switchLeftPanelTab('files');
  }, [openScene, switchLeftPanelTab]);

  const handleOpenGit = useCallback(() => {
    openScene('git');
    switchLeftPanelTab('git');
  }, [openScene, switchLeftPanelTab]);

  const handleOpenProject = useCallback(async () => {
    try {
      const { pickWorkspaceDirectory } = await import(
        '@/infrastructure/peer-device/pickWorkspaceDirectory'
      );
      const selected = await pickWorkspaceDirectory({
        title: t('header.selectProjectDirectory'),
      });
      if (selected) {
        await openWorkspace(selected);
      }
    } catch (err) {
      log.error('Failed to open project', { error: err });
    }
  }, [openWorkspace, t]);

  const handleNewProject = useCallback(() => {
    window.dispatchEvent(new Event('nav:new-project'));
  }, []);

  const handleSwitchWorkspace = useCallback(async (workspaceId: string) => {
    const targetWorkspace = localRecentWorkspaces.find(item => item.id === workspaceId);
    if (!targetWorkspace) return;
    closeWorkspaceMenu();
    await switchWorkspace(targetWorkspace);
  }, [closeWorkspaceMenu, localRecentWorkspaces, switchWorkspace]);

  useEffect(() => {
    if (!workspaceMenuOpen) return;
    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target) return;
      if (workspaceMenuButtonRef.current?.contains(target)) return;
      if (workspaceMenuRef.current?.contains(target)) return;
      closeWorkspaceMenu();
    };
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key === 'Escape') closeWorkspaceMenu();
    };
    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleEscape);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      document.removeEventListener('keydown', handleEscape);
    };
  }, [closeWorkspaceMenu, workspaceMenuOpen]);

  useEffect(() => {
    if (!workspaceMenuOpen) return;

    updateWorkspaceMenuPos();

    const handleViewportChange = () => updateWorkspaceMenuPos();
    window.addEventListener('resize', handleViewportChange);
    window.addEventListener('scroll', handleViewportChange, true);

    return () => {
      window.removeEventListener('resize', handleViewportChange);
      window.removeEventListener('scroll', handleViewportChange, true);
    };
  }, [workspaceMenuOpen, updateWorkspaceMenuPos]);

  const workspaceMenuPortal = workspaceMenuOpen ? createPortal(
    <div
      ref={workspaceMenuRef}
      className={`bitfun-nav-panel__workspace-menu${workspaceMenuClosing ? ' is-closing' : ''}`}
      role="menu"
      style={{ top: workspaceMenuPos.top, left: workspaceMenuPos.left }}
    >
      <button
        type="button"
        className="bitfun-nav-panel__workspace-menu-item"
        role="menuitem"
        onClick={() => { closeWorkspaceMenu(); void handleOpenProject(); }}
      >
        <FolderOpen size={13} />
        <span>{t('header.openProject')}</span>
      </button>
      <button
        type="button"
        className="bitfun-nav-panel__workspace-menu-item"
        role="menuitem"
        onClick={() => { closeWorkspaceMenu(); handleNewProject(); }}
      >
        <FolderPlus size={13} />
        <span>{t('header.newProject')}</span>
      </button>
      <div className="bitfun-nav-panel__workspace-menu-divider" role="separator" />
      <div className="bitfun-nav-panel__workspace-menu-section-title">
        <History size={12} aria-hidden="true" />
        <span>{t('header.recentWorkspaces')}</span>
      </div>
      {localRecentWorkspaces.length === 0 ? (
        <div className="bitfun-nav-panel__workspace-menu-empty">
          <span>{t('header.noRecentWorkspaces')}</span>
        </div>
      ) : (
        <div className="bitfun-nav-panel__workspace-menu-workspaces">
          {localRecentWorkspaces.map((workspace) => {
            const { hostPrefix, folderLabel, tooltip } = getRecentWorkspaceLineParts(workspace);
            return (
              <button
                key={workspace.id}
                type="button"
                className="bitfun-nav-panel__workspace-menu-item bitfun-nav-panel__workspace-menu-item--workspace"
                role="menuitem"
                title={tooltip}
                onClick={() => { void handleSwitchWorkspace(workspace.id); }}
                data-testid="nav-workspace-menu-recent-workspace"
                data-workspace-id={workspace.id}
              >
                <FolderOpen size={13} aria-hidden="true" />
                <span className="bitfun-nav-panel__workspace-menu-item-main">
                  {hostPrefix ? (
                    <>
                      <span className="bitfun-nav-panel__workspace-menu-item-host">{hostPrefix}</span>
                      <span className="bitfun-nav-panel__workspace-menu-item-host-sep" aria-hidden>
                        {' / '}
                      </span>
                    </>
                  ) : null}
                  <span className="bitfun-nav-panel__workspace-menu-item-name">{folderLabel}</span>
                </span>
                {workspace.id === currentWorkspace?.id ? <Check size={12} aria-hidden="true" /> : null}
              </button>
            );
          })}
        </div>
      )}
    </div>,
    document.body
  ) : null;

  const createCodeTooltip = t('nav.sessions.newCodeSession');
  const addWorkspaceTooltip = t('nav.tooltips.addWorkspace');

  return (
    <>
      {legacyNavigationEnabled ? (
        <div className="bitfun-nav-panel__brand-header">
          <div className="bitfun-nav-panel__brand-search">
            <Tooltip content={t('nav.search.triggerTooltip')} placement="right" followCursor>
              <button
                type="button"
                className="bitfun-nav-panel__search-trigger"
                onClick={() => setSearchOpen(true)}
                aria-label={t('nav.search.triggerTooltip')}
                data-testid="nav-search-trigger"
              >
                <span className="bitfun-nav-panel__search-trigger__icon" aria-hidden="true">
                  <span className="bitfun-nav-panel__search-trigger__icon-inner">
                    <Search size={13} />
                  </span>
                </span>
                <span className="bitfun-nav-panel__search-trigger__label">
                  {t('nav.search.triggerPlaceholder')}
                </span>
              </button>
            </Tooltip>
            <Suspense fallback={null}>
              <NavSearchDialog open={searchOpen} onClose={() => setSearchOpen(false)} />
            </Suspense>
          </div>
        </div>
      ) : null}

      <div className="bitfun-nav-panel__top-actions">
        <Tooltip content={createCodeTooltip} placement="right" followCursor>
          <button
            type="button"
            className="bitfun-nav-panel__top-action-btn"
            onClick={() => { void handleCreateCodeSession(); }}
            disabled={!legacyNavigationEnabled && !canCreateWorkbenchSession}
            aria-label={createCodeTooltip}
            data-testid="nav-new-code-session-btn"
          >
            <span className="bitfun-nav-panel__top-action-icon-circle" aria-hidden="true">
              <Plus size={12} />
            </span>
            <span>{t('shared:agents.code')}</span>
          </button>
        </Tooltip>

        <Tooltip content={t('shared:features.files')} placement="right" followCursor>
          <button
            type="button"
            className={`bitfun-nav-panel__top-action-btn${activeTabId === 'file-viewer' ? ' is-active' : ''}`}
            onClick={handleOpenFiles}
            aria-label={t('shared:features.files')}
            data-testid="nav-file-viewer-btn"
          >
            <span className="bitfun-nav-panel__top-action-icon-slot" aria-hidden="true">
              <FileCode2 size={15} />
            </span>
            <span>{t('shared:features.files')}</span>
          </button>
        </Tooltip>

        <Tooltip content="Git" placement="right" followCursor>
          <button
            type="button"
            className={`bitfun-nav-panel__top-action-btn${activeTabId === 'git' ? ' is-active' : ''}`}
            onClick={handleOpenGit}
            aria-label="Git"
            data-testid="nav-git-btn"
          >
            <span className="bitfun-nav-panel__top-action-icon-slot" aria-hidden="true">
              <GitBranch size={15} />
            </span>
            <span>Git</span>
          </button>
        </Tooltip>
      </div>

      <div className="bitfun-nav-panel__sections" data-testid="nav-sections">
        <div className="bitfun-nav-panel__section">
          <SectionHeader
            label={t('nav.items.sessions')}
            collapsible
            isOpen={expandedSections.has('sessions')}
            onToggle={() => toggleSection('sessions')}
          />
          <div className={`bitfun-nav-panel__collapsible${expandedSections.has('sessions') ? '' : ' is-collapsed'}`}>
            <div className="bitfun-nav-panel__collapsible-inner">
              <div className="bitfun-nav-panel__items bitfun-nav-panel__items--session-blocks">
                {localProjectWorkspaces.map(workspace => (
                  legacyNavigationEnabled ? (
                    <Suspense key={workspace.id} fallback={null}>
                      <LegacySessionsSection
                        key={workspace.id}
                        workspaceId={workspace.id}
                        workspacePath={workspace.rootPath}
                        remoteConnectionId={null}
                        isActiveWorkspace={workspace.id === currentWorkspace?.id}
                        isVisible={expandedSections.has('sessions') && !workspaceLoading}
                      />
                    </Suspense>
                  ) : (
                    <WorkbenchSessionsSection
                      key={workspace.id}
                      workspaceId={workspace.id}
                      isActiveWorkspace={workspace.id === currentWorkspace?.id}
                    />
                  )
                ))}
              </div>
            </div>
          </div>
        </div>

        <div className="bitfun-nav-panel__section">
          <SectionHeader
            label={t('shared:features.workspace')}
            collapsible
            isOpen={expandedSections.has('workspace')}
            onToggle={() => toggleSection('workspace')}
            actions={
              <div className="bitfun-nav-panel__workspace-action-wrap">
                <Tooltip content={addWorkspaceTooltip} placement="right" followCursor disabled={workspaceMenuOpen}>
                  <button
                    ref={workspaceMenuButtonRef}
                    type="button"
                    className={`bitfun-nav-panel__section-action${workspaceMenuOpen ? ' is-active' : ''}`}
                    aria-label={addWorkspaceTooltip}
                    aria-expanded={workspaceMenuOpen}
                    onClick={toggleWorkspaceMenu}
                    data-testid="nav-workspace-add-btn"
                  >
                    <Plus size={13} />
                  </button>
                </Tooltip>
              </div>
            }
          />
          <div className={`bitfun-nav-panel__collapsible${expandedSections.has('workspace') ? '' : ' is-collapsed'}`}>
            <div className="bitfun-nav-panel__collapsible-inner">
              <div className="bitfun-nav-panel__items">
                <WorkspaceListSection variant="projects" />
              </div>
            </div>
          </div>
        </div>
      </div>

      {workspaceMenuPortal}
    </>
  );
};

export default MainNav;
