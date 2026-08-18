import React, { useEffect, useMemo, useRef } from 'react';
import { GENERATIVE_WIDGET_SHELL_HTML } from './GenerativeWidgetFrame';
import { readWidgetThemePayload } from './themePayload';

export interface GenerativeWidgetStaticRendererProps {
  widgetCode: string;
  className?: string;
}

function extractShellCss(html: string): string {
  const match = html.match(/<style>([\s\S]*?)<\/style>/);
  const css = match?.[1] ?? '';
  return css.replace(
    /^(\s*):root\s*\{/m,
    '$1.halo-generative-widget-static-renderer {',
  );
}

function runScripts(root: HTMLElement): void {
  const scripts = root.querySelectorAll('script');
  scripts.forEach((oldScript) => {
    const nextScript = document.createElement('script');
    for (let i = 0; i < oldScript.attributes.length; i += 1) {
      const attr = oldScript.attributes[i];
      nextScript.setAttribute(attr.name, attr.value);
    }
    if (oldScript.src) {
      nextScript.src = oldScript.src;
    } else {
      nextScript.textContent = oldScript.textContent;
    }
    oldScript.parentNode?.replaceChild(nextScript, oldScript);
  });
}

export const GenerativeWidgetStaticRenderer: React.FC<GenerativeWidgetStaticRendererProps> = ({
  widgetCode,
  className = '',
}) => {
  const rootRef = useRef<HTMLDivElement | null>(null);
  const shellCss = useMemo(
    () => extractShellCss(GENERATIVE_WIDGET_SHELL_HTML),
    [],
  );
  const themePayload = useMemo(() => readWidgetThemePayload(), []);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return;

    const globalWindow = window as Window & {
      haloWidget?: { send: (data: unknown) => void };
      glimpse?: { send: (data: unknown) => void };
      sendPrompt?: (text: string) => void;
    };

    const prevBridge = globalWindow.haloWidget;
    const prevGlimpse = globalWindow.glimpse;
    const prevSendPrompt = globalWindow.sendPrompt;

    const noopBridge = { send: (_data: unknown) => {} };
    globalWindow.haloWidget = noopBridge;
    globalWindow.glimpse = noopBridge;
    globalWindow.sendPrompt = (_text: string) => {};

    root.innerHTML = String(widgetCode || '');
    runScripts(root);

    return () => {
      root.innerHTML = '';
      globalWindow.haloWidget = prevBridge;
      globalWindow.glimpse = prevGlimpse;
      globalWindow.sendPrompt = prevSendPrompt;
    };
  }, [widgetCode]);

  const themeStyle = useMemo(() => {
    const style: React.CSSProperties & Record<string, string> = {
      background: 'transparent',
      color: 'var(--color-text-primary)',
      width: '100%',
    };

    Object.entries(themePayload?.vars ?? {}).forEach(([key, value]) => {
      style[key] = value;
    });

    return style;
  }, [themePayload]);

  return (
    <div
      className={`halo-generative-widget-static-renderer ${className}`.trim()}
      style={themeStyle}
      data-theme={themePayload?.id ?? 'unknown'}
      data-theme-type={themePayload?.type ?? 'dark'}
    >
      <style>{shellCss}</style>
      <div ref={rootRef} className="halo-generative-widget-static-renderer__root" />
    </div>
  );
};

export default GenerativeWidgetStaticRenderer;
