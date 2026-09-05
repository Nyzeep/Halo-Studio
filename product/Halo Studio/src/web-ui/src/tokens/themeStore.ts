/**
 * Zustand store binding for the token-layer theme (M4, ADR-0077; issue #53).
 *
 * Keeps the preference as state and mirrors it onto `document.documentElement`
 * through applyTheme() on every change, so UI components can subscribe with
 * the usual zustand selector ergonomics while the DOM contract stays in one
 * place. OS color-scheme changes re-resolve a 'system' preference.
 */

import { create } from 'zustand';
import {
  applyTheme,
  getSystemTheme,
  getThemePreference,
  watchSystemTheme,
  type ResolvedTheme,
  type ThemePreference,
} from './theme';

interface ThemeState {
  /** User-facing preference; 'system' defers to prefers-color-scheme. */
  preference: ThemePreference;
  /** Effective theme after resolution; refreshed on OS scheme changes. */
  resolved: ResolvedTheme;
  setPreference: (preference: ThemePreference) => void;
  /** Re-applies the stored preference (e.g. after OS scheme flips). */
  syncFromDocument: () => void;
}

function resolve(preference: ThemePreference): ResolvedTheme {
  return preference === 'system' ? getSystemTheme() : preference;
}

export const useThemeStore = create<ThemeState>((set) => ({
  preference: 'system',
  resolved: resolve('system'),
  setPreference: (preference) => {
    const resolved = applyTheme(preference);
    set({ preference, resolved });
  },
  syncFromDocument: () => {
    const preference = getThemePreference();
    const resolved = applyTheme(preference);
    set({ preference, resolved });
  },
}));

/**
 * Installs the OS color-scheme watcher once per app run. Returns an
 * unsubscribe function for tests or hot reloading.
 */
export function bindThemeStoreToSystemTheme(): () => void {
  return watchSystemTheme(() => {
    const { preference, syncFromDocument } = useThemeStore.getState();
    if (preference === 'system') {
      syncFromDocument();
      return;
    }
    // Explicit preferences are unaffected by OS changes; just refresh the
    // cached resolved value.
    useThemeStore.setState({ resolved: resolve(preference) });
  });
}
