import { ChevronRight, FileCode2, History, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ConfigBackupEntry, ConfigRollbackRequest, ConfigWriteResult, RealConfigWritePlan } from "../../shared/config";
import { useMcpPreview } from "../hooks/useMcpPreview";

const workspaceRoot = "D:\\Halo Studio";

export function McpPreviewPanel() {
  const { previews, loading, server } = useMcpPreview();
  const [selectedAgentId, setSelectedAgentId] = useState<string>("codex-cli");
  const [writeResult, setWriteResult] = useState<ConfigWriteResult | null>(null);
  const [realWritePlan, setRealWritePlan] = useState<RealConfigWritePlan | null>(null);
  const [confirmation, setConfirmation] = useState("");
  const [backups, setBackups] = useState<ConfigBackupEntry[]>([]);
  const [busy, setBusy] = useState(false);

  const selectedPreview = useMemo(
    () => previews.find((preview) => preview.agentId === selectedAgentId) ?? previews[0],
    [previews, selectedAgentId]
  );
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
        reason: `${selectedPreview.agentName} MCP 预览`
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

  return (
    <section className="space-y-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-slate-400">
          <FileCode2 size={14} className="text-purple-400" />
          MCP 配置预览
        </div>
        <span className="rounded border border-purple-500/20 bg-purple-500/10 px-2 py-0.5 text-[9px] font-bold text-purple-300">只读预览</span>
      </div>

      <div className="space-y-1.5 rounded-xl border border-white/5 bg-white/5 p-3.5">
        <div className="text-xs font-bold text-slate-200">{server.displayName}</div>
        <div className="break-all font-mono text-[10px] leading-normal text-slate-500">{server.command} {server.args?.join(" ")}</div>
        <div className="flex items-center gap-1.5 pt-1 text-[10px] text-slate-400">
          <ShieldCheck size={12} className="text-emerald-400" />
          <span>演示写入不会污染真实工作区</span>
        </div>
      </div>

      {loading ? (
        <div className="rounded-xl border border-white/5 bg-white/5 p-4 text-xs text-slate-500">生成预览中...</div>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-1.5">
            {previews.map((preview) => {
              const isSelected = preview.agentId === selectedPreview?.agentId;
              return (
                <button
                  key={preview.agentId}
                  className={`flex items-center justify-between rounded-lg border px-2.5 py-1.5 text-left text-[10px] font-semibold transition-all ${
                    isSelected ? "border-purple-500/30 bg-purple-500/10 text-purple-300" : "border-white/5 bg-white/5 text-slate-400 hover:border-white/10 hover:text-slate-200"
                  }`}
                  onClick={() => setSelectedAgentId(preview.agentId)}
                >
                  <span className="truncate pr-1">{preview.agentName}</span>
                  <ChevronRight size={10} className="shrink-0 opacity-60" />
                </button>
              );
            })}
          </div>

          {selectedPreview && (
            <div className="space-y-4">
              <div className="overflow-hidden rounded-xl border border-white/5 bg-[#070512]">
                <div className="flex items-center justify-between border-b border-white/5 bg-[#0a0815]/50 px-3.5 py-2 font-mono text-[10px] text-slate-500">
                  <span className="truncate">{selectedPreview.targetPath}</span>
                  <span className="font-bold uppercase text-purple-400">{selectedPreview.language}</span>
                </div>
                <pre className="max-h-48 overflow-auto p-3.5 font-mono text-[10px] leading-relaxed text-slate-400">
                  <code>{selectedPreview.content}</code>
                </pre>
              </div>

              <div className="space-y-3 rounded-xl border border-white/5 bg-white/5 p-4">
                <div className="rounded-lg border border-white/5 bg-[#070512] px-3 py-2 font-mono text-[10px] text-slate-500">
                  <div className="text-[9px] font-bold uppercase text-slate-600">演示写入目标</div>
                  <div className="mt-1 break-all text-slate-400">{demoTargetPath}</div>
                </div>
                <div className="flex gap-2">
                  <button disabled={busy} onClick={() => void applyDemoWrite()} className="btn-cosmic-gradient flex-1 rounded-xl py-2 text-xs font-semibold text-white disabled:pointer-events-none disabled:opacity-40">
                    写入演示文件
                  </button>
                  <button disabled={busy || !writeResult} onClick={() => void rollbackDemoWrite()} className="rounded-xl border border-white/10 bg-white/5 px-3 py-2 text-xs font-semibold text-slate-300 hover:bg-white/10 disabled:pointer-events-none disabled:opacity-30">
                    回滚
                  </button>
                </div>
                {writeResult && (
                  <div className="mt-3.5 space-y-2 border-t border-white/5 pt-3">
                    <div className="break-all font-mono text-[9px] text-slate-500">写入: {writeResult.targetPath}</div>
                    <div className="break-all font-mono text-[9px] text-slate-500">备份: {writeResult.backupPath}</div>
                    <pre className="max-h-36 overflow-auto rounded-lg bg-[#070512] p-2.5 font-mono text-[9px] leading-normal text-purple-300">
                      <code>{writeResult.diff}</code>
                    </pre>
                  </div>
                )}
              </div>

              {realWritePlan && (
                <div className="space-y-3 rounded-xl border border-purple-500/15 bg-purple-500/5 p-4">
                  <div className="flex items-center justify-between gap-2">
                    <div className="text-[10px] font-bold uppercase tracking-wider text-slate-200">项目配置真实写入预案</div>
                    <span className={`rounded px-2 py-0.5 text-[9px] font-bold ${realWritePlan.allowed ? "bg-emerald-500/10 text-emerald-400" : "bg-red-500/10 text-red-400"}`}>
                      {realWritePlan.risk}
                    </span>
                  </div>
                  <div className="break-all font-mono text-[10px] text-slate-500">目标：{realWritePlan.normalizedTargetPath}</div>
                  {realWritePlan.warnings.length > 0 ? (
                    <div className="rounded-lg border border-red-500/20 bg-red-500/5 p-2.5 text-[10px] text-red-400">
                      {realWritePlan.warnings.join(" ")}
                    </div>
                  ) : (
                    <div className="rounded-lg border border-emerald-500/20 bg-emerald-500/5 p-2.5 text-[10px] text-emerald-400">
                      目标位于当前工作区内。当前会写入完整生成文件。
                    </div>
                  )}
                  <input
                    className="w-full rounded-lg border border-white/5 bg-[#070512] px-3 py-2 text-[10px] text-slate-200 outline-none focus:border-purple-500"
                    value={confirmation}
                    onChange={(event) => setConfirmation(event.target.value)}
                    placeholder={realWritePlan.confirmationPhrase}
                  />
                  <button
                    className="w-full rounded-xl bg-purple-600 py-2 text-xs font-bold text-white transition-all hover:bg-purple-500 disabled:pointer-events-none disabled:opacity-35"
                    disabled={busy || !realWritePlan.allowed || confirmation !== realWritePlan.confirmationPhrase}
                    onClick={() => void applyRealWrite()}
                  >
                    确认写入项目配置
                  </button>
                </div>
              )}

              <div className="space-y-3 rounded-xl border border-white/5 bg-white/5 p-4">
                <div className="flex items-center gap-2 text-[10px] font-bold uppercase tracking-wider text-slate-200">
                  <History size={12} className="text-purple-400" />
                  备份历史
                </div>
                {backups.length === 0 ? (
                  <p className="text-[10px] leading-normal text-slate-500">暂无演示备份记录。</p>
                ) : (
                  <div className="max-h-48 space-y-2 overflow-y-auto pr-1">
                    {backups.map((backup) => (
                      <div key={backup.backupPath} className="space-y-2.5 rounded-lg border border-white/5 bg-[#070512] p-2.5">
                        <div className="break-all font-mono text-[9px] text-slate-500">{backup.backupPath}</div>
                        <div className="flex items-center justify-between gap-2 border-t border-white/5 pt-1">
                          <span className="font-mono text-[9px] text-slate-500">{new Date(backup.createdAt).toLocaleTimeString()} · {backup.size} B</span>
                          <button disabled={busy} onClick={() => void rollbackBackup(backup)} className="rounded border border-white/10 bg-white/5 px-2 py-0.5 text-[9px] font-semibold text-slate-300 hover:bg-white/10 disabled:pointer-events-none disabled:opacity-40">
                            恢复
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          )}
        </>
      )}
    </section>
  );
}
