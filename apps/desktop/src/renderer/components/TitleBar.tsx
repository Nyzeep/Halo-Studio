import { Command, PanelsTopLeft } from "lucide-react";
import { useEffect, useRef } from "react";

export interface TitleBarProps {
  readonly commandOpen: boolean;
  onToggleCommand(): void;
  onSelectCommand(view: "files" | "agent" | "config"): void;
}

export function TitleBar({ commandOpen, onToggleCommand, onSelectCommand }: TitleBarProps): JSX.Element {
  const commandButtonRef = useRef<HTMLButtonElement>(null);
  const commandMenuRef = useRef<HTMLDivElement>(null);
  const wasOpen = useRef(false);

  useEffect(() => {
    if (commandOpen) {
      commandMenuRef.current?.focus();
    } else if (wasOpen.current) {
      commandButtonRef.current?.focus();
    }
    wasOpen.current = commandOpen;
  }, [commandOpen]);

  return (
    <div className="titlebar">
      <div className="titlebar__brand" aria-label="Halo Studio">
        <PanelsTopLeft size={16} strokeWidth={1.8} aria-hidden="true" />
        <span>HALO STUDIO</span>
      </div>
      <div className="titlebar__command-wrap">
        <button
          className="titlebar__command"
          type="button"
          ref={commandButtonRef}
          aria-expanded={commandOpen}
          aria-haspopup="menu"
          aria-controls="halo-command-menu"
          aria-label="命令中心"
          title="命令中心"
          onClick={onToggleCommand}
        >
          <Command size={15} aria-hidden="true" />
          <span>命令中心</span>
        </button>
        {commandOpen ? (
          <div
            id="halo-command-menu"
            className="command-menu"
            role="menu"
            aria-label="命令中心"
            ref={commandMenuRef}
            tabIndex={-1}
            onKeyDown={(event) => {
              if (event.key === "Escape") onToggleCommand();
            }}
          >
            <button type="button" role="menuitem" onClick={() => onSelectCommand("files")}>资源管理器</button>
            <button type="button" role="menuitem" onClick={() => onSelectCommand("agent")}>Agent 状态</button>
            <button type="button" role="menuitem" onClick={() => onSelectCommand("config")}>配置域</button>
          </div>
        ) : null}
      </div>
      <div className="titlebar__window-controls" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
    </div>
  );
}
