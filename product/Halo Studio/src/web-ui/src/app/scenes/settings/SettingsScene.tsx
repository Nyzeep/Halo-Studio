/**
 * SettingsScene — content-only renderer for the Settings scene.
 *
 * The left-side navigation lives in SettingsNav (rendered by NavPanel via
 * nav-registry). This component only renders the active config content panel
 * driven by settingsStore.activeTab.
 */

import React, {
  Suspense,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from 'react';
import { useSettingsStore } from './settingsStore';
import { DEFAULT_SETTINGS_TAB } from './settingsConfig';
import type { ConfigTab } from './settingsConfig';
import {
  AppearanceConfig,
  ArchivedSessionsConfig,
  BasicsConfig,
  EditorConfig,
  KeyboardShortcutsTab,
} from './settingsContentRegistry';
import './SettingsScene.scss';

// Keep in sync with settings-content-exit in SettingsScene.scss.
const SETTINGS_CONTENT_EXIT_DURATION_MS = 180;

function SettingsSceneLoading() {
  return (
    <div className="halo-settings-scene__loading" aria-busy="true" aria-hidden="true">
      <div className="halo-settings-scene__loading-line halo-settings-scene__loading-line--title" />
      <div className="halo-settings-scene__loading-line" />
      <div className="halo-settings-scene__loading-line" />
      <div className="halo-settings-scene__loading-block" />
    </div>
  );
}

function resolveSettingsContent(tab: ConfigTab): React.ComponentType | null {
  switch (tab) {
    case 'basics':                  return BasicsConfig;
    case 'appearance':              return AppearanceConfig;
    case 'archived-sessions':       return ArchivedSessionsConfig;
    case 'editor':                  return EditorConfig;
    case 'keyboard':                return KeyboardShortcutsTab;
    default:                        return null;
  }
}

const SettingsScene: React.FC = () => {
  const activeTab = useSettingsStore(s => s.activeTab);
  const setActiveTab = useSettingsStore(s => s.setActiveTab);

  const resolvedTab: ConfigTab =
    (activeTab as string) === 'session-config' ? DEFAULT_SETTINGS_TAB : activeTab;
  const [outgoingTab, setOutgoingTab] = useState<ConfigTab | null>(null);
  const previousTabRef = useRef<ConfigTab>(resolvedTab);

  useEffect(() => {
    if ((activeTab as string) === 'session-config') {
      setActiveTab(DEFAULT_SETTINGS_TAB);
    }
  }, [activeTab, setActiveTab]);

  // Derive the previous tab during render so React keeps its keyed subtree
  // mounted in the same commit that introduces the incoming page.
  const renderedOutgoingTab = previousTabRef.current !== resolvedTab
    ? previousTabRef.current
    : outgoingTab;

  useLayoutEffect(() => {
    const previousTab = previousTabRef.current;
    previousTabRef.current = resolvedTab;
    if (previousTab === resolvedTab) return;

    setOutgoingTab(previousTab);
    const exitTimer = window.setTimeout(() => {
      setOutgoingTab(current => current === previousTab ? null : current);
    }, SETTINGS_CONTENT_EXIT_DURATION_MS);

    return () => window.clearTimeout(exitTimer);
  }, [resolvedTab]);

  const renderedTabs: ConfigTab[] = [resolvedTab];
  if (renderedOutgoingTab && renderedOutgoingTab !== resolvedTab) {
    renderedTabs.push(renderedOutgoingTab);
  }

  return (
    <div className="halo-settings-scene" data-testid="settings-scene" data-settings-tab={resolvedTab}>
      <div className="halo-settings-scene__content-stack">
        {renderedTabs.map(tab => {
          const Content = resolveSettingsContent(tab);
          if (!Content) return null;

          const isActive = tab === resolvedTab;
          const isOutgoing = !isActive && tab === renderedOutgoingTab;
          return (
            <div
              key={tab}
              className={[
                'halo-settings-scene__content-wrapper',
                isActive && 'halo-settings-scene__content-wrapper--active',
                isActive && renderedOutgoingTab && 'halo-settings-scene__content-wrapper--entering',
                isOutgoing && 'halo-settings-scene__content-wrapper--outgoing',
              ].filter(Boolean).join(' ')}
              aria-hidden={!isActive}
              data-testid="settings-scene-content"
              data-settings-panel={tab}
              data-settings-panel-active={isActive ? 'true' : 'false'}
            >
              <Suspense fallback={isActive ? <SettingsSceneLoading /> : null}>
                <Content />
              </Suspense>
            </div>
          );
        })}
      </div>
    </div>
  );
};

export default SettingsScene;
