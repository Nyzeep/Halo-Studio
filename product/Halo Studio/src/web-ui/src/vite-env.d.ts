/// <reference types="vite/client" />

declare module 'virtual:halo-canvas-runtime-bundle' {
  const bundle: {
    js: string;
    css: string;
  };
  export const haloCanvasRuntimeBundle: typeof bundle;
  export default bundle;
}
