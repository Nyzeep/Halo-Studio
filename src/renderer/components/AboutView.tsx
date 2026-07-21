import { ArrowUpRight, Cpu, Github, Heart, Info } from "lucide-react";

export function AboutView() {
  return (
    <div className="relative flex h-full w-full flex-col overflow-y-auto px-6 py-10 md:px-10 lg:px-14">
      <div className="starfield" />
      <div className="cosmic-planet-container opacity-40">
        <div className="cosmic-nebula-glow" />
        <div className="planet-rings-back" />
        <div className="planet-sphere" />
        <div className="planet-rings-front" />
      </div>

      <div className="relative z-10 my-auto max-w-3xl space-y-8">
        <div className="space-y-4">
          <div className="inline-flex items-center gap-2 rounded-full border border-purple-500/20 bg-purple-500/10 px-3.5 py-1 text-xs font-semibold text-purple-300">
            <Heart size={12} className="text-purple-400" />
            关于项目与作者
          </div>
          <h1 className="text-cosmic-glow bg-gradient-to-r from-violet-200 via-indigo-200 to-purple-300 bg-clip-text text-3xl font-extrabold tracking-tight text-transparent sm:text-4xl">
            关于 Halo Studio
          </h1>
          <p className="max-w-xl text-xs leading-relaxed text-slate-400">
            Halo Studio 是一个面向本地 AI 编程 Agent 的桌面工作台。它把终端会话、MCP 配置预览、安全写入和备份回滚放在同一个清晰的界面里，目标是让多 Agent 开发变得顺手、透明、可控。
          </p>
        </div>

        <div className="glass-panel flex flex-col items-center gap-6 rounded-2xl p-6 md:flex-row md:p-8">
          <div className="relative shrink-0">
            <div className="absolute inset-0 rounded-full bg-gradient-to-tr from-purple-600 to-indigo-600 opacity-70 blur-md" />
            <div className="relative flex h-24 w-24 items-center justify-center rounded-full border-2 border-purple-500/40 bg-[#0c0a1c] text-lg font-bold tracking-widest text-purple-200">
              NYZEEP
            </div>
          </div>

          <div className="flex-1 space-y-4 text-center md:text-left">
            <div>
              <h2 className="text-lg font-bold text-slate-100">Nyzeep</h2>
              <p className="mt-0.5 text-xs font-medium text-purple-400">Halo Studio 核心作者 / 主理人</p>
            </div>
            <p className="max-w-md text-xs leading-relaxed text-slate-400">
              这个项目正在探索一种更适合本地开发的多 Agent 工作方式：既保留 CLI 的直接感，也提供桌面应用应有的安全、状态和可视化。
            </p>
            <a
              href="https://github.com/Nyzeep/Halo-Studio"
              target="_blank"
              rel="noopener noreferrer"
              className="btn-cosmic-gradient inline-flex items-center gap-2 rounded-xl px-4 py-2.5 text-xs font-semibold text-white"
            >
              <Github size={14} />
              访问 GitHub 仓库
              <ArrowUpRight size={12} className="opacity-70" />
            </a>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <div className="glass-panel-soft space-y-2 rounded-2xl p-5">
            <h3 className="flex items-center gap-2 text-xs font-bold text-slate-200">
              <Cpu size={14} className="text-purple-400" />
              架构与技术栈
            </h3>
            <p className="text-[11px] leading-relaxed text-slate-400">
              Electron、React、TypeScript、xterm.js 与 node-pty 构成当前桌面工作台核心，配置写入层内建 diff、备份、原子替换和确认守卫。
            </p>
          </div>
          <div className="glass-panel-soft space-y-2 rounded-2xl p-5">
            <h3 className="flex items-center gap-2 text-xs font-bold text-slate-200">
              <Info size={14} className="text-cyan-400" />
              当前发布状态
            </h3>
            <p className="text-[11px] leading-relaxed text-slate-400">
              当前版本为 <span className="font-mono font-semibold text-purple-300">v0.2.0-beta</span>。重点验证 Windows 本地桌面壳、多 Agent 会话、MCP 预览和项目级写入预案。
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
