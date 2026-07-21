import React, { useState } from "react";
import { Sparkles, FolderClosed, History, Settings, Terminal, ChevronLeft, Info } from "lucide-react";
import { AgentRail } from "./components/AgentRail";
import { InspectorPanel } from "./components/InspectorPanel";
import { SessionTabs } from "./components/SessionTabs";
import { StatusBar } from "./components/StatusBar";
import { TerminalPane } from "./components/TerminalPane";
import { UtilityStrip } from "./components/UtilityStrip";
import { DashboardView } from "./components/DashboardView";
import { SettingsView } from "./components/SettingsView";
import { HistoryView } from "./components/HistoryView";
import { AboutView } from "./components/AboutView";
import { useAgents } from "./hooks/useAgents";
import { useTerminalSession } from "./hooks/useTerminalSession";

const defaultWorkspace = "D:\\Halo Studio";

export function App() {
  const { agents, loading, refreshDiscovery } = useAgents();
  const { sessions, activeSession, activeSessionId, setActiveSessionId, start, stop } = useTerminalSession();

  // Navigation State
  const [activeTab, setActiveTab] = useState<"dashboard" | "workspace" | "history" | "settings" | "about">("dashboard");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);

  const menuItems = [
    { id: "dashboard", label: "My Projects", icon: FolderClosed },
    { id: "workspace", label: "Active IDE", icon: Terminal },
    { id: "history", label: "Chat History", icon: History },
    { id: "settings", label: "Settings", icon: Settings },
    { id: "about", label: "About", icon: Info }
  ] as const;

  return (
    <div className="flex h-full w-full min-h-0 bg-[#05030a] text-slate-100 font-sans selection:bg-purple-500/30 selection:text-white">

      {/* 1. Left Nav Bar - Custom high-fidelity design from the reference image */}
      <aside className={`flex h-full flex-col glass-sidebar transition-all duration-300 shrink-0 ${
        sidebarCollapsed ? "w-16" : "w-64"
      }`}>
        {/* Top Logo & Collapse icon */}
        <div className="flex h-16 items-center justify-between border-b border-white/5 px-4">
          <div className="flex items-center gap-2.5 min-w-0">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl bg-purple-500/15 text-purple-400 cosmic-glow-border-active">
              <Sparkles size={16} className="text-purple-300 animate-pulse" />
            </div>
            {!sidebarCollapsed && (
              <span className="text-sm font-bold tracking-wider bg-gradient-to-r from-purple-200 via-indigo-200 to-purple-300 bg-clip-text text-transparent truncate">
                Halo-Studio
              </span>
            )}
          </div>
          <button
            onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
            className="rounded-lg p-1 text-slate-500 hover:bg-white/5 hover:text-slate-300 transition-colors hidden md:block"
          >
            <ChevronLeft size={16} className={`transition-transform duration-300 ${sidebarCollapsed ? "rotate-180" : ""}`} />
          </button>
        </div>

        {/* Navigation list */}
        <nav className="mt-6 flex-1 space-y-1.5 px-3">
          {menuItems.map((item) => {
            const Icon = item.icon;
            const isActive = activeTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => setActiveTab(item.id)}
                className={`flex w-full items-center gap-3 rounded-xl px-3.5 py-3 text-xs font-semibold transition-all ${
                  isActive
                    ? "bg-purple-600/15 text-purple-300 border border-purple-500/20 shadow-[0_0_15px_rgba(168,85,247,0.1)]"
                    : "text-slate-400 border border-transparent hover:bg-white/5 hover:text-slate-200"
                }`}
              >
                <Icon size={16} className={isActive ? "text-purple-400" : "text-slate-500"} />
                {!sidebarCollapsed && <span>{item.label}</span>}
              </button>
            );
          })}
        </nav>

        {/* Bottom User profile panel linked to About page */}
        <div
          onClick={() => setActiveTab("about")}
          className="border-t border-white/5 p-3.5 cursor-pointer hover:bg-white/5 transition-colors"
        >
          <div className="flex items-center gap-3">
            <div className="h-9 w-9 shrink-0 rounded-full border-2 border-purple-500/40 bg-[#0c0a1c] flex items-center justify-center overflow-hidden font-bold text-[10px] text-purple-300">
              NYZ
            </div>
            {!sidebarCollapsed && (
              <div className="min-w-0">
                <div className="truncate text-xs font-bold text-slate-200">鍏充簬 / About</div>
                <div className="truncate text-[10px] text-slate-500 font-medium">github.com/Nyzeep</div>
              </div>
            )}
          </div>
        </div>
      </aside>

      {/* 2. Main Content viewport depending on current tab */}
      <main className="relative flex min-w-0 flex-1 flex-col overflow-hidden bg-[#05030a]">
        {activeTab === "dashboard" && (
          <DashboardView
            agents={agents}
            loading={loading}
            onLaunchAgent={(agentId) => void start(agentId, defaultWorkspace)}
            onTransitionToTab={setActiveTab}
          />
        )}

        {activeTab === "workspace" && (
          <div className="flex h-full w-full min-h-0 overflow-hidden">
            {/* Left Agent Side control within Workspace tab */}
            <AgentRail agents={agents} loading={loading} onLaunch={(agentId) => void start(agentId, defaultWorkspace)} />

            {/* Center terminal viewport */}
            <div className="flex min-w-0 flex-1 flex-col">
              <SessionTabs
                sessions={sessions}
                activeSessionId={activeSessionId}
                onSelect={setActiveSessionId}
                onClose={(sessionId) => void stop(sessionId)}
              />
              <UtilityStrip />
              <div className="min-h-0 flex-1">
                <TerminalPane session={activeSession} />
              </div>
              <StatusBar activeSession={activeSession} />
            </div>

            {/* Right inspect & MCP previews panel */}
            <InspectorPanel agents={agents} activeSession={activeSession} />
          </div>
        )}

        {activeTab === "history" && (
          <HistoryView
            sessions={sessions}
            activeSessionId={activeSessionId}
            onSelectSession={setActiveSessionId}
            onCloseSession={(sessionId) => void stop(sessionId)}
          />
        )}

        {activeTab === "settings" && (
          <SettingsView
            agents={agents}
            loading={loading}
            onRefreshDiscovery={() => void refreshDiscovery()}
          />
        )}

        {activeTab === "about" && (
          <AboutView />
        )}
      </main>
    </div>
  );
}
