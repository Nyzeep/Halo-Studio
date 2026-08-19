import './styles/canvas-runtime.scss';
import * as sdkAdapters from './sdk';
import { installHaloCanvasRuntimeApp } from './CanvasRuntimeApp';

declare global {
  interface Window {
    HaloCanvasSDKAdapters?: typeof sdkAdapters;
  }
}

window.HaloCanvasSDKAdapters = sdkAdapters;
installHaloCanvasRuntimeApp();
