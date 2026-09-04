

import { ThemeConfig } from '../types';
import {
  createAccentScale,
  createGitColors,
  createSemanticColors,
  createSecondaryAccentScale,
  createStandardEasing,
  createStandardRadius,
  createStandardSpacing,
  createStandardTypography,
  rgbFromHex,
  rgbaFromHex,
  STATIC_WHITE,
} from './shared';

const LIGHT_INK = '#0f172a';
const LIGHT_TEXT_PRIMARY = '#1e293b';
const LIGHT_TEXT_STRONG = '#334155';
// Interactive accent: Google-blue used for primary actions, focus rings, and selection.
const LIGHT_ACCENT = '#1a73e8';
const LIGHT_ACCENT_HOVER = '#1765cc';
// Neutral slate keeps borders, muted text, and git chrome free of accent tint.
const LIGHT_NEUTRAL = '#64748b';
const LIGHT_NEUTRAL_HOVER = '#475569';
// Google gray-800 drives the elevation shadows like Google Workspace surfaces.
const LIGHT_SHADE = '#3c4043';
const LIGHT_PURPLE = '#7c6b99';
const LIGHT_PURPLE_HOVER = '#655680';
const LIGHT_SUCCESS = '#5b9a6f';
const LIGHT_WARNING = '#c08c42';
const LIGHT_ERROR = '#c26565';

const lightInk = (alpha: number | string) => rgbaFromHex(LIGHT_INK, alpha);
const lightAccent = (alpha: number | string) => rgbaFromHex(LIGHT_ACCENT, alpha);
const lightNeutral = (alpha: number | string) => rgbaFromHex(LIGHT_NEUTRAL, alpha);
const lightNeutralHover = (alpha: number | string) => rgbaFromHex(LIGHT_NEUTRAL_HOVER, alpha);
const lightShade = (alpha: number | string) => rgbaFromHex(LIGHT_SHADE, alpha);

export const haloLightTheme: ThemeConfig = {

  id: 'halo-light',
  name: 'Light',
  type: 'light',
  description: 'Light theme - Soft neutral surfaces, Google-blue primary actions',
  author: 'Halo Studio Team',
  version: '2.4.0',

  layout: {
    sceneViewportBorder: false,
  },


  colors: {
    background: {
      primary: '#f8f9fa',
      secondary: STATIC_WHITE,
      tertiary: '#f1f3f4',
      elevated: STATIC_WHITE,
      workbench: '#f1f3f4',
      scene: STATIC_WHITE,
    },

    text: {
      primary: LIGHT_TEXT_PRIMARY,
      secondary: '#3d4f66',
      muted: LIGHT_NEUTRAL,
      disabled: '#94a3b8',
    },


    accent: createAccentScale({
      base: LIGHT_ACCENT,
      hover: LIGHT_ACCENT_HOVER,
      alpha: { 700: 0.88 },
    }),


    purple: createSecondaryAccentScale({
      base: '#6b5a89',
      hover: LIGHT_PURPLE_HOVER,
      alpha: { 200: 0.14 },
      stops: {
        500: LIGHT_PURPLE,
      },
    }),


    semantic: createSemanticColors({
      success: LIGHT_SUCCESS,
      warning: LIGHT_WARNING,
      error: LIGHT_ERROR,
      info: LIGHT_ACCENT,
      bgAlpha: 0.08,
      borderAlpha: 0.25,
      overrides: {
        infoBg: lightAccent(0.1),
        infoBorder: lightAccent(0.28),
      },
    }),


    border: {
      subtle: lightNeutral(0.15),
      base: lightNeutral(0.22),
      medium: lightNeutral(0.32),
      strong: lightNeutral(0.42),
      prominent: lightNeutral(0.52),
    },


    element: {
      subtle: lightInk(0.045),
      soft: lightInk(0.065),
      base: lightInk(0.09),
      medium: lightInk(0.12),
      strong: lightInk(0.16),
    },


    git: createGitColors({
      branch: rgbFromHex(LIGHT_NEUTRAL_HOVER),
      branchBg: lightNeutralHover(0.1),
      changes: rgbFromHex(LIGHT_WARNING),
      added: rgbFromHex(LIGHT_SUCCESS),
      deleted: rgbFromHex(LIGHT_ERROR),
    }),
  },


  effects: {
    shadow: {
      // Google Workspace-style two-layer elevation.
      xs: `0 1px 2px ${lightShade(0.1)}`,
      sm: `0 1px 3px ${lightShade(0.16)}`,
      base: `0 1px 3px ${lightShade(0.3)}, 0 4px 8px 3px ${lightShade(0.08)}`,
      lg: `0 4px 8px 3px ${lightShade(0.12)}, 0 14px 28px ${lightShade(0.24)}`,
      xl: `0 6px 10px 4px ${lightShade(0.14)}, 0 20px 40px ${lightShade(0.28)}`,
    },


    blur: {
      subtle: 'blur(4px) saturate(1.02)',
      base: 'blur(8px) saturate(1.05)',
    },

    radius: createStandardRadius(),

    spacing: createStandardSpacing(),

    opacity: {
      disabled: 0.55,
      hover: 0.75,
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
          background: '#0b57d0',
          color: STATIC_WHITE,
          border: 'transparent',
          shadow: 'none',
        },
        hover: {
          background: '#0a4cb4',
          color: STATIC_WHITE,
          border: 'transparent',
          shadow: `0 1px 3px ${lightShade(0.3)}, 0 2px 6px 2px ${lightShade(0.12)}`,
          transform: 'none',
        },
        active: {
          background: '#093f99',
          color: STATIC_WHITE,
          border: 'transparent',
          shadow: `0 1px 2px ${lightShade(0.28)}`,
          transform: 'none',
        },
      },


      ghost: {
        default: {
          color: LIGHT_ACCENT_HOVER,
        },
        hover: {
          background: lightInk(0.08),
          color: LIGHT_TEXT_STRONG,
          border: 'transparent',
        },
      },
    },
  },


  monaco: {
    base: 'vs',
    inherit: true,
    rules: [
      { token: 'comment', foreground: '94a3b8', fontStyle: 'italic' },
      { token: 'keyword', foreground: '6b5a89' },
      { token: 'string', foreground: '5b9a6f' },
      { token: 'number', foreground: 'b8863a' },
      { token: 'type', foreground: '475569' },
      { token: 'class', foreground: '475569' },
      { token: 'function', foreground: '7c6b99' },
      { token: 'variable', foreground: '475569' },
      { token: 'constant', foreground: 'c08c42' },
      { token: 'operator', foreground: '6b5a89' },
      { token: 'tag', foreground: '475569' },
      { token: 'attribute.name', foreground: '7c6b99' },
      { token: 'attribute.value', foreground: '5b9a6f' },
    ],
    colors: {
      background: '#f8f9fa',
      foreground: LIGHT_TEXT_PRIMARY,
      lineHighlight: '#eef3fc',
      selection: '#d3e3fd',
      cursor: LIGHT_TEXT_PRIMARY,

      'editor.selectionBackground': '#d3e3fd',
      'editor.selectionForeground': LIGHT_TEXT_PRIMARY,
      'editor.inactiveSelectionBackground': lightAccent(0.12),
      'editor.selectionHighlightBackground': lightAccent(0.14),
      'editor.selectionHighlightBorder': lightAccent(0.32),
      'editorCursor.foreground': LIGHT_TEXT_PRIMARY,

      'editor.wordHighlightBackground': lightAccent(0.1),
      'editor.wordHighlightStrongBackground': lightAccent(0.16),
    },
  },
};




