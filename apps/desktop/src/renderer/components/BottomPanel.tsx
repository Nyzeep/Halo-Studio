import { CircleAlert, TerminalSquare } from "lucide-react";
import { useState } from "react";

export interface BottomPanelProps {
  readonly workspaceName: string | undefined;
  readonly message: string | undefined;
}

export function BottomPanel({ workspaceName, message }: BottomPanelProps): JSX.Element {
  const [activePanel, setActivePanel] = useState<"output" | "problems">("output");

  return (
    <div className="bottom-panel">
      <div className="bottom-panel__tabs" role="tablist" aria-label="底部面板标签">
        <button
          id="bottom-panel-output-tab"
          className={`bottom-panel__tab${activePanel === "output" ? " bottom-panel__tab--active" : ""}`}
          type="button"
          role="tab"
          aria-selected={activePanel === "output"}
          aria-controls="bottom-panel-output"
          onClick={() => setActivePanel("output")}
        >输出</button>
        <button
          id="bottom-panel-problems-tab"
          className={`bottom-panel__tab${activePanel === "problems" ? " bottom-panel__tab--active" : ""}`}
          type="button"
          role="tab"
          aria-selected={activePanel === "problems"}
          aria-controls="bottom-panel-problems"
          onClick={() => setActivePanel("problems")}
        >问题</button>
      </div>
      {activePanel === "output" ? (
        <div id="bottom-panel-output" className="bottom-panel__body" role="tabpanel" aria-labelledby="bottom-panel-output-tab">
          {message === undefined ? (
            <><TerminalSquare size={15} aria-hidden="true" /><span>{workspaceName === undefined ? "等待工作区" : `${workspaceName} 已连接到桌面桥接`}</span></>
          ) : (
            <><CircleAlert size={15} aria-hidden="true" /><span>{message}</span></>
          )}
        </div>
      ) : (
        <div id="bottom-panel-problems" className="bottom-panel__body" role="tabpanel" aria-labelledby="bottom-panel-problems-tab">
          {message === undefined ? (
            <><TerminalSquare size={15} aria-hidden="true" /><span>无问题</span></>
          ) : (
            <><CircleAlert size={15} aria-hidden="true" /><span>{message}</span></>
          )}
        </div>
      )}
    </div>
  );
}
