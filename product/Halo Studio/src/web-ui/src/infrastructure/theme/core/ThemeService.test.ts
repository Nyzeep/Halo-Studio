import { JSDOM } from 'jsdom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { configAPI } from '@/infrastructure/api';
import { haloDarkTheme, haloLightTheme } from '../presets';
import {
  PLUGIN_THEME_COLOR_KEYS,
  createPluginThemeColorProjection,
} from '../pluginThemeProjection';
import { SYSTEM_THEME_ID, type ThemeConfig } from '../types';
import { ThemeService } from './ThemeService';

function expectThemeError(
  result: ReturnType<ThemeService['validateTheme']>,
  path: string,
  code: string,
) {
  expect(result.errors).toEqual(expect.arrayContaining([expect.objectContaining({ path, code })]));
}

function expectNoRetiredThemeAuthoringKeys(theme: ThemeConfig) {
  const accentColors = theme.colors.accent as unknown as Record<string, unknown>;
  const backgroundColors = theme.colors.background as unknown as Record<string, unknown>;
  const purpleColors = theme.colors.purple as unknown as Record<string, unknown>;
  const elementColors = theme.colors.element as unknown as Record<string, unknown>;
  const fontWeights = theme.typography.weight as unknown as Record<string, unknown>;
  const components = theme.components as unknown as Record<string, unknown> | undefined;
  expect(accentColors).not.toHaveProperty('800');
  expect(backgroundColors).not.toHaveProperty('quaternary');
  expect(backgroundColors).not.toHaveProperty('tooltip');
  expect(purpleColors).not.toHaveProperty('50');
  expect(purpleColors).not.toHaveProperty('400');
  expect(purpleColors).not.toHaveProperty('800');
  expect(elementColors).not.toHaveProperty('elevated');
  expect(fontWeights).not.toHaveProperty('bold');
  expect(components?.windowControls).toBeUndefined();
}

function createThemeWithRetiredAuthoringKeys(id: string, name: string): ThemeConfig {
  return {
    ...haloDarkTheme,
    id,
    name,
    colors: {
      ...haloDarkTheme.colors,
      accent: {
        ...haloDarkTheme.colors.accent,
        800: '#0f766e',
      },
      background: {
        ...haloDarkTheme.colors.background,
        quaternary: '#252528',
        tooltip: 'rgba(28, 28, 31, 0.96)',
      },
      purple: {
        ...(haloDarkTheme.colors.purple ?? {}),
        50: '#faf5ff',
        400: '#c084fc',
        800: '#6b21a8',
      },
      element: {
        ...haloDarkTheme.colors.element,
        elevated: 'rgba(255, 255, 255, 0.2)',
      },
    },
    typography: {
      ...haloDarkTheme.typography,
      weight: {
        ...haloDarkTheme.typography.weight,
        bold: 700,
      },
    },
    components: {
      ...haloDarkTheme.components,
      windowControls: {
        close: {
          hoverColor: '#a85555',
        },
      },
    },
  } as unknown as ThemeConfig;
}

vi.mock('@/infrastructure/api', () => ({
  configAPI: {
    getConfig: vi.fn(),
    setConfig: vi.fn().mockResolvedValue(undefined),
  },
}));

vi.mock('../integrations/MonacoThemeSync', () => ({
  monacoThemeSync: {
    syncTheme: vi.fn(),
  },
}));

