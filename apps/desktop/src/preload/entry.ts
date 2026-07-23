import { contextBridge, ipcRenderer } from "electron";

import { installHaloPreload } from "./preload.js";

installHaloPreload(contextBridge, (channel, request) => ipcRenderer.invoke(channel, request));
