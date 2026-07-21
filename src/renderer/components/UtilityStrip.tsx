import { Boxes, FileSearch, GitBranch, History, SlidersHorizontal } from "lucide-react";

const utilities = [
  { label: "会话档案", description: "按项目浏览历史会话", icon: History },
  { label: "项目文件", description: "预览源码、Markdown 和 diff", icon: FileSearch },
  { label: "模型配置", description: "集中查看 Agent 模型状态", icon: SlidersHorizontal },
  { label: "技能管理", description: "启用指令集和 Agent 技能", icon: Boxes },
  { label: "Worktree", description: "为分支实验预留入口", icon: GitBranch }
];

export function UtilityStrip() {
  return (
    <div className="grid grid-cols-5 gap-2 border-b border-white/5 bg-[#0a0814]/40 px-4 py-2">
      {utilities.map((item) => {
        const Icon = item.icon;
        return (
          <button
            key={item.label}
            className="flex h-14 items-center gap-3 rounded-xl border border-white/5 bg-white/5 px-3.5 text-left hover:border-purple-500/30 hover:bg-purple-500/5 transition-all duration-300"
          >
            <div className="rounded-lg bg-purple-500/10 p-1.5 text-purple-400">
              <Icon size={14} />
            </div>
            <span className="min-w-0">
              <span className="block truncate text-xs font-bold text-slate-200">{item.label}</span>
              <span className="block truncate text-[10px] text-slate-500 leading-tight mt-0.5">{item.description}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
