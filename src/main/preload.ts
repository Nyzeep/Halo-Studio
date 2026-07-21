import { contextBridge, ipcRenderer } from "electron";
import type { HaloApi } from "../shared/api.js";

const api: HaloApi = {
  agents: {
    detectAll: () => ipcRenderer.invoke("agents:detectAll")
  },
  config: {
    applyDemoWrite: (request) => ipcRenderer.invoke("config:applyDemoWrite", request),
    rollbackWrite: (request) => ipcRenderer.invoke("config:rollbackWrite", request)
  },
  mcp: {
    previewConfig: (server) => ipcRenderer.invoke("mcp:previewConfig", server)
  },
  sessions: {
    start: (request) => ipcRenderer.invoke("sessions:start", request),
    stop: (sessionId) => ipcRenderer.invoke("sessions:stop", sessionId),
    write: (sessionId, data) => ipcRenderer.invoke("sessions:write", sessionId, data),
    resize: (sessionId, cols, rows) => ipcRenderer.invoke("sessions:resize", sessionId, cols, rows),
    onData: (callback) => {
      const listener = (_event: Electron.IpcRendererEvent, payload: { sessionId: string; data: string }) => {
        callback(payload);
      };
      ipcRenderer.on("sessions:data", listener);
      return () => ipcRenderer.off("sessions:data", listener);
    },
    onExit: (callback) => {
      const listener = (_event: Electron.IpcRendererEvent, payload: { sessionId: string; exitCode: number | null }) => {
        callback(payload);
      };
      ipcRenderer.on("sessions:exit", listener);
      return () => ipcRenderer.off("sessions:exit", listener);
    }
  }
};

contextBridge.exposeInMainWorld("halo", api);
