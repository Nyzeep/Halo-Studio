import { ArrowRight, Bot, Code2, Command, Image, Mic, Sparkles, Terminal } from "lucide-react";
import { useState } from "react";
import type { AgentId, AgentInfo } from "../../shared/agents";
import { AgentConfigView } from "./AgentConfigView";

interface DashboardViewProps {
  agents: AgentInfo[];
  loading: boolean;
  onLaunchAgent(agentId: AgentId): void;
  onTransitionToTab(tab: "dashboard" | "workspace" | "history" | "settings" | "about"): void;
}

const agentCards = [
  { id: "claude-code" as const, name: "Claude Code", icon: Bot, color: "text-[#cc5a37]", hover: "hover:border-[#cc5a37]/30 hover:bg-[#cc5a37]/5" },
  { id: "codex-cli" as const, name: "Codex CLI", icon: Command, color: "text-[#10b981]", hover: "hover:border-[#10b981]/30 hover:bg-[#10b981]/5" },
  { id: "opencode" as const, name: "OpenCode Agent", icon: Code2, color: "text-[#06b6d4]", hover: "hover:border-[#06b6d4]/30 hover:bg-[#06b6d4]/5" },
  { id: "pi" as const, name: "Pi Agent", icon: Sparkles, color: "text-[#f59e0b]", hover: "hover:border-[#f59e0b]/30 hover:bg-[#f59e0b]/5" }
];

