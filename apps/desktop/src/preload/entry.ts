import { contextBridge, ipcRenderer } from "electron";

import { installHaloPreload } from "./preload.js";

installHaloPreload(
  contextBridge,
  (channel, request) => ipcRenderer.invoke(channel, request),
  {
    on: (channel, listener) => ipcRenderer.on(channel, listener),
    removeListener: (channel, listener) => ipcRenderer.removeListener(channel, listener),
  },
);
