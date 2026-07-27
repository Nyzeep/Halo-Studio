import type { LucideIcon } from "lucide-react";
import { Bot, FileCode2, History, Search, Settings2, SlidersHorizontal } from "lucide-react";

export type ActivityView = "files" | "search" | "agent" | "config" | "history" | "settings";

export interface ActivityBarProps {
  readonly activeView: ActivityView;
  onSelect(view: ActivityView): void;
}

const items: ReadonlyArray<{ readonly view: ActivityView; readonly label: string; readonly icon: LucideIcon }> = [
  { view: "files", label: "资源", icon: FileCode2 },
  { view: "search", label: "搜索", icon: Search },
  { view: "agent", label: "Agent", icon: Bot },
  { view: "config", label: "配置", icon: SlidersHorizontal },
  { view: "history", label: "历史", icon: History },
  { view: "settings", label: "设置", icon: Settings2 },
];

export function ActivityBar({ activeView, onSelect }: ActivityBarProps): JSX.Element {
  return (
    <div className="activitybar__stack">
      {items.map(({ view, label, icon: Icon }) => (
        <button
          key={view}
          className={`activitybar__button${activeView === view ? " activitybar__button--active" : ""}`}
          type="button"
          aria-label={label}
          aria-pressed={activeView === view}
          title={label}
          onClick={() => onSelect(view)}
        >
          <Icon size={20} strokeWidth={1.7} aria-hidden="true" />
        </button>
      ))}
    </div>
  );
}
