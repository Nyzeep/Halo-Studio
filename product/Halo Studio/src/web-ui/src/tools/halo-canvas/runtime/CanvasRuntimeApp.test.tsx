import { afterEach, describe, expect, it, vi } from 'vitest';

import { installHaloCanvasRuntimeApp } from './CanvasRuntimeApp';

type TestWindow = Window & {
  HaloCanvasSDK?: Record<string, unknown>;
  HaloCanvasSDKAdapters?: Record<string, unknown>;
  HaloCanvasRuntime?: Record<string, any>;
  ReactDOM?: {
    createRoot: (element: HTMLElement) => { render: (node: unknown) => void };
  };
};

const originalWindow = (globalThis as typeof globalThis & { window?: Window }).window;
const originalDocument = (globalThis as typeof globalThis & { document?: Document }).document;

afterEach(() => {
  (globalThis as typeof globalThis & { window?: Window }).window = originalWindow;
  (globalThis as typeof globalThis & { document?: Document }).document = originalDocument;
});

function installTestDom() {
  const render = vi.fn();
  const postMessage = vi.fn();
  const rootElement = {
    innerHTML: '',
    querySelector: vi.fn(),
  } as unknown as HTMLElement;
  const testWindow = {
    parent: { postMessage },
    setTimeout: vi.fn(),
    clearTimeout: vi.fn(),
    HaloCanvasSDK: { Fallback: true },
    HaloCanvasSDKAdapters: { Adapter: true },
    HaloCanvasRuntime: { fallback: true },
    ReactDOM: {
      createRoot: vi.fn(() => ({ render })),
    },
  } as unknown as TestWindow;
  const testDocument = {
    getElementById: vi.fn(() => rootElement),
    querySelector: vi.fn(() => ({ getAttribute: () => 'rev_test' })),
  } as unknown as Document;

  (globalThis as typeof globalThis & { window?: TestWindow }).window = testWindow;
  (globalThis as typeof globalThis & { document?: Document }).document = testDocument;

  return { render, postMessage, rootElement, testWindow };
}

describe('CanvasRuntimeApp', () => {
  it('installs runtime hooks without removing fallback runtime fields', () => {
    const { testWindow } = installTestDom();

    installHaloCanvasRuntimeApp();

    expect(testWindow.HaloCanvasRuntime?.fallback).toBe(true);
    expect(testWindow.HaloCanvasRuntime?.h).toBeTypeOf('function');
    expect(testWindow.HaloCanvasRuntime?.Fragment).toBeTruthy();
  });

  it('merges SDK adapters on module start and posts startup event', () => {
    const { postMessage, testWindow } = installTestDom();

    installHaloCanvasRuntimeApp();
    testWindow.HaloCanvasRuntime?.moduleStarted();

    expect(testWindow.HaloCanvasSDK).toMatchObject({ Fallback: true, Adapter: true });
    expect(postMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'halo-canvas-module-started',
        sourceRevisionSeen: 'rev_test',
      }),
      '*',
    );
  });

  it('mounts user components through the bundled React runtime app', () => {
    const { render, postMessage, testWindow } = installTestDom();
    function Canvas() {
      return null;
    }

    installHaloCanvasRuntimeApp();
    testWindow.HaloCanvasRuntime?.mount(Canvas);

    expect(testWindow.ReactDOM?.createRoot).toHaveBeenCalled();
    expect(render).toHaveBeenCalled();
    expect(postMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        type: 'halo-canvas-ready',
        sourceRevisionSeen: 'rev_test',
      }),
      '*',
    );
  });
});
