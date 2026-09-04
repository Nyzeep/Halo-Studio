/**
 * Token-layer theme utilities (M4, ADR-0077; issue #53).
 *
 * Imperative counterpart to the `[data-theme]` contract in ./tokens.css:
 *   - 'system' removes the attribute so `prefers-color-scheme` decides;
 *   - 'dark' / 'light' force the matching role set via the attribute.
 *
 * The functions are pure DOM utilities over a root element (defaulting to
 * `document.documentElement`), which makes them trivially drivable from a
 * zustand store (see useThemeStore below) and unit-testable with jsdom.
 *
 * Coexistence note: the legacy ThemeService (src/infrastructure/theme) also
 * writes `data-theme` (with preset ids such as `halo-dark`). The token layer
 * only reacts to the literal values 'dark' | 'light'; M5 settings migration
 * will move theme selection onto this module.
 */

export type ThemePreference = 'system' | 'dark' | 'light';

export type ResolvedTheme = 'dark' | 'light';

export const THEME_ATTRIBUTE = 'data-theme';

function resolveRoot(root?: HTMLElement | null): HTMLElement | null {
  if (root) {
    return root;
  }
  return typeof document === 'undefined' ? null : document.documentElement;
}

/** Reads the OS color scheme preference. Falls back to 'dark' when unavailable. */
export function getSystemTheme(targetWindow?: Pick<Window, 'matchMedia'> | null): ResolvedTheme {
  const query = typeof targetWindow !== 'undefined'
    ? targetWindow
    : typeof window === 'undefined'
      ? null
      : window;
  if (!query || typeof query.matchMedia !== 'function') {
    return 'dark';
  }
  try {
    return query.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
  } catch {
    return 'dark';
  }
}

/**
 * Applies a theme preference to the root element and returns the resolved
 * theme ('dark' | 'light'). `system` clears the attribute so the CSS media
 * query provides the default; callers that need to know the effective theme
 * can combine this with {@link getSystemTheme}.
 */
export function applyTheme(
  preference: ThemePreference,
  options?: { root?: HTMLElement | null; window?: Pick<Window, 'matchMedia'> | null },
): ResolvedTheme {
  const root = resolveRoot(options?.root);
  if (!root) {
    return getSystemTheme(options?.window);
  }

  if (preference === 'system') {
    root.removeAttribute(THEME_ATTRIBUTE);
    return getSystemTheme(options?.window);
  }

  root.setAttribute(THEME_ATTRIBUTE, preference);
  return preference;
}

/** Reads the current preference from the root element's `data-theme` attribute. */
export function getThemePreference(root?: HTMLElement | null): ThemePreference {
  const element = resolveRoot(root);
  const value = element?.getAttribute(THEME_ATTRIBUTE);
  if (value === 'dark' || value === 'light') {
    return value;
  }
  return 'system';
}

/**
 * Subscribes to OS color-scheme changes. Returns an unsubscribe function.
 * Useful alongside a zustand store so a `system` preference can re-resolve
 * when the OS flips themes.
 */
export function watchSystemTheme(
  onChanged: (theme: ResolvedTheme) => void,
  targetWindow?: Pick<Window, 'matchMedia'> | null,
): () => void {
  const query = typeof targetWindow !== 'undefined'
    ? targetWindow
    : typeof window === 'undefined'
      ? null
      : window;
  if (!query || typeof query.matchMedia !== 'function') {
    return () => {};
  }

  let media: MediaQueryList;
  try {
    media = query.matchMedia('(prefers-color-scheme: light)');
  } catch {
    return () => {};
  }

  const handler = (event: MediaQueryListEvent) => {
    onChanged(event.matches ? 'light' : 'dark');
  };
  if (typeof media.addEventListener === 'function') {
    media.addEventListener('change', handler);
    return () => media.removeEventListener('change', handler);
  }

  // Legacy engines (Safari < 14) only expose addListener.
  const legacy = media as unknown as {
    addListener?: (listener: (event: MediaQueryListEvent) => void) => void;
    removeListener?: (listener: (event: MediaQueryListEvent) => void) => void;
  };
  if (typeof legacy.addListener === 'function') {
    legacy.addListener(handler);
    return () => legacy.removeListener?.(handler);
  }

  return () => {};
}
