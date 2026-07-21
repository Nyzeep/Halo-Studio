import {
  AlertTriangle,
  ArrowLeft,
  Bot,
  CheckCircle,
  ChevronRight,
  Code2,
  ExternalLink,
  History,
  Info,
  Play,
  Sparkles,
  Terminal
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ConfigBackupEntry, ConfigRollbackRequest, ConfigWriteResult, RealConfigWritePlan } from "../../shared/config";
import type { AgentId, AgentInfo } from "../../shared/agents";
import { useMcpPreview } from "../hooks/useMcpPreview";

const workspaceRoot = "D:\\Halo Studio";

interface AgentConfigViewProps {
  agentId: AgentId;
  agents: AgentInfo[];
  onBack(): void;
  onLaunchAgent(agentId: AgentId): void;
  onTransitionToTab(tab: "dashboard" | "workspace" | "history" | "settings" | "about"): void;
}

const brandByAgent = {
  "claude-code": {
    name: "Claude Code",
    tagline: "Anthropic 本地 AI 编程 Agent 客户端",
    description: "适合代码理解、文件编辑和项目级任务执行。Halo Studio 会为它生成 Claude Code 可识别的项目级 MCP 配置。",
    icon: Bot,
    colorClass: "text-[#cc5a37]",
    bgGlow: "from-[#cc5a37]/15 via-red-950/5 to-transparent",
    badgeBg: "bg-[#cc5a37]/10 text-[#e06d48] border-[#cc5a37]/20",
    officialUrl: "https://anthropic.com/claude",
    configFormat: "Claude MCP JSON"
  },
  "codex-cli": {
    name: "Codex CLI",
    tagline: "OpenAI Codex 命令行编程助手",
    description: "适合终端内任务执行、代码生成和项目诊断。Halo Studio 会为它生成 `.codex/config.toml` 项目级 MCP 配置。",
    icon: Terminal,
    colorClass: "text-[#10b981]",
    bgGlow: "from-[#10b981]/15 via-emerald-950/5 to-transparent",
    badgeBg: "bg-[#10b981]/10 text-[#34d399] border-[#10b981]/20",
    officialUrl: "https://openai.com",
    configFormat: "Codex TOML"
  },
  opencode: {
    name: "OpenCode Agent",
    tagline: "开源代码 Agent 与本地工作流入口",
    description: "适合在开源 Agent 生态中探索本地编排。Halo Studio 会为它生成 `opencode.json` 项目级 MCP 配置。",
    icon: Code2,
    colorClass: "text-[#06b6d4]",
    bgGlow: "from-[#06b6d4]/15 via-cyan-950/5 to-transparent",
    badgeBg: "bg-[#06b6d4]/10 text-[#22d3ee] border-[#06b6d4]/20",
    officialUrl: "https://github.com/anomalyco/opencode",
    configFormat: "OpenCode JSON"
  },
  pi: {
    name: "Pi Agent",
    tagline: "用于探索式开发和协作式思考的 Agent",
    description: "适合把 Pi 作为温和的代码讨论与规划入口。Halo Studio 会为它生成 `.pi/mcp.json` 项目级 MCP 配置。",
    icon: Sparkles,
    colorClass: "text-[#f59e0b]",
    bgGlow: "from-[#f59e0b]/15 via-amber-950/5 to-transparent",
    badgeBg: "bg-[#f59e0b]/10 text-[#fbbf24] border-[#f59e0b]/20",
    officialUrl: "https://github.com/earendil-works/pi",
    configFormat: "Pi MCP JSON"
  }
} satisfies Record<AgentId, {
  name: string;
  tagline: string;
  description: string;
  icon: typeof Bot;
  colorClass: string;
  bgGlow: string;
  badgeBg: string;
  officialUrl: string;
  configFormat: string;
}>;

