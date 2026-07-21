import React, { useState } from "react";
import { Settings, FolderOpen, Shield, ShieldAlert, KeyRound, Bot, Check, RotateCcw, AlertTriangle } from "lucide-react";
import type { AgentInfo } from "../../shared/agents";

interface SettingsViewProps {
  agents: AgentInfo[];
  loading: boolean;
  onRefreshDiscovery(): void;
}

export function SettingsView({ agents, loading, onRefreshDiscovery }: SettingsViewProps) {
  const [workspacePath, setWorkspacePath] = useState("D:\\Halo Studio");
  const [writeGuardEnabled, setWriteGuardEnabled] = useState(true);
  const [maxBackups, setMaxBackups] = useState(10);
  const [autoDetect, setAutoDetect] = useState(true);

  return (
    <div className="relative flex h-full w-full flex-col overflow-y-auto px-6 py-10 md:px-10 lg:px-14">
      <div className="starfield" />

      <div className="relative z-10 max-w-4xl space-y-8">
        {/* Page Header */}
        <div className="flex items-center gap-3">
          <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-purple-500/10 text-purple-400">
            <Settings size={24} />
          </div>
          <div>
            <h1 className="text-2xl font-bold tracking-tight text-slate-100">Halo-Studio 配置中心</h1>
            <p className="text-xs text-slate-400">管理本地 Agent 环境变量、安全写入权限与 MCP 注册配置</p>
          </div>
        </div>

        {/* 2-Column Grid */}
        <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
          {/* Main settings column */}
          <div className="md:col-span-2 space-y-6">
            {/* 1. Workspace Configuration */}
            <div className="glass-panel rounded-2xl p-6 space-y-4">
              <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-200">
                <FolderOpen size={16} className="text-purple-400" />
                本地开发工作区
              </h2>
              <div className="space-y-2">
                <label className="block text-xs text-slate-400">默认工作目录路径 (Workspace Root)</label>
                <div className="flex gap-2">
                  <input
                    type="text"
                    className="flex-1 rounded-xl border border-white/5 bg-[#090712] px-3.5 py-2.5 text-xs text-slate-300 outline-none focus:border-purple-500/40"
                    value={workspacePath}
                    onChange={(e) => setWorkspacePath(e.target.value)}
                  />
                  <button className="rounded-xl border border-white/10 bg-white/5 px-4 text-xs font-semibold hover:bg-white/10 text-slate-200">
                    浏览
                  </button>
                </div>
                <p className="text-[11px] text-slate-500">所有的 PTY 子进程、MCP 配置更新及代码 diff 将在这个根目录进行定位与修改。</p>
              </div>
            </div>

            {/* 2. Write Guard & Security policies */}
            <div className="glass-panel rounded-2xl p-6 space-y-4">
              <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-200">
                <Shield size={16} className="text-purple-400" />
                安全写入守护与防抖 (Write Guard Mode)
              </h2>

              <div className="space-y-4">
                <div className="flex items-center justify-between">
                  <div>
                    <div className="text-xs font-medium text-slate-200">启用安全二次确认守护 (Write Guard)</div>
                    <p className="text-[11px] text-slate-500 mt-0.5">当 Agent 请求写入本地配置文件时，需验证指定的短语。</p>
                  </div>
                  <button
                    onClick={() => setWriteGuardEnabled(!writeGuardEnabled)}
                    className={`relative inline-flex h-6 w-11 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none ${
                      writeGuardEnabled ? "bg-purple-600" : "bg-slate-700"
                    }`}
                  >
                    <span
                      className={`pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out ${
                        writeGuardEnabled ? "translate-x-5" : "translate-x-0"
                      }`}
                    />
                  </button>
                </div>

                <div className="grid grid-cols-2 gap-4">
                  <div className="space-y-2">
                    <label className="block text-xs text-slate-400">保留备份历史数量</label>
                    <input
                      type="number"
                      className="w-full rounded-xl border border-white/5 bg-[#090712] px-3.5 py-2.5 text-xs text-slate-300 outline-none focus:border-purple-500/40"
                      value={maxBackups}
                      onChange={(e) => setMaxBackups(Number(e.target.value))}
                    />
                  </div>
                  <div className="space-y-2">
                    <label className="block text-xs text-slate-400">防抖检测时钟周期 (ms)</label>
                    <input
                      type="number"
                      className="w-full rounded-xl border border-white/5 bg-[#090712] px-3.5 py-2.5 text-xs text-slate-300 outline-none focus:border-purple-500/40"
                      defaultValue={1500}
                    />
                  </div>
                </div>

                <div className="rounded-xl border border-amber-500/15 bg-amber-500/5 p-3.5 flex gap-3 text-xs leading-relaxed text-amber-300">
                  <AlertTriangle size={16} className="shrink-0 text-amber-400 mt-0.5" />
                  <div>
                    <span className="font-semibold text-amber-200">警告：</span>
                    当前处于 [演示防误写模式]。点击 “写入项目配置” 会校验工作区路径，禁止改动工作区外的任何敏感系统文件。
                  </div>
                </div>
              </div>
            </div>

            {/* 3. Credentials & Keys */}
            <div className="glass-panel rounded-2xl p-6 space-y-4">
              <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-200">
                <KeyRound size={16} className="text-purple-400" />
                凭据服务与 API 密钥
              </h2>
              <div className="space-y-3">
                <div className="flex items-center justify-between rounded-xl border border-white/5 bg-white/5 p-3 text-xs">
                  <span className="text-slate-300">GEMINI_API_KEY (Server Side)</span>
                  <span className="rounded bg-emerald-500/10 px-2 py-0.5 text-emerald-400 text-[10px]">系统就绪</span>
                </div>
                <div className="flex items-center justify-between rounded-xl border border-white/5 bg-white/5 p-3 text-xs">
                  <span className="text-slate-300">OPENAI_API_KEY</span>
                  <span className="rounded bg-slate-500/10 px-2 py-0.5 text-slate-400 text-[10px]">未设置</span>
                </div>
                <p className="text-[11px] text-slate-500">API 秘钥由 AI Studio 环境内置托管，服务器安全调用，不会泄露给前端浏览器。</p>
              </div>
            </div>
          </div>

          {/* Right sidebar info column */}
          <div className="space-y-6">
            {/* Agent auto discovery status */}
            <div className="glass-panel rounded-2xl p-5 space-y-4">
              <div className="flex items-center justify-between">
                <h3 className="text-xs font-bold text-slate-200 uppercase tracking-wider">自动扫描引擎</h3>
                <button
                  onClick={onRefreshDiscovery}
                  disabled={loading}
                  className="text-xs text-purple-400 hover:text-purple-300 disabled:opacity-50"
                >
                  立即刷新
                </button>
              </div>

              <div className="space-y-2 max-h-64 overflow-y-auto pr-1">
                {agents.map((agent) => (
                  <div key={agent.id} className="rounded-xl border border-white/5 bg-[#090712] p-3 text-xs flex justify-between items-center">
                    <div>
                      <div className="font-semibold text-slate-300">{agent.name}</div>
                      <div className="text-[10px] text-slate-500 font-mono mt-0.5">{agent.command}</div>
                    </div>
                    {agent.status === "ready" ? (
                      <span className="rounded bg-emerald-500/10 px-2 py-0.5 text-emerald-400 text-[10px] flex items-center gap-1">
                        <Check size={10} />
                        就绪
                      </span>
                    ) : (
                      <span className="rounded bg-amber-500/10 px-2 py-0.5 text-amber-400 text-[10px]">
                        未检出
                      </span>
                    )}
                  </div>
                ))}
              </div>

              <div className="rounded-xl border border-white/5 bg-[#0c0a1a] p-3 text-xs space-y-2">
                <div className="text-purple-300 font-semibold flex items-center gap-1">
                  <Bot size={13} />
                  Agent 自动探测规则
                </div>
                <p className="text-[11px] text-slate-500 leading-relaxed">
                  Halo Studio 会通过执行环境的 `where`/`which` 指令实时查询系统的 PATH 变量，自动绑定可执行子进程。
                </p>
              </div>
            </div>

            {/* About App quick summary widget */}
            <div className="glass-panel rounded-2xl p-5 space-y-3">
              <div className="text-xs font-bold text-slate-200 uppercase tracking-wider">系统版本信息</div>
              <div className="text-xs text-slate-400 space-y-1.5">
                <div className="flex justify-between"><span className="text-slate-500">主程序版本</span><span className="font-mono">v0.2.0</span></div>
                <div className="flex justify-between"><span className="text-slate-500">控制层协议</span><span className="font-mono text-purple-300">MCP v1.0</span></div>
                <div className="flex justify-between"><span className="text-slate-500">许可协议</span><span className="font-mono">MIT</span></div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
