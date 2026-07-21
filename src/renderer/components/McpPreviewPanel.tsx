import { ChevronRight, FileCode2, ShieldCheck } from "lucide-react";
import { useMemo, useState } from "react";
import { useMcpPreview } from "../hooks/useMcpPreview";

export function McpPreviewPanel() {
  const { previews, loading, server } = useMcpPreview();
  const [selectedAgentId, setSelectedAgentId] = useState<string>("codex-cli");
  const selectedPreview = useMemo(
    () => previews.find((preview) => preview.agentId === selectedAgentId) ?? previews[0],
    [previews, selectedAgentId]
  );

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
            <div className="rounded border border-halo-line bg-[#090d12]">
              <div className="flex items-center justify-between border-b border-halo-line px-3 py-2 text-xs text-slate-500">
                <span>{selectedPreview.targetPath}</span>
                <span>{selectedPreview.language}</span>
              </div>
              <pre className="max-h-56 overflow-auto p-3 text-xs leading-5 text-slate-300">
                <code>{selectedPreview.content}</code>
              </pre>
            </div>
          ) : null}
        </>
      )}
    </section>
  );
}