vi.mock('@/shared/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

describe('ThemeService runtime theme tokens', () => {
  let dom: JSDOM;
  const bootstrapGlobals = globalThis as typeof globalThis & {
    __HALO_BOOTSTRAP_THEME_ID__?: string;
    __HALO_BOOTSTRAP_THEME_SELECTION__?: string;
  };

  beforeEach(() => {
    dom = new JSDOM('<!doctype html><html><body></body></html>');
    vi.stubGlobal('window', dom.window);
    vi.stubGlobal('document', dom.window.document);
    Object.defineProperty(dom.window, 'matchMedia', {
      writable: true,
      value: vi.fn().mockReturnValue({
        matches: false,
        addEventListener: vi.fn(),
        removeEventListener: vi.fn(),
      }),
    });
    delete bootstrapGlobals.__HALO_BOOTSTRAP_THEME_ID__;
    delete bootstrapGlobals.__HALO_BOOTSTRAP_THEME_SELECTION__;
    vi.mocked(configAPI.getConfig).mockResolvedValue(undefined);
    vi.mocked(configAPI.setConfig).mockResolvedValue(undefined);
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it('keeps light theme Flow Chat markdown links browser-blue even with a neutral app accent', async () => {
    const service = new ThemeService();

    await service.applyTheme('halo-light');

    const rootStyle = document.documentElement.style;
    expect(rootStyle.getPropertyValue('--color-accent-500')).toBe('#64748b');
    expect(rootStyle.getPropertyValue('--flowchat-link-color')).toBe('#0969da');
    expect(rootStyle.getPropertyValue('--flowchat-link-hover-color')).toBe('#0550ae');
  });

  it('keeps dark neutral-accent themes on an obvious blue link color', async () => {
    const service = new ThemeService();

    await service.applyTheme('halo-slate');

    const rootStyle = document.documentElement.style;
    expect(rootStyle.getPropertyValue('--color-accent-500')).toBe('#94a3b8');
    expect(rootStyle.getPropertyValue('--flowchat-link-color')).toBe('#60a5fa');
    expect(rootStyle.getPropertyValue('--flowchat-link-hover-color')).toBe('#93c5fd');
  });

  it('uses canonical light overlay stops for scrollbar fallback hover', async () => {
    const service = new ThemeService();

    await service.applyTheme('halo-light');

    expect(document.documentElement.style.getPropertyValue('--scrollbar-thumb-hover')).toBe('rgba(0, 0, 0, 0.3)');
  });

  it('does not inject component-private card surface variables into the root theme contract', async () => {
    const service = new ThemeService();

    await service.applyTheme('halo-dark');

    expect(document.documentElement.style.getPropertyValue('--card-bg-default')).toBe('');
    expect(document.documentElement.style.getPropertyValue('--card-bg-hover')).toBe('');
    expect(document.documentElement.style.getPropertyValue('--card-bg-active')).toBe('');
    expect(document.documentElement.style.getPropertyValue('--card-bg-accent')).toBe('');
    expect(document.documentElement.style.getPropertyValue('--card-bg-purple')).toBe('');
  });

  it('keeps dark info border aligned with the canonical medium overlay stop', async () => {
    const service = new ThemeService();

    await service.applyTheme('halo-dark');

    expect(document.documentElement.style.getPropertyValue('--color-info-border')).toBe('rgba(255, 255, 255, 0.24)');
  });

  it('exports the consumed git runtime token family from the resolved theme', async () => {
    const service = new ThemeService();

    await service.applyTheme('halo-dark');

    const rootStyle = document.documentElement.style;
    expect(rootStyle.getPropertyValue('--git-color-branch')).toBe('#a1a1aa');
    expect(rootStyle.getPropertyValue('--git-color-branch-bg')).toBe('rgba(255, 255, 255, 0.06)');
    expect(rootStyle.getPropertyValue('--git-color-branch-bg-hover')).toBe('rgba(255, 255, 255, 0.12)');
    expect(rootStyle.getPropertyValue('--git-color-changes')).toBe('rgb(245, 158, 11)');
    expect(rootStyle.getPropertyValue('--git-color-added')).toBe('rgb(34, 197, 94)');
    expect(rootStyle.getPropertyValue('--git-color-deleted')).toBe('rgb(239, 68, 68)');
    expect(rootStyle.getPropertyValue('--git-color-staged')).toBe('rgb(34, 197, 94)');
    expect(rootStyle.getPropertyValue('--git-color-changes-bg')).toBe('');
    expect(rootStyle.getPropertyValue('--git-color-added-bg')).toBe('');
    expect(rootStyle.getPropertyValue('--git-color-deleted-bg')).toBe('');
    expect(rootStyle.getPropertyValue('--git-color-staged-bg')).toBe('');
    expect(rootStyle.getPropertyValue('--git-color-staged-border')).toBe('');
    expect(rootStyle.getPropertyValue('--git-color-pull')).toBe('');
    expect(rootStyle.getPropertyValue('--git-color-push')).toBe('');
  });

  it('uses canonical dark overlay stops when a theme omits scrollbar values', () => {
    const service = new ThemeService();
    const fallbackTheme: ThemeConfig = {
      ...haloDarkTheme,
      id: 'fallback-dark',
      colors: {
        ...haloDarkTheme.colors,
        scrollbar: undefined,
      },
    } as unknown as ThemeConfig;

    (service as unknown as { injectCSSVariables(theme: ThemeConfig): void }).injectCSSVariables(fallbackTheme);

    expect(document.documentElement.style.getPropertyValue('--scrollbar-thumb-hover')).toBe('rgba(255, 255, 255, 0.24)');
  });

  it('exports only the compact low-risk shadow overlay stops', async () => {
    const service = new ThemeService();

    await service.applyTheme('halo-dark');

    const rootStyle = document.documentElement.style;
    for (const [tone, stop] of [['white', '06'], ['white', '10'], ['black', '10']] as const) {
      expect(rootStyle.getPropertyValue(`--color-overlay-${tone}-${stop}`)).toBe('');
    }
    expect(rootStyle.getPropertyValue('--color-overlay-white-08')).toBe('rgba(255, 255, 255, 0.08)');
    expect(rootStyle.getPropertyValue('--color-overlay-white-12')).toBe('rgba(255, 255, 255, 0.12)');
    expect(rootStyle.getPropertyValue('--color-overlay-black-12')).toBe('rgba(0, 0, 0, 0.12)');
    expect(rootStyle.getPropertyValue('--color-overlay-black-30')).toBe('rgba(0, 0, 0, 0.3)');
  });

  it('initializes from bootstrap theme selection without reading or writing themes.current', async () => {
    bootstrapGlobals.__HALO_BOOTSTRAP_THEME_ID__ = 'halo-slate';
    bootstrapGlobals.__HALO_BOOTSTRAP_THEME_SELECTION__ = 'halo-slate';
    const service = new ThemeService();

    await service.initialize();

    expect(service.getCurrentThemeId()).toBe('halo-slate');
    expect(document.documentElement.getAttribute('data-theme')).toBe('halo-slate');
    expect(configAPI.getConfig).not.toHaveBeenCalled();
    expect(configAPI.getConfig).not.toHaveBeenCalledWith(
      'themes.current',
      expect.anything(),
    );
    expect(configAPI.setConfig).not.toHaveBeenCalledWith(
      'themes.current',
      expect.anything(),
    );
  });

  it('loads custom themes on demand after initialization and deduplicates repeated loads', async () => {
    bootstrapGlobals.__HALO_BOOTSTRAP_THEME_ID__ = 'halo-slate';
    bootstrapGlobals.__HALO_BOOTSTRAP_THEME_SELECTION__ = 'halo-slate';
    const service = new ThemeService();
    await service.initialize();

    await service.ensureUserThemesLoaded();
    await service.ensureUserThemesLoaded();

    expect(configAPI.getConfig).toHaveBeenCalledTimes(1);
    expect(configAPI.getConfig).toHaveBeenCalledWith(
      'themes',
      expect.objectContaining({ skipRetryOnNotFound: true }),
    );
  });

  it('falls back to config lookup when bootstrap theme selection is unavailable', async () => {
    bootstrapGlobals.__HALO_BOOTSTRAP_THEME_ID__ = 'halo-light';
    vi.mocked(configAPI.getConfig).mockImplementation(async (key: string) => {
      if (key === 'themes.current') {
        return 'halo-slate';
      }
      return undefined;
    });
    const service = new ThemeService();

    await service.initialize();

    expect(service.getCurrentThemeId()).toBe('halo-slate');
    expect(configAPI.getConfig).toHaveBeenCalledWith(
      'themes.current',
      expect.objectContaining({ skipRetryOnNotFound: true }),
    );
  });

  it('applies saved custom theme during initialization when bootstrap cannot provide it', async () => {
    const customTheme: ThemeConfig = {
      ...haloLightTheme,
      id: 'custom-ocean',
      name: 'Custom Ocean',
      colors: {
        ...haloLightTheme.colors,
        background: {
          ...haloLightTheme.colors.background,
          primary: '#001122',
        },
      },
    };
    vi.mocked(configAPI.getConfig).mockImplementation(async (key: string) => {
      if (key === 'themes.current') {
        return 'custom-ocean';
      }
      if (key === 'themes') {
        return { custom: [customTheme] };
      }
      return undefined;
    });
    const service = new ThemeService();

    await service.initialize();
    await service.ensureUserThemesLoaded();

    expect(service.getCurrentThemeId()).toBe('custom-ocean');
    expect(service.getResolvedThemeId()).toBe('custom-ocean');
    expect(document.documentElement.getAttribute('data-theme')).toBe('custom-ocean');
    expect(document.documentElement.style.getPropertyValue('--color-bg-primary')).toBe('#001122');
    expect(configAPI.getConfig).toHaveBeenCalledWith(
      'themes',
      expect.objectContaining({ skipRetryOnNotFound: true }),
    );
    expect(vi.mocked(configAPI.getConfig).mock.calls.filter(([key]) => key === 'themes')).toHaveLength(1);
    expect(configAPI.setConfig).not.toHaveBeenCalledWith('themes.current', 'custom-ocean');
  });

  it('does not persist the theme selection again during initialization', async () => {
    vi.mocked(configAPI.getConfig).mockImplementation(async (key: string) => {
      if (key === 'themes.current') {
        return 'halo-slate';
      }
      return undefined;
    });
    const service = new ThemeService();

    await service.initialize();

    expect(configAPI.setConfig).not.toHaveBeenCalledWith(
      'themes.current',
      expect.anything(),
    );
  });

  it('validates the core theme schema instead of only root fields', () => {
    const service = new ThemeService();
    const invalidTheme: ThemeConfig = {
      ...haloLightTheme,
      id: 'custom-invalid-semantic',
      name: 'Invalid Semantic',
      colors: {
        ...haloLightTheme.colors,
        semantic: {
          ...haloLightTheme.colors.semantic,
          success: 'not-a-color',
        },
      },
    };

    const result = service.validateTheme(invalidTheme);

    expect(result.valid).toBe(false);
    expectThemeError(result, 'colors.semantic.success', 'INVALID_COLOR_FORMAT');

    const incompleteTheme = {
      ...haloLightTheme,
      id: 'custom-incomplete',
      name: 'Incomplete Custom',
      effects: undefined,
      motion: undefined,
      typography: undefined,
    } as unknown as ThemeConfig;
    const incompleteResult = service.validateTheme(incompleteTheme);

    expect(incompleteResult.valid).toBe(false);
    expectThemeError(incompleteResult, 'effects', 'MISSING_THEME_FIELD_GROUP');
    expectThemeError(incompleteResult, 'motion', 'MISSING_THEME_FIELD_GROUP');
    expectThemeError(incompleteResult, 'typography', 'MISSING_THEME_FIELD_GROUP');

    const invalidOptionalTheme = {
      ...haloLightTheme,
      id: 'custom-invalid-optional-scrollbar',
      name: 'Invalid Optional Scrollbar',
      colors: {
        ...haloLightTheme.colors,
        scrollbar: {
          thumb: 'invalid',
          thumbHover: '#ffffff',
        },
      },
    } as unknown as ThemeConfig;
    const invalidOptionalResult = service.validateTheme(invalidOptionalTheme);

    expect(invalidOptionalResult.valid).toBe(false);
    expectThemeError(invalidOptionalResult, 'colors.scrollbar.thumb', 'INVALID_COLOR_FORMAT');
  });

  it('normalizes older partial custom themes before applying them', async () => {
    const partialCustomTheme = {
      id: 'custom-partial',
      name: 'Partial Custom',
      type: 'light',
      colors: {
        background: {
          primary: '#101820',
        },
        text: {
          primary: '#f8fafc',
        },
        accent: {
          500: '#2f80ed',
        },
      },
    } as unknown as ThemeConfig;
    vi.mocked(configAPI.getConfig).mockImplementation(async (key: string) => {
      if (key === 'themes.current') {
        return 'custom-partial';
      }
      if (key === 'themes') {
        return { custom: [partialCustomTheme] };
      }
      return undefined;
    });
    const service = new ThemeService();

    await service.initialize();

    const normalized = service.getTheme('custom-partial');
    expect(service.getCurrentThemeId()).toBe('custom-partial');
    expect(service.getResolvedThemeId()).toBe('custom-partial');
    expect(normalized?.colors.background.primary).toBe('#101820');
    expect(normalized?.colors.background.secondary).toBe(haloLightTheme.colors.background.secondary);
    expect(normalized?.colors.text.primary).toBe('#f8fafc');
    expect(normalized?.colors.text.secondary).toBe(haloLightTheme.colors.text.secondary);
    expect(normalized?.effects.spacing[4]).toBe(haloLightTheme.effects.spacing[4]);
    expect(document.documentElement.style.getPropertyValue('--color-bg-primary')).toBe('#101820');
    expect(document.documentElement.style.getPropertyValue('--color-bg-secondary')).toBe(
      haloLightTheme.colors.background.secondary,
    );
    expect(configAPI.setConfig).not.toHaveBeenCalledWith('themes.custom', expect.anything());
  });

  it('does not inject non-contract dynamic keys from custom themes', () => {
    const service = new ThemeService();
    const customTheme = {
      ...haloLightTheme,
      id: 'custom-extra-keys',
      colors: {
        ...haloLightTheme.colors,
        accent: {
          ...haloLightTheme.colors.accent,
          950: '#111111',
        },
        purple: {
          ...haloLightTheme.colors.purple,
          300: '#222222',
          700: '#333333',
        },
      },
      effects: {
        ...haloLightTheme.effects,
        shadow: {
          ...haloLightTheme.effects.shadow,
          '2xl': '0 0 0 #111111',
        },
        blur: {
          ...haloLightTheme.effects.blur,
          intense: 'blur(99px)',
        },
        radius: {
          ...haloLightTheme.effects.radius,
          huge: '99px',
        },
        spacing: {
          ...haloLightTheme.effects.spacing,
          99: '99px',
        },
      },
      motion: {
        ...haloLightTheme.motion,
        duration: {
          ...haloLightTheme.motion.duration,
          lazy: '99s',
        },
        easing: {
          ...haloLightTheme.motion.easing,
          bounce: 'cubic-bezier(0.68, -0.55, 0.265, 1.55)',
        },
      },
      typography: {
        ...haloLightTheme.typography,
        weight: {
          ...haloLightTheme.typography.weight,
          black: 900,
        },
        size: {
          ...haloLightTheme.typography.size,
          '5xl': '99px',
        },
        lineHeight: {
          ...haloLightTheme.typography.lineHeight,
          loose: 2,
        },
      },
    } as unknown as ThemeConfig;

    (service as unknown as { injectCSSVariables(theme: ThemeConfig): void }).injectCSSVariables(customTheme);

    const rootStyle = document.documentElement.style;
    expect(rootStyle.getPropertyValue('--color-accent-950')).toBe('');
    expect(rootStyle.getPropertyValue('--color-purple-300')).toBe('');
    expect(rootStyle.getPropertyValue('--color-purple-700')).toBe('');
    expect(rootStyle.getPropertyValue('--shadow-2xl')).toBe('');
    expect(rootStyle.getPropertyValue('--blur-intense')).toBe('');
    expect(rootStyle.getPropertyValue('--size-radius-huge')).toBe('');
    expect(rootStyle.getPropertyValue('--size-gap-99')).toBe('');
    expect(rootStyle.getPropertyValue('--motion-lazy')).toBe('');
    expect(rootStyle.getPropertyValue('--easing-bounce')).toBe('');
    expect(rootStyle.getPropertyValue('--font-weight-black')).toBe('');
    expect(rootStyle.getPropertyValue('--font-size-5xl')).toBe('');
    expect(rootStyle.getPropertyValue('--line-height-loose')).toBe('');
  });

  it('projects theme motion duration tokens', () => {
    const service = new ThemeService();
    const customTheme = {
      ...haloLightTheme,
      id: 'custom-motion-alias',
      motion: {
        ...haloLightTheme.motion,
        duration: {
          ...haloLightTheme.motion.duration,
          slow: '0.7s',
        },
      },
    } as unknown as ThemeConfig;

    (service as unknown as { injectCSSVariables(theme: ThemeConfig): void }).injectCSSVariables(customTheme);

    const rootStyle = document.documentElement.style;
    expect(rootStyle.getPropertyValue('--motion-slow')).toBe('0.7s');
  });

  it('does not expose window controls as a theme extension surface', () => {
    const service = new ThemeService();
    const customTheme = {
      ...haloLightTheme,
      id: 'custom-window-controls',
      components: {
        ...haloLightTheme.components,
        windowControls: {
          close: {
            hoverColor: '#a85555',
          },
        },
      },
    } as unknown as ThemeConfig;

    (service as unknown as { injectCSSVariables(theme: ThemeConfig): void }).injectCSSVariables(customTheme);
    expect(document.documentElement.style.getPropertyValue('--window-control-close-hover-color')).toBe('');
    expect(document.documentElement.getAttribute('data-window-control-close-hover-override')).toBeNull();
  });

  it('skips invalid persisted custom themes before they reach preview or runtime injection', async () => {
    const invalidCustomTheme = {
      ...haloLightTheme,
      id: 'custom-broken',
      name: 'Broken Custom',
      colors: {
        ...haloLightTheme.colors,
        background: {
          ...haloLightTheme.colors.background,
          primary: 'definitely-not-a-color',
        },
      },
    };
    vi.mocked(configAPI.getConfig).mockImplementation(async (key: string) => {
      if (key === 'themes.current') {
        return 'custom-broken';
      }
      if (key === 'themes') {
        return { custom: [invalidCustomTheme] };
      }
      return undefined;
    });
    const service = new ThemeService();

    await service.initialize();

    expect(service.getTheme('custom-broken')).toBeUndefined();
    expect(service.getCurrentThemeId()).toBe(SYSTEM_THEME_ID);
    expect(document.documentElement.getAttribute('data-theme')).not.toBe('custom-broken');
    expect(configAPI.setConfig).not.toHaveBeenCalledWith('themes.custom', expect.anything());
  });

  it('persists registered custom themes only after schema normalization succeeds', async () => {
    const service = new ThemeService();
    const partialCustomTheme = {
      id: 'custom-registered',
      name: 'Registered Custom',
      type: 'dark',
      colors: {
        background: {
          primary: '#04080f',
        },
        text: {
          primary: '#f8fafc',
        },
        accent: {
          500: '#7c3aed',
        },
      },
    } as unknown as ThemeConfig;

    await service.registerTheme(partialCustomTheme);

    const normalized = service.getTheme('custom-registered');
    expect(normalized?.colors.background.primary).toBe('#04080f');
    expect(normalized?.colors.background.secondary).toBe(haloDarkTheme.colors.background.secondary);
    expect(normalized?.effects.radius.base).toBe(haloDarkTheme.effects.radius.base);
    expect(configAPI.setConfig).toHaveBeenCalledWith(
      'themes.custom',
      expect.arrayContaining([
        expect.objectContaining({
          id: 'custom-registered',
          colors: expect.objectContaining({
            background: expect.objectContaining({
              primary: '#04080f',
              secondary: haloDarkTheme.colors.background.secondary,
            }),
          }),
        }),
      ]),
    );

    await expect(
      service.registerTheme({
        ...haloLightTheme,
        id: 'custom-invalid-register',
        colors: {
          ...haloLightTheme.colors,
          text: {
            ...haloLightTheme.colors.text,
            primary: 'invalid',
          },
        },
      }),
    ).rejects.toThrow(/Invalid theme/);
    expect(service.getTheme('custom-invalid-register')).toBeUndefined();

    await expect(
      service.registerTheme({
        ...haloLightTheme,
        id: '',
        name: '',
      }),
    ).rejects.toThrow(/Theme id cannot be empty/);

    await expect(
      service.registerTheme({
        ...haloLightTheme,
        name: 'Builtin Override',
      }),
    ).rejects.toThrow(/reserved for a built-in theme/);
  });

  it('strips non-contract git color keys from registered custom themes', async () => {
    const nonContractGitColorKeys = [
      'changesBg',
      'addedBg',
      'deletedBg',
      'stagedBg',
      'addedBgHover',
      'stagedBorder',
      'pull',
    ] as const;
    const expectNoNonContractGitColorKeys = (gitColors: ThemeConfig['colors']['git']) => {
      const gitRecord = gitColors as unknown as Record<string, unknown>;
      nonContractGitColorKeys.forEach(key => {
        expect(gitRecord).not.toHaveProperty(key);
      });
    };
    const service = new ThemeService();
    const legacyTheme = {
      ...haloDarkTheme,
      id: 'custom-legacy-git-bg',
      name: 'Legacy Git Backgrounds',
      colors: {
        ...haloDarkTheme.colors,
        git: {
          ...haloDarkTheme.colors.git,
          changesBg: 'rgba(245, 158, 11, 0.1)',
          addedBg: 'rgba(34, 197, 94, 0.1)',
          deletedBg: 'rgba(239, 68, 68, 0.1)',
          stagedBg: 'rgba(16, 185, 129, 0.1)',
          addedBgHover: 'rgba(34, 197, 94, 0.2)',
          stagedBorder: 'rgba(16, 185, 129, 0.4)',
          pull: '#60a5fa',
        },
      },
    } as unknown as ThemeConfig;

    await service.registerTheme(legacyTheme);

    const normalized = service.getTheme('custom-legacy-git-bg');
    expect(normalized).toBeDefined();
    if (!normalized) {
      throw new Error('Expected custom legacy git theme to be registered');
    }
    expect(normalized.colors.git.added).toBe(haloDarkTheme.colors.git.added);
    expectNoNonContractGitColorKeys(normalized.colors.git);

    const persistedThemes = vi.mocked(configAPI.setConfig).mock.calls.find(([key]) => key === 'themes.custom')?.[1] as
      | ThemeConfig[]
      | undefined;
    const persistedTheme = persistedThemes?.find(theme => theme.id === 'custom-legacy-git-bg');
    expect(persistedTheme).toBeDefined();
    if (!persistedTheme) {
      throw new Error('Expected custom legacy git theme to be persisted');
    }
    expectNoNonContractGitColorKeys(persistedTheme.colors.git);

    const exported = service.exportTheme('custom-legacy-git-bg');
    expect(exported).not.toBeNull();
    if (!exported) {
      throw new Error('Expected custom legacy git theme to be exported');
    }
    expectNoNonContractGitColorKeys(exported.theme.colors.git);
  });

  it('strips retired theme authoring keys from registered custom themes', async () => {
    const service = new ThemeService();
    const retiredAuthoringTheme = createThemeWithRetiredAuthoringKeys(
      'custom-retired-authoring',
      'Retired Authoring Keys',
    );

    await service.registerTheme(retiredAuthoringTheme);

    const normalized = service.getTheme('custom-retired-authoring');
    expect(normalized).toBeDefined();
    if (!normalized) {
      throw new Error('Expected custom theme with retired keys to be registered');
    }
    expect(normalized.colors.accent[700]).toBe(haloDarkTheme.colors.accent[700]);
    expectNoRetiredThemeAuthoringKeys(normalized);

    const persistedThemes = vi.mocked(configAPI.setConfig).mock.calls.find(([key]) => key === 'themes.custom')?.[1] as
      | ThemeConfig[]
      | undefined;
    const persistedTheme = persistedThemes?.find(theme => theme.id === 'custom-retired-authoring');
    expect(persistedTheme).toBeDefined();
    if (!persistedTheme) {
      throw new Error('Expected custom theme with retired keys to be persisted');
    }
    expectNoRetiredThemeAuthoringKeys(persistedTheme);

    const exported = service.exportTheme('custom-retired-authoring');
    expect(exported).not.toBeNull();
    if (!exported) {
      throw new Error('Expected custom theme with retired keys to be exported');
    }
    expectNoRetiredThemeAuthoringKeys(exported.theme);
  });

  it('migrates persisted custom themes with retired authoring keys on load', async () => {
    const retiredAuthoringTheme = createThemeWithRetiredAuthoringKeys(
      'custom-loaded-retired-authoring',
      'Loaded Retired Authoring Keys',
    );
    vi.mocked(configAPI.getConfig).mockResolvedValue({ custom: [retiredAuthoringTheme] });
    const service = new ThemeService();

    await service.ensureUserThemesLoaded();

    const normalized = service.getTheme('custom-loaded-retired-authoring');
    expect(normalized).toBeDefined();
    if (!normalized) {
      throw new Error('Expected custom theme with retired keys to load');
    }
    expectNoRetiredThemeAuthoringKeys(normalized);

    const migratedThemes = vi.mocked(configAPI.setConfig).mock.calls.find(([key]) => key === 'themes.custom')?.[1] as
      | ThemeConfig[]
      | undefined;
    expect(migratedThemes).toHaveLength(1);
    const migratedTheme = migratedThemes?.[0];
    expect(migratedTheme).toBeDefined();
    if (!migratedTheme) {
      throw new Error('Expected custom theme with retired keys to be migrated');
    }
    expectNoRetiredThemeAuthoringKeys(migratedTheme);
  });

  it('migrates persisted custom themes with non-contract git color keys on load', async () => {
    const legacyTheme = {
      ...haloDarkTheme,
      id: 'custom-loaded-legacy-git',
      name: 'Loaded Legacy Git',
      colors: {
        ...haloDarkTheme.colors,
        git: {
          ...haloDarkTheme.colors.git,
          changesBg: 'rgba(245, 158, 11, 0.1)',
          addedBgHover: 'rgba(34, 197, 94, 0.2)',
          stagedBorder: 'rgba(16, 185, 129, 0.4)',
        },
      },
    } as unknown as ThemeConfig;
    vi.mocked(configAPI.getConfig).mockResolvedValue({ custom: [legacyTheme] });
    const service = new ThemeService();

    await service.ensureUserThemesLoaded();

    const normalized = service.getTheme('custom-loaded-legacy-git');
    expect(normalized).toBeDefined();
    if (!normalized) {
      throw new Error('Expected legacy custom theme to load');
    }
    expect(normalized.colors.git.added).toBe(haloDarkTheme.colors.git.added);
    expect(normalized.colors.git.staged).toBe(haloDarkTheme.colors.git.staged);
    expect(normalized.colors.git as unknown as Record<string, unknown>).not.toHaveProperty('changesBg');
    expect(normalized.colors.git as unknown as Record<string, unknown>).not.toHaveProperty('addedBgHover');
    expect(normalized.colors.git as unknown as Record<string, unknown>).not.toHaveProperty('stagedBorder');

    const migratedThemes = vi.mocked(configAPI.setConfig).mock.calls.find(([key]) => key === 'themes.custom')?.[1] as
      | ThemeConfig[]
      | undefined;
    expect(migratedThemes).toHaveLength(1);
    const migratedGitColors = migratedThemes?.[0]?.colors.git as unknown as Record<string, unknown> | undefined;
    expect(migratedGitColors).toBeDefined();
    if (!migratedGitColors) {
      throw new Error('Expected migrated theme to keep git colors');
    }
    expect(migratedGitColors).not.toHaveProperty('changesBg');
    expect(migratedGitColors).not.toHaveProperty('addedBgHover');
    expect(migratedGitColors).not.toHaveProperty('stagedBorder');
  });

  it('projects normalized custom themes through the compact plugin color boundary', async () => {
    const service = new ThemeService();
    const partialCustomTheme = {
      id: 'custom-plugin-projection',
      name: 'Plugin Projection',
      type: 'dark',
      colors: {
        accent: {
          500: '#14b8a6',
          600: '#0f766e',
        },
        purple: {
          500: '#a855f7',
        },
        semantic: {
          success: '#22c55e',
          warning: '#f59e0b',
          error: '#ef4444',
          info: '#38bdf8',
        },
      },
    } as unknown as ThemeConfig;

    await service.registerTheme(partialCustomTheme);

    const normalized = service.getTheme('custom-plugin-projection');
    expect(normalized).toBeDefined();
    const projection = createPluginThemeColorProjection(normalized!);

    expect(Object.keys(projection).sort()).toEqual([...PLUGIN_THEME_COLOR_KEYS].sort());
    expect(projection.primary).toBe('#14b8a6');
    expect(projection.secondary).toBe('#a855f7');
    expect(projection.accent).toBe('#0f766e');
    expect(projection.success).toBe('#22c55e');
    expect(projection.warning).toBe('#f59e0b');
    expect(projection.error).toBe('#ef4444');
    expect(projection.info).toBe('#38bdf8');
  });
});