export function AgentConfigView({ agentId, agents, onBack, onLaunchAgent, onTransitionToTab }: AgentConfigViewProps) {
  const { previews, loading: mcpLoading, server } = useMcpPreview();
  const [writeResult, setWriteResult] = useState<ConfigWriteResult | null>(null);
  const [realWritePlan, setRealWritePlan] = useState<RealConfigWritePlan | null>(null);
  const [confirmation, setConfirmation] = useState("");
  const [backups, setBackups] = useState<ConfigBackupEntry[]>([]);
  const [busy, setBusy] = useState(false);

  const agentInfo = useMemo(() => agents.find((agent) => agent.id === agentId), [agents, agentId]);
  const brand = brandByAgent[agentId];
  const BrandIcon = brand.icon;
  const isReady = agentInfo?.status === "ready";
  const selectedPreview = useMemo(() => previews.find((preview) => preview.agentId === agentId), [previews, agentId]);
  const demoTargetPath = selectedPreview ? `${selectedPreview.agentId}-${selectedPreview.targetPath}` : "";

  useEffect(() => {
    if (!demoTargetPath) {
      setBackups([]);
      return;
    }

    let active = true;
    window.halo.config.listDemoBackups(demoTargetPath).then((result) => {
      if (active) setBackups(result);
    });
    return () => {
      active = false;
    };
  }, [demoTargetPath, writeResult]);

  useEffect(() => {
    if (!selectedPreview) {
      setRealWritePlan(null);
      setConfirmation("");
      return;
    }

    let active = true;
    window.halo.mcp.planProjectMcpWrite(workspaceRoot, selectedPreview).then((plan) => {
      if (active) {
        setRealWritePlan(plan);
        setConfirmation("");
      }
    });
    return () => {
      active = false;
    };
  }, [selectedPreview]);

  async function applyDemoWrite() {
    if (!selectedPreview) return;
    setBusy(true);
    try {
      const result = await window.halo.config.applyDemoWrite({
        targetPath: demoTargetPath,
        nextContent: selectedPreview.content,
        reason: `${selectedPreview.agentName} MCP 专属预演`
      });
      setWriteResult(result);
    } finally {
      setBusy(false);
    }
  }

  async function rollbackDemoWrite() {
    if (!writeResult) return;
    setBusy(true);
    try {
      const request: ConfigRollbackRequest = {
        targetPath: writeResult.targetPath,
        backupPath: writeResult.backupPath
      };
      await window.halo.config.rollbackWrite(request);
      setWriteResult(null);
    } finally {
      setBusy(false);
    }
  }

  async function rollbackBackup(backup: ConfigBackupEntry) {
    setBusy(true);
    try {
      await window.halo.config.rollbackWrite({
        targetPath: backup.targetPath,
        backupPath: backup.backupPath
      });
      setWriteResult(null);
    } finally {
      setBusy(false);
    }
  }

  async function applyRealWrite() {
    if (!selectedPreview || !realWritePlan) return;
    setBusy(true);
    try {
      const result = await window.halo.config.applyConfirmedWrite({
        workspaceRoot: realWritePlan.workspaceRoot,
        targetPath: realWritePlan.normalizedTargetPath,
        nextContent: selectedPreview.content,
        reason: realWritePlan.reason,
        confirmation
      });
      setWriteResult(result);
    } finally {
      setBusy(false);
    }
  }

  function launchSession() {
    onLaunchAgent(agentId);
    onTransitionToTab("workspace");
  }

  return (
    <div className="relative flex h-full w-full flex-col overflow-y-auto px-6 py-8 md:px-10 lg:px-14">
      <div className={`pointer-events-none absolute right-0 top-0 h-[500px] w-[500px] rounded-full bg-gradient-to-b ${brand.bgGlow} blur-[80px]`} />
      <div className="starfield opacity-40" />

      <div className="relative z-10 mb-8 flex items-center justify-between border-b border-white/5 pb-5">
        <button onClick={onBack} className="group inline-flex items-center gap-2 text-xs font-semibold text-slate-400 transition-colors hover:text-slate-100">
          <ArrowLeft size={14} className="transition-transform group-hover:-translate-x-1" />
          返回控制台大厅
        </button>
        <div className="flex items-center gap-2 text-[11px] font-medium text-slate-500">
          <span>Agent 编排</span>
          <ChevronRight size={10} />
          <span className="font-semibold text-purple-400">{brand.name}</span>
        </div>
      </div>

      <div className="relative z-10 grid grid-cols-1 items-start gap-8 lg:grid-cols-12">
        <div className="space-y-6 lg:col-span-5">
          <div className="glass-panel relative space-y-5 overflow-hidden rounded-2xl p-6">
            <div className="flex items-start justify-between gap-4">
              <div className="flex items-center gap-4">
                <div className="relative flex h-16 w-16 shrink-0 items-center justify-center overflow-hidden rounded-2xl border border-white/10 bg-slate-900">
                  <div className="absolute inset-0 bg-gradient-to-tr from-purple-500/10 to-indigo-500/10" />
                  <BrandIcon size={28} className={brand.colorClass} />
                </div>
                <div>
                  <h1 className="text-xl font-bold text-slate-100">{brand.name}</h1>
                  <span className={`mt-1.5 inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-[10px] font-bold ${
                    isReady ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-400" : "border-amber-500/20 bg-amber-500/10 text-amber-400"
                  }`}>
                    <span className="h-1.5 w-1.5 rounded-full bg-current" />
                    {isReady ? "已在本地就绪" : "本地未检测到"}
                  </span>
                </div>
              </div>
              <span className={`rounded-lg border px-2.5 py-1 text-[10px] font-bold uppercase tracking-wider ${brand.badgeBg}`}>
                {agentId}
              </span>
            </div>

            <div className="space-y-1">
              <div className="text-xs font-semibold text-slate-200">一句话简述</div>
              <p className="text-xs font-medium leading-relaxed text-slate-400">{brand.tagline}</p>
            </div>
            <p className="border-t border-white/5 pt-4 text-xs leading-relaxed text-slate-500">{brand.description}</p>

            <button onClick={launchSession} className="btn-cosmic-gradient flex w-full items-center justify-center gap-2.5 rounded-xl py-3.5 text-xs font-bold text-white">
              <Play size={14} className="fill-current" />
              {isReady ? `立即启动 ${brand.name} 交互会话` : `启动 ${brand.name} 模拟会话`}
            </button>
          </div>

          <div className="glass-panel-soft space-y-4 rounded-2xl p-5">
            <h3 className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-slate-300">
              <Info size={14} className="text-purple-400" />
              运行环境配置
            </h3>
            <div className="space-y-3 font-mono text-[11px]">
              <div className="flex items-center justify-between border-b border-white/5 py-1.5">
                <span className="text-slate-500">二进制调用命令</span>
                <span className="rounded bg-white/5 px-2 py-0.5 text-[10px] text-slate-300">{agentInfo?.command ?? agentId}</span>
              </div>
              <div className="flex items-center justify-between border-b border-white/5 py-1.5">
                <span className="text-slate-500">当前版本</span>
                <span className="font-semibold text-purple-300">{agentInfo?.version ?? "未知或未安装"}</span>
              </div>
              <div className="flex items-center justify-between border-b border-white/5 py-1.5">
                <span className="text-slate-500">连接模式</span>
                <span className="flex gap-1 text-slate-300">
                  {(agentInfo?.modes ?? ["config-only"]).map((mode) => (
                    <span key={mode} className="rounded bg-purple-500/10 px-1.5 py-0.5 text-[9px] font-bold uppercase text-purple-400">
                      {mode}
                    </span>
                  ))}
                </span>
              </div>
            </div>
            {!isReady && (
              <div className="space-y-2 rounded-xl border border-amber-500/20 bg-amber-500/5 p-3.5">
                <div className="flex items-center gap-2 text-xs font-bold text-amber-400">
                  <AlertTriangle size={14} />
                  安装与调用指南
                </div>
                <p className="text-[11px] leading-relaxed text-slate-400">
                  {agentInfo?.installHint || "当前未检测到本地 CLI。你仍可启动模拟会话，用于验证界面和 MCP 配置流程。"}
                </p>
                <a href={brand.officialUrl} target="_blank" rel="noopener noreferrer" className="inline-flex items-center gap-1 text-[10px] font-bold text-purple-400 hover:text-purple-300">
                  访问官方主页
                  <ExternalLink size={10} />
                </a>
              </div>
            )}
          </div>
        </div>

        <div className="space-y-6 lg:col-span-7">
          <div className="glass-panel space-y-5 rounded-2xl p-6">
            <div className="flex items-center justify-between gap-4">
              <div className="space-y-1">
                <h2 className="text-sm font-bold text-slate-200">MCP 插件编排配置</h2>
                <p className="text-xs text-slate-500">Halo Studio 将自动为 {brand.name} 生成独有的 MCP 配置块。</p>
              </div>
              <span className="rounded-full border border-purple-500/20 bg-purple-500/10 px-2.5 py-0.5 text-[9px] font-bold text-purple-300">
                {brand.configFormat}
              </span>
            </div>

            <div className="space-y-2 rounded-xl border border-white/5 bg-white/5 p-4 text-xs">
              <div className="flex items-center justify-between">
                <span className="font-bold text-slate-400">主数据服务器: {server.displayName}</span>
                <span className="rounded border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-bold text-emerald-400">ACTIVE</span>
              </div>
              <div className="break-all rounded-lg border border-white/5 bg-[#070512] p-2.5 font-mono text-[10px] leading-relaxed text-slate-500">
                <span className="text-purple-400">Command:</span> {server.command} {server.args?.join(" ")}
              </div>
            </div>

            {mcpLoading ? (
              <div className="rounded-xl border border-white/5 bg-white/5 p-6 text-center text-xs text-slate-500">生成 MCP 配置预览中...</div>
            ) : selectedPreview ? (
              <div className="space-y-5">
                <div className="overflow-hidden rounded-xl border border-white/5 bg-[#070512]">
                  <div className="flex items-center justify-between border-b border-white/5 bg-[#0a0815]/50 px-3.5 py-2.5 font-mono text-[10px] text-slate-500">
                    <span className="truncate font-semibold text-slate-400">{selectedPreview.targetPath}</span>
                    <span className="rounded bg-purple-900/10 px-2 py-0.5 text-[9px] font-bold uppercase text-purple-400">{selectedPreview.language}</span>
                  </div>
                  <pre className="max-h-60 overflow-auto p-4 font-mono text-[10px] leading-relaxed text-slate-400">
                    <code>{selectedPreview.content}</code>
                  </pre>
                </div>

                <div className="space-y-4 rounded-xl border border-white/5 bg-white/5 p-5">
                  <div>
                    <h3 className="text-xs font-bold text-slate-200">安全沙箱预演</h3>
                    <p className="mt-1 text-[11px] text-slate-500">先写入独立演示文件，验证 diff、备份与回滚流程。</p>
                  </div>
                  <div className="space-y-1 rounded-lg border border-white/5 bg-[#070512] p-3 font-mono text-[10px] text-slate-500">
                    <div className="text-[9px] font-bold uppercase text-slate-600">模拟宿主写入路径</div>
                    <div className="break-all font-semibold text-slate-400">{demoTargetPath}</div>
                  </div>
                  <div className="flex gap-2.5">
                    <button disabled={busy} onClick={() => void applyDemoWrite()} className="btn-cosmic-gradient flex-1 rounded-xl py-2.5 text-xs font-semibold text-white disabled:pointer-events-none disabled:opacity-40">
                      安全写入演示
                    </button>
                    <button disabled={busy || !writeResult} onClick={() => void rollbackDemoWrite()} className="rounded-xl border border-white/10 bg-white/5 px-4 py-2.5 text-xs font-semibold text-slate-300 hover:bg-white/10 disabled:pointer-events-none disabled:opacity-30">
                      安全回滚
                    </button>
                  </div>
                  {writeResult && (
                    <div className="mt-4 space-y-2 border-t border-white/5 pt-4">
                      <div className="flex items-center gap-1.5 text-xs font-bold text-emerald-400">
                        <CheckCircle size={14} />
                        演示写入成功
                      </div>
                      <div className="break-all font-mono text-[9px] text-slate-500">目标: {writeResult.targetPath}</div>
                      <div className="break-all font-mono text-[9px] text-slate-500">备份: {writeResult.backupPath}</div>
                      <pre className="max-h-40 overflow-auto rounded-lg border border-purple-500/10 bg-[#070512] p-3 font-mono text-[9px] leading-normal text-purple-300">
                        <code>{writeResult.diff}</code>
                      </pre>
                    </div>
                  )}
                </div>

                {realWritePlan && (
                  <div className="space-y-4 rounded-xl border border-purple-500/15 bg-purple-500/5 p-5">
                    <div className="flex items-center justify-between gap-2">
                      <div className="text-xs font-bold uppercase tracking-wider text-slate-200">项目配置真实写入预案</div>
                      <span className={`rounded border px-2.5 py-0.5 text-[9px] font-bold ${
                        realWritePlan.allowed ? "border-emerald-500/20 bg-emerald-500/10 text-emerald-400" : "border-red-500/20 bg-red-500/10 text-red-400"
                      }`}>
                        风险等级: {realWritePlan.risk}
                      </span>
                    </div>
                    <div className="break-all rounded-lg border border-white/5 bg-black/35 p-2.5 font-mono text-xs text-slate-400">
                      <span className="text-slate-500">目标路径：</span>
                      {realWritePlan.normalizedTargetPath}
                    </div>
                    {realWritePlan.warnings.length > 0 ? (
                      <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-3 text-xs font-semibold text-red-400">
                        {realWritePlan.warnings.join(" ")}
                      </div>
                    ) : (
                      <div className="rounded-lg border border-emerald-500/20 bg-emerald-500/5 p-3 text-xs font-medium leading-relaxed text-emerald-400">
                        目标位于当前工作区内。当前会写入完整生成文件，结构化合并将在后续阶段加入。
                      </div>
                    )}
                    <label className="block space-y-2 text-xs">
                      <span className="font-semibold text-slate-300">安全授权验证短语</span>
                      <input
                        className="w-full rounded-xl border border-white/10 bg-[#070512] px-3.5 py-2.5 text-slate-200 outline-none focus:border-purple-500"
                        value={confirmation}
                        onChange={(event) => setConfirmation(event.target.value)}
                        placeholder={`请输入 ${realWritePlan.confirmationPhrase}`}
                      />
                    </label>
                    <button
                      className="w-full rounded-xl bg-purple-600 py-3 text-xs font-bold text-white transition-all hover:bg-purple-500 disabled:pointer-events-none disabled:opacity-35"
                      disabled={busy || !realWritePlan.allowed || confirmation !== realWritePlan.confirmationPhrase}
                      onClick={() => void applyRealWrite()}
                    >
                      确认写入项目配置
                    </button>
                  </div>
                )}

                <div className="space-y-4 rounded-xl border border-white/5 bg-white/5 p-5">
                  <div className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-slate-200">
                    <History size={14} className="text-purple-400" />
                    安全备份与回滚历史
                  </div>
                  {backups.length === 0 ? (
                    <p className="text-xs leading-normal text-slate-500">暂无此 Agent 的演示备份记录。</p>
                  ) : (
                    <div className="max-h-56 space-y-3 overflow-y-auto pr-1">
                      {backups.map((backup) => (
                        <div key={backup.backupPath} className="space-y-3 rounded-xl border border-white/5 bg-[#070512] p-3">
                          <div className="break-all font-mono text-[10px] leading-relaxed text-slate-500">{backup.backupPath}</div>
                          <div className="flex items-center justify-between gap-2 border-t border-white/5 pt-2">
                            <span className="font-mono text-[10px] text-slate-500">{new Date(backup.createdAt).toLocaleString()} · {backup.size} B</span>
                            <button disabled={busy} onClick={() => void rollbackBackup(backup)} className="rounded-lg border border-white/10 bg-white/5 px-3 py-1 text-xs font-semibold text-slate-300 hover:bg-white/10 disabled:pointer-events-none disabled:opacity-40">
                              恢复
                            </button>
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            ) : (
              <p className="text-xs leading-relaxed text-slate-500">未能为该 Agent 找到 MCP 配置预览。</p>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
