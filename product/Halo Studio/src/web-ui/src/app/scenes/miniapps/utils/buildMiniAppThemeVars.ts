/**
 * Build MiniApp theme payload from main app ThemeConfig.
 * Maps to --halo-* CSS variables for iframe theme sync.
 */
import type { ThemeConfig, ThemeType } from '@/infrastructure/theme/types';
import { MINI_APP_SCROLLBAR_FALLBACKS } from '@/shared/theme/themeBoundaryFallbacks';

export interface MiniAppThemePayload {
  type: ThemeType;
  id: string;
  vars: Record<string, string>;
}

export function buildMiniAppThemeVars(theme: ThemeConfig | null): MiniAppThemePayload | null {
  if (!theme) return null;

  const { colors, effects, typography } = theme;
  const vars: Record<string, string> = {};

  vars['--halo-bg'] = colors.background.primary;
  vars['--halo-bg-secondary'] = colors.background.secondary;
  vars['--halo-bg-tertiary'] = colors.background.tertiary;
  vars['--halo-bg-elevated'] = colors.background.elevated;

  vars['--halo-text'] = colors.text.primary;
  vars['--halo-text-secondary'] = colors.text.secondary;
  vars['--halo-text-muted'] = colors.text.muted;

  vars['--halo-accent'] = colors.accent[500];
  vars['--halo-accent-hover'] = colors.accent[600];

  vars['--halo-success'] = colors.semantic.success;
  vars['--halo-warning'] = colors.semantic.warning;
  vars['--halo-error'] = colors.semantic.error;
  vars['--halo-info'] = colors.semantic.info;

  vars['--halo-border'] = colors.border.base;
  vars['--halo-border-subtle'] = colors.border.subtle;

  vars['--halo-element-bg'] = colors.element.base;
  vars['--halo-element-hover'] = colors.element.medium;

  if (effects?.radius) {
    vars['--halo-radius'] = effects.radius.base;
    vars['--halo-radius-lg'] = effects.radius.lg;
  }

  if (typography?.font) {
    vars['--halo-font-sans'] = typography.font.sans;
    vars['--halo-font-mono'] = typography.font.mono;
  }

  if (colors.scrollbar) {
    vars['--halo-scrollbar-thumb'] = colors.scrollbar.thumb;
    vars['--halo-scrollbar-thumb-hover'] = colors.scrollbar.thumbHover;
  } else {
    const scrollbarFallback = MINI_APP_SCROLLBAR_FALLBACKS[theme.type];
    vars['--halo-scrollbar-thumb'] = scrollbarFallback.thumb;
    vars['--halo-scrollbar-thumb-hover'] = scrollbarFallback.thumbHover;
  }

  return {
    type: theme.type,
    id: theme.id,
    vars,
  };
}
