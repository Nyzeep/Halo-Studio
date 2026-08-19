 

export { haloDarkTheme } from './dark-theme';
export { haloLightTheme } from './light-theme';
export { haloMidnightTheme } from './midnight-theme';
export { haloChinaStyleTheme } from './china-style-theme';
export { haloChinaNightTheme } from './china-night-theme';
export { haloCyberTheme } from './cyber-theme';
export { haloSlateTheme } from './slate-theme';
export { haloTokyoNightTheme } from './tokyo-night-theme';

import { haloDarkTheme } from './dark-theme';
import { haloLightTheme } from './light-theme';
import { haloMidnightTheme } from './midnight-theme';
import { haloChinaStyleTheme } from './china-style-theme';
import { haloChinaNightTheme } from './china-night-theme';
import { haloCyberTheme } from './cyber-theme';
import { haloSlateTheme } from './slate-theme';
import { haloTokyoNightTheme } from './tokyo-night-theme';
import { ThemeConfig, ThemeId } from '../types';

/** Default light / dark builtin themes used when following system appearance. */
export const DEFAULT_LIGHT_THEME_ID: ThemeId = 'halo-light';
export const DEFAULT_DARK_THEME_ID: ThemeId = 'halo-dark';

/**
 * Picks halo-dark vs halo-light from `prefers-color-scheme`.
 * Used when the user has no saved theme preference.
 */
export function getSystemPreferredDefaultThemeId(): ThemeId {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return DEFAULT_LIGHT_THEME_ID;
  }
  return window.matchMedia('(prefers-color-scheme: dark)').matches
    ? DEFAULT_DARK_THEME_ID
    : DEFAULT_LIGHT_THEME_ID;
}

/** Static fallback when system preference is unavailable (e.g. SSR). */
export const DEFAULT_THEME_ID: ThemeId = DEFAULT_LIGHT_THEME_ID;

 
export const builtinThemes: ThemeConfig[] = [
  haloLightTheme,
  haloSlateTheme,
  haloDarkTheme,
  haloMidnightTheme,
  haloChinaStyleTheme,
  haloChinaNightTheme,
  haloCyberTheme,
  haloTokyoNightTheme,
];

 



