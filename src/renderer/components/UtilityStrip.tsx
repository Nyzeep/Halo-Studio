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
    <div className="grid grid-cols-5 gap-2 border-b border-halo-line bg-halo-panel px-3 py-2">
      {utilities.map((item) => {
        const Icon = item.icon;
        return (
          <button
            key={item.label}
            className="flex h-14 items-center gap-3 rounded border border-halo-line bg-halo-panelSoft px-3 text-left hover:border-halo-cyan/60"
          >
            <Icon size={17} className="shrink-0 text-halo-cyan" />
            <span className="min-w-0">
              <span className="block truncate text-sm font-medium text-slate-200">{item.label}</span>
              <span className="block truncate text-xs text-slate-500">{item.description}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
