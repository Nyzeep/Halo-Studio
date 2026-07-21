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
      if (active) {
        setBackups(result);
      }
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
    window.halo.mcp
      .planProjectMcpWrite(workspaceRoot, selectedPreview)
      .then((plan) => {
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
    if (!selectedPreview) {
      return;
    }

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
    if (!writeResult) {
      return;
    }

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
      const request: ConfigRollbackRequest = {
        targetPath: backup.targetPath,
        backupPath: backup.backupPath
      };
      await window.halo.config.rollbackWrite(request);
      setWriteResult(null);
    } finally {
      setBusy(false);
    }
  }

  async function applyRealWrite() {
    if (!selectedPreview || !realWritePlan) {
      return;
    }

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
    <section className="mt-6 space-y-3">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 text-sm font-semibold text-slate-100">
          <FileCode2 size={16} />
          MCP 预览
        </div>
        <span className="rounded bg-halo-green/10 px-2 py-1 text-xs text-halo-green">只读预览</span>
      </div>

      <div className="rounded border border-halo-line bg-halo-panelSoft p-3">
        <div className="text-sm font-medium text-slate-100">{server.displayName}</div>
        <div className="mt-1 break-all text-xs text-slate-500">{server.command} {server.args?.join(" ")}</div>
        <div className="mt-3 flex items-center gap-2 text-xs text-slate-400">
          <ShieldCheck size={14} className="text-halo-green" />
          当前不会写入真实配置文件
        </div>
      </div>

      {loading ? (
        <div className="rounded border border-halo-line bg-halo-panelSoft p-3 text-sm text-slate-500">生成预览中...</div>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-2">
            {previews.map((preview) => (
              <button
                key={preview.agentId}
                className={`flex items-center justify-between rounded border px-2 py-2 text-left text-xs ${
                  preview.agentId === selectedPreview?.agentId
                    ? "border-halo-cyan bg-halo-cyan/10 text-halo-cyan"
                    : "border-halo-line bg-halo-panelSoft text-slate-400"
                }`}
                onClick={() => setSelectedAgentId(preview.agentId)}
              >
                {preview.agentName}
                <ChevronRight size={13} />
              </button>
            ))}
          </div>

          {selectedPreview ? (
            <div className="space-y-3">
              <div className="rounded border border-halo-line bg-[#090d12]">
                <div className="flex items-center justify-between border-b border-halo-line px-3 py-2 text-xs text-slate-500">
                  <span>{selectedPreview.targetPath}</span>
                  <span>{selectedPreview.language}</span>
                </div>
                <pre className="max-h-56 overflow-auto p-3 text-xs leading-5 text-slate-300">
                  <code>{selectedPreview.content}</code>
                </pre>
              </div>

              <div className="rounded border border-halo-line bg-halo-panelSoft p-3">
                <div className="mb-3 rounded border border-halo-line bg-[#090d12] px-3 py-2 text-xs text-slate-500">
                  <div className="text-slate-400">安全演示目标</div>
                  <div className="mt-1 break-all">{demoTargetPath}</div>
                </div>

                <div className="flex gap-2">
                  <button
                    className="flex-1 rounded bg-halo-cyan px-3 py-2 text-xs font-medium text-slate-950 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
                    disabled={busy}
                    onClick={() => void applyDemoWrite()}
                  >
                    写入演示文件
                  </button>
                  <button
                    className="rounded border border-halo-line px-3 py-2 text-xs text-slate-300 disabled:cursor-not-allowed disabled:text-slate-600"
                    disabled={busy || !writeResult}
                    onClick={() => void rollbackDemoWrite()}
                  >
                    回滚
                  </button>
                </div>

                {writeResult ? (
                  <div className="mt-3 space-y-2">
                    <div className="break-all text-xs text-slate-500">写入：{writeResult.targetPath}</div>
                    <div className="break-all text-xs text-slate-500">备份：{writeResult.backupPath}</div>
                    <pre className="max-h-40 overflow-auto rounded bg-[#090d12] p-2 text-xs leading-5 text-slate-300">
                      <code>{writeResult.diff}</code>
                    </pre>
                  </div>
                ) : (
                  <div className="mt-3 text-xs text-slate-500">写入按钮只会生成 Halo 演示文件，不会改动真实 Agent 配置。</div>
                )}
              </div>

              {realWritePlan ? (
                <div className="rounded border border-halo-line bg-halo-panelSoft p-3">
                  <div className="flex items-center justify-between gap-2">
                    <div className="text-xs font-medium text-slate-300">项目级真实写入预案</div>
                    <span
                      className={`rounded px-2 py-1 text-xs ${
                        realWritePlan.allowed ? "bg-halo-green/10 text-halo-green" : "bg-halo-red/10 text-halo-red"
                      }`}
                    >
                      {realWritePlan.risk}
                    </span>
                  </div>
                  <div className="mt-3 space-y-2 text-xs">
                    <div className="break-all text-slate-500">目标：{realWritePlan.normalizedTargetPath}</div>
                    {realWritePlan.warnings.length > 0 ? (
                      <div className="rounded border border-halo-red/40 bg-halo-red/10 p-2 text-halo-red">
                        {realWritePlan.warnings.join(" ")}
                      </div>
                    ) : (
                      <div className="space-y-2">
                        <div className="rounded border border-halo-green/30 bg-halo-green/10 p-2 text-halo-green">
                          目标位于当前工作区内。写入前仍需要确认短语。
                        </div>
                        <div className="rounded border border-halo-amber/30 bg-halo-amber/10 p-2 text-halo-amber">
                          当前会写入完整生成文件；结构化合并将在下一阶段加入。
                        </div>
                      </div>
                    )}
                    <label className="block text-slate-400">
                      确认短语
                      <input
                        className="mt-2 w-full rounded border border-halo-line bg-[#090d12] px-3 py-2 text-slate-200 outline-none focus:border-halo-cyan"
                        value={confirmation}
                        onChange={(event) => setConfirmation(event.target.value)}
                        placeholder={realWritePlan.confirmationPhrase}
                      />
                    </label>
                    <button
                      className="w-full rounded bg-halo-amber px-3 py-2 text-xs font-medium text-slate-950 disabled:cursor-not-allowed disabled:bg-slate-700 disabled:text-slate-400"
                      disabled={busy || !realWritePlan.allowed || confirmation !== realWritePlan.confirmationPhrase}
                      onClick={() => void applyRealWrite()}
                    >
                      确认写入项目配置
                    </button>
                  </div>
                </div>
              ) : null}

              <div className="rounded border border-halo-line bg-halo-panelSoft p-3">
                <div className="flex items-center gap-2 text-xs font-medium text-slate-300">
                  <History size={14} />
                  备份历史
                </div>
                {backups.length === 0 ? (
                  <div className="mt-3 text-xs text-slate-500">暂无备份。写入演示文件后会在这里出现恢复记录。</div>
                ) : (
                  <div className="mt-3 space-y-2">
                    {backups.map((backup) => (
                      <div key={backup.backupPath} className="rounded border border-halo-line bg-[#090d12] p-2">
                        <div className="break-all text-xs text-slate-400">{backup.backupPath}</div>
                        <div className="mt-2 flex items-center justify-between gap-2">
                          <span className="text-xs text-slate-500">
                            {new Date(backup.createdAt).toLocaleString()} · {backup.size} B
                          </span>
                          <button
                            className="rounded border border-halo-line px-2 py-1 text-xs text-slate-300 disabled:cursor-not-allowed disabled:text-slate-600"
                            disabled={busy}
                            onClick={() => void rollbackBackup(backup)}
                          >
                            恢复
                          </button>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            </div>
          ) : null}
        </>
      )}
    </section>
  );
}
