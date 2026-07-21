import React from "react";
import { History, Activity, ShieldCheck, Terminal, Undo, FileCode2, Clock } from "lucide-react";
import type { TerminalSessionInfo } from "../../shared/agents";

interface HistoryViewProps {
  sessions: TerminalSessionInfo[];
  activeSessionId: string | null;
  onSelectSession(id: string): void;
  onCloseSession(id: string): void;
}

export function HistoryView({ sessions, activeSessionId, onSelectSession, onCloseSession }: HistoryViewProps) {
  // Static audit logs for gorgeous tech feel
  const auditLogs = [
    {
      time: "2026-07-21 06:12:44",
      type: "WRITE_DEMO_SUCCESS",
      agent: "claude-code",
      file: "claude-code-mcp-preview.txt",
      details: "成功创建 1245 字节的安全沙箱演示文件，伴随 1 条变更备份记录"
    },
    {
      time: "2026-07-21 05:44:12",
      type: "BACKUP_CREATED",
      agent: "codex-cli",
      file: "codex-cli-mcp-preview.txt",
      details: "生成配置快照，备份于 ~/.halo-user-data/preview-configs"
    },
    {
      time: "2026-07-21 04:10:02",
      type: "AGENT_DISCOVERY",
      agent: "system",
      file: "system-registry",
      details: "扫描本地环境变量：检测到 4 个可用 Agent 适配器且正常初始化"
    }
  ];

  return (
    <div className="relative flex h-full w-full flex-col overflow-y-auto px-6 py-10 md:px-10 lg:px-14">
      <div className="starfield" />

      <div className="relative z-10 max-w-4xl space-y-8">
        {/* Page Header */}
        <div className="flex items-center gap-3">
          <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-purple-500/10 text-purple-400">
            <History size={24} />
          </div>
          <div>
            <h1 className="text-2xl font-bold tracking-tight text-slate-100">审计日志与备份历史</h1>
            <p className="text-xs text-slate-400">追踪 Agent 真实写入轨迹、管理会话进程快照与安全性回滚</p>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-6 md:grid-cols-3">
          {/* Main timeline of activities */}
          <div className="md:col-span-2 space-y-6">
            <div className="glass-panel rounded-2xl p-6 space-y-4">
              <h2 className="flex items-center gap-2 text-sm font-semibold text-slate-200">
                <Clock size={16} className="text-purple-400" />
                自动合并与修改审计轨迹
              </h2>

              <div className="relative border-l border-white/5 pl-5 ml-2.5 space-y-6">
                {auditLogs.map((log, idx) => (
                  <div key={idx} className="relative group">
                    {/* Pulsing timeline dot */}
                    <span className="absolute -left-[26px] top-1.5 flex h-2 w-2 rounded-full bg-purple-500 ring-4 ring-purple-500/10 group-hover:scale-125 transition-transform" />

                    <div className="space-y-1.5">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-[10px] text-slate-500">{log.time}</span>
                        <span className="rounded bg-purple-500/10 px-2 py-0.5 text-[10px] text-purple-300 font-mono">
                          {log.type}
                        </span>
                      </div>
                      <div className="text-xs font-semibold text-slate-300">
                        Agent: <span className="text-purple-400">{log.agent}</span> · Target: <span className="font-mono text-slate-400">{log.file}</span>
                      </div>
                      <p className="text-xs text-slate-500 leading-relaxed">
                        {log.details}
                      </p>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* Quick backup tip */}
            <div className="glass-panel rounded-2xl p-5 flex items-start gap-4">
              <div className="rounded-xl bg-purple-500/10 p-2.5 text-purple-400">
                <ShieldCheck size={20} />
              </div>
              <div className="space-y-1.5 text-xs">
                <h4 className="font-semibold text-slate-200">零侵入、高安全的回滚保护</h4>
                <p className="text-slate-500 leading-relaxed">
                  所有的配置文件改动在真实生效前都会自动拉取旧版内容并压缩存档。若 Agent 生成了错误的代码导致项目崩溃，可以在右侧的“MCP 预览”配置中选择任意备份记录一键回退。
                </p>
              </div>
            </div>
          </div>

          {/* Active terminals panel */}
          <div className="space-y-6">
            <div className="glass-panel rounded-2xl p-5 space-y-4">
              <h3 className="text-xs font-bold text-slate-200 uppercase tracking-wider">当前活动会话</h3>

              {sessions.length === 0 ? (
                <div className="text-center py-8 space-y-2">
                  <Terminal size={24} className="mx-auto text-slate-600" />
                  <p className="text-xs text-slate-500">暂无启动中的终端进程</p>
                </div>
              ) : (
                <div className="space-y-3">
                  {sessions.map((session) => (
                    <div
                      key={session.id}
                      className={`rounded-xl border p-3.5 space-y-3 transition-all ${
                        session.id === activeSessionId
                          ? "border-purple-500/40 bg-purple-500/5"
                          : "border-white/5 bg-[#090712]"
                      }`}
                    >
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-semibold text-slate-200 truncate pr-2">
                          {session.title}
                        </span>
                        <span className="rounded-full bg-emerald-500/10 px-2 py-0.5 text-[9px] font-semibold text-emerald-400">
                          RUNNING
                        </span>
                      </div>
                      <p className="font-mono text-[10px] text-slate-500 truncate">
                        CWD: {session.cwd}
                      </p>
                      <div className="flex gap-2">
                        <button
                          onClick={() => onSelectSession(session.id)}
                          className="flex-1 rounded-lg border border-white/10 bg-white/5 py-1.5 text-[10px] font-semibold hover:bg-white/10 text-slate-300"
                        >
                          切换到
                        </button>
                        <button
                          onClick={() => onCloseSession(session.id)}
                          className="rounded-lg border border-red-500/20 bg-red-500/5 px-2.5 py-1.5 text-[10px] font-semibold text-red-400 hover:bg-red-500/10"
                        >
                          关闭
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