export function DashboardView({ agents, loading, onLaunchAgent, onTransitionToTab }: DashboardViewProps) {
  const [promptInput, setPromptInput] = useState("");
  const [selectedAgentId, setSelectedAgentId] = useState<AgentId | null>(null);
  const [answers, setAnswers] = useState<Array<{ q: string; a: string }>>([]);
  const [isGenerating, setIsGenerating] = useState(false);

  function handleGenerate() {
    const prompt = promptInput.trim();
    if (!prompt) {
      return;
    }

    setPromptInput("");
    setIsGenerating(true);
    window.setTimeout(() => {
      const lower = prompt.toLowerCase();
      let nextAgent: AgentId | null = null;
      if (lower.includes("claude")) nextAgent = "claude-code";
      if (lower.includes("codex")) nextAgent = "codex-cli";
      if (lower.includes("opencode")) nextAgent = "opencode";
      if (lower.includes("pi")) nextAgent = "pi";

      if (nextAgent) {
        setSelectedAgentId(nextAgent);
      }
      setAnswers((current) => [
        ...current,
        {
          q: prompt,
          a: nextAgent
            ? "已为你打开对应 Agent 的配置与启动面板，可以先预览 MCP 配置，也可以直接启动会话。"
            : "已收到你的指令。建议从右上角选择一个 Agent，进入专属配置页后再启动会话。"
        }
      ]);
      setIsGenerating(false);
    }, 500);
  }

  if (selectedAgentId) {
    return (
      <AgentConfigView
        agentId={selectedAgentId}
        agents={agents}
        onBack={() => setSelectedAgentId(null)}
        onLaunchAgent={onLaunchAgent}
        onTransitionToTab={onTransitionToTab}
      />
    );
  }

  return (
    <div className="relative flex h-full w-full flex-col overflow-y-auto px-6 py-8 md:px-10 lg:px-14">
      <div className="starfield" />
      <div className="cosmic-planet-container">
        <div className="cosmic-nebula-glow" />
        <div className="planet-rings-back" />
        <div className="planet-sphere" />
        <div className="planet-rings-front" />
      </div>

      <div className="relative z-20 mb-6 flex shrink-0 flex-col items-start justify-between gap-4 border-b border-white/5 pb-5 md:flex-row md:items-center">
        <div className="flex items-center gap-2.5">
          <div className="flex h-6 w-6 items-center justify-center rounded-lg bg-purple-500/10 text-purple-400 cosmic-glow-border-active">
            <Sparkles size={12} className="text-purple-300" />
          </div>
          <span className="text-xs font-bold uppercase tracking-wider text-slate-400">Halo-Studio 工作台主页</span>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <span className="mr-1 hidden text-[10px] font-bold text-slate-500 lg:inline">启动与配置通道:</span>
          {agentCards.map((item) => {
            const Icon = item.icon;
            const isReady = agents.find((agent) => agent.id === item.id)?.status === "ready";
            return (
              <button
                key={item.id}
                onClick={() => setSelectedAgentId(item.id)}
                className={`flex items-center gap-2 rounded-xl border border-white/5 bg-white/5 px-3.5 py-2 text-xs font-semibold text-slate-300 transition-all duration-300 ${item.hover}`}
              >
                <Icon size={14} className={item.color} />
                <span>{item.name}</span>
                <span className={`h-1.5 w-1.5 rounded-full ${isReady ? "bg-emerald-500" : "bg-amber-500"}`} />
              </button>
            );
          })}
        </div>
      </div>

      <div className="relative z-10 my-auto flex max-w-3xl flex-col space-y-8">
        <div className="space-y-4">
          <div className="inline-flex items-center gap-2 rounded-full border border-purple-500/20 bg-purple-500/10 px-3.5 py-1 text-xs font-semibold text-purple-300">
            <Sparkles size={12} className="text-purple-400" />
            Halo-Studio v0.2.0 · Cosmic Release
          </div>
          <h1 className="text-cosmic-glow bg-gradient-to-r from-violet-200 via-indigo-200 to-purple-300 bg-clip-text text-4xl font-extrabold leading-tight tracking-tight text-transparent sm:text-5xl lg:text-6xl">
            Welcome to Halo-Studio AI Agent...
          </h1>
          <p className="max-w-xl text-sm leading-relaxed text-slate-400">
            本地 AI 编程 Agent 统一桌面工作台与编排中心。选择任意 Agent 进入专属面板，预览 MCP 配置、安全写入项目配置，或直接开启本地终端会话。
          </p>
        </div>

        {answers.length > 0 && (
          <div className="max-h-48 space-y-3 overflow-y-auto pr-2">
            {answers.map((item, index) => (
              <div key={`${item.q}-${index}`} className="glass-panel space-y-1 rounded-xl p-3 text-xs">
                <div className="font-semibold text-purple-300">Q: {item.q}</div>
                <div className="text-slate-300">A: {item.a}</div>
              </div>
            ))}
          </div>
        )}

        <div className="glass-input relative flex flex-col rounded-2xl p-4">
          <textarea
            className="h-16 w-full resize-none bg-transparent text-sm text-slate-100 outline-none placeholder:text-slate-500"
            placeholder="Ask me anything or start with a prompt... 例如：启动 claude"
            value={promptInput}
            onChange={(event) => setPromptInput(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                handleGenerate();
              }
            }}
          />
          <div className="mt-3 flex items-center justify-between border-t border-white/5 pt-3">
            <div className="flex items-center gap-3 text-slate-400">
              <button className="rounded-lg p-1.5 transition-colors hover:bg-white/5 hover:text-purple-300">
                <Mic size={18} />
              </button>
              <button className="rounded-lg p-1.5 transition-colors hover:bg-white/5 hover:text-purple-300">
                <Image size={18} />
              </button>
              <span className="hidden text-[11px] text-slate-600 sm:inline">按 Enter 发送</span>
            </div>
            <button
              onClick={handleGenerate}
              disabled={isGenerating || !promptInput.trim()}
              className="btn-cosmic-gradient inline-flex items-center gap-2 rounded-xl px-4 py-2 text-xs font-semibold text-white disabled:pointer-events-none disabled:opacity-40"
            >
              {isGenerating ? "正在生成..." : "Generate"}
              <ArrowRight size={14} />
            </button>
          </div>
        </div>

        <div className="flex flex-wrap gap-2">
          {agentCards.map((item) => (
            <button
              key={item.id}
              onClick={() => setSelectedAgentId(item.id)}
              className="rounded-full border border-white/5 bg-white/5 px-4 py-2 text-xs text-slate-300 transition-all duration-200 hover:border-purple-500/30 hover:bg-purple-500/10"
            >
              Config & Launch {item.name}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
