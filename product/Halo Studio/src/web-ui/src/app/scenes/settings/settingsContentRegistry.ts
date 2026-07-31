import { lazy } from 'react';
import type { ConfigTab } from './settingsConfig';

const loadEditorConfig = () => import('../../../infrastructure/config/components/EditorConfig');
const loadBasicsConfig = () => import('../../../infrastructure/config/components/BasicsConfig');
const loadAppearanceConfig = () => import('../../../infrastructure/config/components/AppearanceConfig');
const loadArchivedSessionsConfig = () => import('./components/ArchivedSessionsConfig');
const loadKeyboardShortcutsTab = () => import('./components/KeyboardShortcutsTab');

export const EditorConfig = lazy(loadEditorConfig);
export const BasicsConfig = lazy(loadBasicsConfig);
export const AppearanceConfig = lazy(loadAppearanceConfig);
export const ArchivedSessionsConfig = lazy(loadArchivedSessionsConfig);
export const KeyboardShortcutsTab = lazy(loadKeyboardShortcutsTab);

const SETTINGS_CONTENT_LOADERS: Partial<Record<ConfigTab, () => Promise<unknown>>> = {
  basics: loadBasicsConfig,
  appearance: loadAppearanceConfig,
  'archived-sessions': loadArchivedSessionsConfig,
  editor: loadEditorConfig,
  keyboard: loadKeyboardShortcutsTab,
};

export async function preloadSettingsTabContent(tab: ConfigTab): Promise<void> {
  await SETTINGS_CONTENT_LOADERS[tab]?.();
}
