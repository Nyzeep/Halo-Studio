/**
 * i18n keys for in-page section titles/descriptions per Halo settings tab.
 * SettingsNav search reads this map only for tabs assembled in settingsConfig.
 */

export interface SettingsTabSearchPhrase {
  ns: string;
  key: string;
}

export const SETTINGS_TAB_SEARCH_CONTENT: Record<string, readonly SettingsTabSearchPhrase[]> = {
  basics: [
    { ns: 'settings/basics', key: 'title' },
    { ns: 'settings/basics', key: 'subtitle' },
    { ns: 'settings/basics', key: 'logging.sections.logging' },
    { ns: 'settings/basics', key: 'logging.sections.loggingHint' },
    { ns: 'settings/basics', key: 'terminal.sections.terminal' },
    { ns: 'settings/basics', key: 'terminal.sections.terminalHint' },
    { ns: 'settings/basics', key: 'notifications.title' },
    { ns: 'settings/basics', key: 'notifications.hint' },
  ],
  appearance: [
    { ns: 'settings/appearance', key: 'title' },
    { ns: 'settings/appearance', key: 'subtitle' },
    { ns: 'settings/basics', key: 'appearance.title' },
    { ns: 'settings/basics', key: 'appearance.hint' },
    { ns: 'settings/basics', key: 'appearance.fontSize.title' },
    { ns: 'settings/basics', key: 'appearance.fontSize.hint' },
  ],
  'archived-sessions': [
    { ns: 'settings/archived-sessions', key: 'title' },
    { ns: 'settings/archived-sessions', key: 'subtitle' },
    { ns: 'settings/archived-sessions', key: 'empty.title' },
  ],
  editor: [
    { ns: 'settings/editor', key: 'title' },
    { ns: 'settings/editor', key: 'subtitle' },
    { ns: 'settings/editor', key: 'fontFamily.title' },
    { ns: 'settings/editor', key: 'fontSize.title' },
    { ns: 'settings/editor', key: 'wordWrap.title' },
  ],
  keyboard: [
    { ns: 'settings/keyboard-shortcuts', key: 'title' },
    { ns: 'settings/keyboard-shortcuts', key: 'subtitle' },
    { ns: 'settings/keyboard-shortcuts', key: 'search.placeholder' },
  ],
};
