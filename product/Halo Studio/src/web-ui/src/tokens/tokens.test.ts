// @vitest-environment jsdom
/**
 * Token layer contract tests (M4, issue #53).
 *
 * - tokens.css must define the MD3 role vocabulary and the scale tiers
 *   (radius / spacing / font-size x fontScale / motion).
 * - tokens.css must disable motion under prefers-reduced-motion (M4
 *   acceptance: automated assertion for the global reduced-motion rule).
 * - theme.ts / themeStore.ts must uphold the data-theme DOM contract.
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { applyTheme, getSystemTheme, getThemePreference } from './theme';

function readTokensCss(): string {
  try {
    return readFileSync(new URL('./tokens.css', import.meta.url), 'utf8');
  } catch {
    // Under the vitest browser-style transform import.meta.url is not a file
    // URL; fall back to the package root (vitest always runs from there).
    return readFileSync(resolve(process.cwd(), 'src/tokens/tokens.css'), 'utf8');
  }
}

const tokensCss = readTokensCss();

const ROLE_TOKENS = [
  '--surface',
  '--surface-container-lowest',
  '--surface-container-low',
  '--surface-container',
  '--surface-container-high',
  '--surface-container-highest',
  '--on-surface',
  '--on-surface-variant',
  '--outline',
  '--outline-variant',
  '--primary',
  '--on-primary',
  '--primary-container',
  '--on-primary-container',
  '--error',
  '--on-error',
  '--error-container',
  '--on-error-container',
  '--success',
  '--warning',
] as const;

const SCALE_TOKENS = [
  '--radius-small',
  '--radius-medium',
  '--radius-large',
  '--radius-full',
  '--space-xs',
  '--space-sm',
  '--space-md',
  '--space-lg',
  '--space-xl',
  '--font-size-small',
  '--font-size-medium',
  '--font-size-large',
  '--font-size-xlarge',
  '--font-scale',
  '--motion-duration-short',
  '--motion-duration-medium',
  '--motion-duration-long',
  '--motion-easing-standard',
  '--motion-easing-decelerate',
  '--motion-easing-accelerate',
] as const;

describe('tokens.css vocabulary', () => {
  it('defines the MD3 surface/content/primary/semantic color roles', () => {
    for (const token of ROLE_TOKENS) {
      expect(tokensCss, `missing color role ${token}`).toContain(`${token}:`);
    }
  });

  it('defines the radius / spacing / typography / motion scales', () => {
    for (const token of SCALE_TOKENS) {
      expect(tokensCss, `missing scale token ${token}`).toContain(`${token}:`);
    }
  });

  it('scales the typography steps by --font-scale', () => {
    for (const step of ['--font-size-small', '--font-size-medium', '--font-size-large', '--font-size-xlarge']) {
      const declaration = new RegExp(`${step}:\\s*calc\\([^)]*var\\(--font-scale\\)\\)`);
      expect(tokensCss, `${step} must be multiplied by var(--font-scale)`).toMatch(declaration);
    }
  });

  it('provides dark and light role sets under the data-theme contract', () => {
    expect(tokensCss).toContain(":root[data-theme='dark']");
    expect(tokensCss).toContain(":root[data-theme='light']");
    expect(tokensCss).toContain('@media (prefers-color-scheme: light)');
  });

  it('disables motion under prefers-reduced-motion', () => {
    const reducedMotionBlock = tokensCss.match(
      /@media \(prefers-reduced-motion: reduce\) \{[\s\S]*?\n\}/,
    );
    expect(reducedMotionBlock, 'missing prefers-reduced-motion media rule').not.toBeNull();
    const block = reducedMotionBlock?.[0] ?? '';
    expect(block).toContain('animation-duration: 0.01ms !important');
    expect(block).toContain('transition-duration: 0.01ms !important');
    expect(block).toContain('scroll-behavior: auto !important');
  });
});

describe('theme utilities', () => {
  function createRoot(): HTMLElement {
    const root = document.createElement('div');
    document.body.appendChild(root);
    return root;
  }

  it('applyTheme writes the data-theme attribute for explicit preferences', () => {
    const root = createRoot();
    expect(applyTheme('dark', { root })).toBe('dark');
    expect(root.getAttribute('data-theme')).toBe('dark');
    expect(applyTheme('light', { root })).toBe('light');
    expect(root.getAttribute('data-theme')).toBe('light');
    root.remove();
  });

  it('applyTheme system clears the attribute so the media query decides', () => {
    const root = createRoot();
    root.setAttribute('data-theme', 'light');
    expect(applyTheme('system', { root })).toBe(getSystemTheme());
    expect(root.hasAttribute('data-theme')).toBe(false);
    root.remove();
  });

  it('getThemePreference reads the preference back from the root element', () => {
    const root = createRoot();
    expect(getThemePreference(root)).toBe('system');
    applyTheme('dark', { root });
    expect(getThemePreference(root)).toBe('dark');
    root.remove();
  });
});
