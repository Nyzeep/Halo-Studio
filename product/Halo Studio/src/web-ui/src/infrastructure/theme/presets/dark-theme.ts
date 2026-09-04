

import { ThemeConfig } from '../types';
import {
  createAccentScale,
  createDarkNeutralBorder,
  createDarkNeutralElement,
  createDarkNeutralScrollbar,
  createGitColors,
  createSemanticColors,
  createSecondaryAccentScale,
  createStandardEasing,
  createStandardRadius,
  createStandardSpacing,
  createStandardTypography,
  overlayBlack,
  overlayWhite,
  rgbFromHex,
  STATIC_WHITE,
} from './shared';

// Gemini-style dark neutrals: true dark canvas, layered surfaces.
const DARK_BACKGROUND_PRIMARY = '#131314';
const DARK_BACKGROUND_SECONDARY = '#1e1f20';
const DARK_BACKGROUND_ELEVATED = '#26282b';
const DARK_TEXT_PRIMARY = '#e3e3e3';
const DARK_BUTTON_TEXT = '#c8c8c8';
// Google-blue tuned for dark surfaces (Gemini dark accent).
const DARK_ACCENT = '#a8c7fa';
const DARK_ACCENT_HOVER = '#7cacf8';
const DARK_PURPLE = '#8b5cf6';
const DARK_PURPLE_HOVER = '#7c3aed';
const DARK_SUCCESS = '#34d399';
const DARK_WARNING = '#f59e0b';
const DARK_ERROR = '#ef4444';

export const haloDarkTheme: ThemeConfig = {

  id: 'halo-dark',
  name: 'Dark',
  type: 'dark',
  description: 'Default dark theme - Gemini-style layered neutrals',
  author: 'Halo Studio Team',
  version: '2.2.0',


  colors: {
    background: {
      primary: DARK_BACKGROUND_PRIMARY,
      secondary: DARK_BACKGROUND_SECONDARY,
      tertiary: DARK_BACKGROUND_PRIMARY,
      elevated: DARK_BACKGROUND_ELEVATED,
      workbench: DARK_BACKGROUND_PRIMARY,
      scene: DARK_BACKGROUND_SECONDARY,
    },

    text: {
      primary: DARK_TEXT_PRIMARY,
      secondary: '#c4c7c5',
      muted: '#9aa0a6',
      disabled: '#5f6368',
    },

    accent: createAccentScale({ base: DARK_ACCENT, hover: DARK_ACCENT_HOVER }),

    purple: createSecondaryAccentScale({ base: DARK_PURPLE, hover: DARK_PURPLE_HOVER }),

    semantic: createSemanticColors({
      success: DARK_SUCCESS,
      warning: DARK_WARNING,
      error: DARK_ERROR,
      info: '#a1a1aa',
      overrides: {
        infoBg: overlayWhite(0.08),
        infoBorder: overlayWhite(0.24),
      },
    }),

    border: createDarkNeutralBorder(),

    element: createDarkNeutralElement(),

    git: createGitColors({
      branch: '#a1a1aa',
      branchBg: overlayWhite(0.06),
      changes: rgbFromHex(DARK_WARNING),
      added: 'rgb(34, 197, 94)',
      deleted: rgbFromHex(DARK_ERROR),
    }),

    scrollbar: createDarkNeutralScrollbar(),
  },


  effects: {
    shadow: {
      xs: `0 1px 2px ${overlayBlack(0.5)}`,
      sm: `0 2px 6px ${overlayBlack(0.45)}`,
      base: `0 4px 12px ${overlayBlack(0.45)}`,
      lg: `0 8px 24px ${overlayBlack(0.5)}`,
      xl: `0 12px 32px ${overlayBlack(0.55)}`,
    },

    blur: {
      subtle: 'blur(4px) saturate(1.05)',
      base: 'blur(8px) saturate(1.1)',
    },

    radius: createStandardRadius(),

    spacing: createStandardSpacing(),

    opacity: {
      disabled: 0.6,
      hover: 0.8,
      focus: 0.9,
    },
  },


  motion: {
    duration: {
      instant: '0.1s',
      fast: '0.15s',
      base: '0.3s',
      slow: '0.6s',
    },

    easing: createStandardEasing(),
  },


  typography: createStandardTypography(),


  components: {
    button: {



      primary: {
        default: {
          background: overlayWhite(0.16),
          color: '#f3f3f5',
          border: 'transparent',
          shadow: 'none',
        },
        hover: {
          background: overlayWhite(0.24),
          color: STATIC_WHITE,
          border: 'transparent',
          shadow: '0 2px 8px rgba(0, 0, 0, 0.35)',
          transform: 'none',
        },
        active: {
          background: overlayWhite(0.2),
          color: STATIC_WHITE,
          border: 'transparent',
          shadow: '0 1px 4px rgba(0, 0, 0, 0.3)',
          transform: 'none',
        },
      },


      ghost: {
        default: {
          color: '#9a9a9a',
        },
        hover: {
          background: overlayWhite(0.1),
          color: DARK_BUTTON_TEXT,
          border: 'transparent',
        },
      },
    },
  },




  monaco: {
    base: 'vs-dark',
    inherit: true,
    rules: [],
    colors: {
      background: DARK_BACKGROUND_PRIMARY,
      foreground: DARK_TEXT_PRIMARY,
      lineHighlight: DARK_BACKGROUND_SECONDARY,
      selection: overlayWhite(0.12),
      cursor: DARK_BUTTON_TEXT,
    },
  },
};





