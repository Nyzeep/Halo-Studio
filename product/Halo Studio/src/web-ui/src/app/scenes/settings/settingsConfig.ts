/**
 * settingsConfig - Halo local-coding settings categories and tabs.
 *
 * Shared by SettingsNav and SettingsScene. The broader Halo settings modules
 * remain in the source tree, but Halo only assembles the local coding surface.
 */

export type ConfigTab =
  | 'basics'
  | 'appearance'
  | 'models'
  | 'worktrees'
  | 'archived-sessions'
  | 'session-personalization'
  | 'session-permissions'
  | 'quick-actions'
  | 'voice-input'
  | 'review'
  | 'memories'
  | 'mcp-tools'
  | 'external-sources'
  | 'hooks'
  | 'acp-agents'
  | 'editor'
  | 'keyboard';

export interface ConfigTabDef {
  id: ConfigTab;
  labelKey: string;
  /** i18n key under settings namespace for tab description (search + discoverability). */
  descriptionKey?: string;
  /** Language-neutral extra tokens matched by search (ASCII recommended). */
  keywords?: string[];
  /** Show a Beta pill next to the tab label in the settings nav. */
  beta?: boolean;
}

export interface ConfigCategoryDef {
  id: string;
  nameKey: string;
  tabs: ConfigTabDef[];
}

export const SETTINGS_CATEGORIES: ConfigCategoryDef[] = [
  {
    id: 'general',
    nameKey: 'configCenter.categories.general',
    tabs: [
      {
        id: 'basics',
        labelKey: 'configCenter.tabs.basics',
        descriptionKey: 'configCenter.tabDescriptions.basics',
        keywords: [
          'logging',
          'log',
          'terminal',
          'shell',
          'pwsh',
          'powershell',
          'autostart',
          'launch',
          'notification',
          'notifications',
          'startup tips',
        ],
      },
      {
        id: 'appearance',
        labelKey: 'configCenter.tabs.appearance',
        descriptionKey: 'configCenter.tabDescriptions.appearance',
        keywords: [
          'language',
          'locale',
          'i18n',
          'theme',
          'appearance',
          'font',
          'fonts',
          'typography',
          'size',
        ],
      },
      {
        id: 'archived-sessions',
        labelKey: 'configCenter.tabs.archivedSessions',
        descriptionKey: 'configCenter.tabDescriptions.archivedSessions',
        keywords: [
          'archive',
          'archived',
          'session',
          'sessions',
          'restore',
          'unarchive',
        ],
      },
      {
        id: 'keyboard',
        labelKey: 'configCenter.tabs.keyboard',
        descriptionKey: 'configCenter.tabDescriptions.keyboard',
        keywords: [
          'keyboard',
          'shortcut',
          'keybinding',
          'hotkey',
          'shortcut key',
        ],
      },
    ],
  },
  {
    id: 'devkit',
    nameKey: 'configCenter.categories.devkit',
    tabs: [
      {
        id: 'editor',
        labelKey: 'configCenter.tabs.editor',
        descriptionKey: 'configCenter.tabDescriptions.editor',
        keywords: [
          'font',
          'indent',
          'tab',
          'minimap',
          'word wrap',
          'line number',
          'format',
          'save',
        ],
      },
    ],
  },
];

export const DEFAULT_SETTINGS_TAB: ConfigTab = 'basics';

const KNOWN_TABS: ConfigTab[] = SETTINGS_CATEGORIES.flatMap((c) => c.tabs.map((t) => t.id));

/** Map removed or renamed tabs; used by deep links and IDE actions. */
export function normalizeSettingsTab(section: string): ConfigTab {
  if (section === 'theme' || section === 'font' || section === 'fonts') return 'appearance';
  if (section === 'logging' || section === 'terminal') return 'basics';
  if (section === 'lsp') return DEFAULT_SETTINGS_TAB;
  if (section === 'session-config') return DEFAULT_SETTINGS_TAB;
  if (section === 'shortcuts' || section === 'keybindings' || section === 'hotkeys') return 'keyboard';
  if ((KNOWN_TABS as readonly string[]).includes(section)) return section as ConfigTab;
  return DEFAULT_SETTINGS_TAB;
}
