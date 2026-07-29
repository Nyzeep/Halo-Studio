import { fireEvent, render, screen } from "@testing-library/react";
import type {
  CommandDescriptor,
  SessionMessage,
  SessionSummary,
} from "@halo-studio/contracts";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { SessionWorkbench, type SessionWorkbenchProps } from "./SessionWorkbench.js";

const piSession: SessionSummary = {
  agentKind: "pi",
  sessionId: "pi-session-1",
  title: "修复登录流程",
  active: true,
};

const openCodeSession: SessionSummary = {
  agentKind: "opencode",
  sessionId: "opencode-session-1",
  title: "审查变更",
  active: false,
};

const messages: readonly SessionMessage[] = [
  {
    agentKind: "pi",
    sessionId: piSession.sessionId,
    ordinal: 0,
    role: "user",
    text: "请检查登录流程。",
  },
  {
    agentKind: "pi",
    sessionId: piSession.sessionId,
    ordinal: 1,
    role: "assistant",
    text: "我会先检查当前实现。",
  },
  {
    agentKind: "opencode",
    sessionId: openCodeSession.sessionId,
    ordinal: 0,
    role: "assistant",
    text: "不应显示在 Pi 会话中。",
  },
];

const commands: readonly CommandDescriptor[] = [
  {
    agentKind: "pi",
    name: "/compact",
    description: "压缩当前会话。",
    source: "native",
    channel: "rpc",
    allowedWhileRunning: true,
    mutatesGlobalDefaults: false,
    tuiOnly: false,
  },
  {
    agentKind: "pi",
    name: "/model",
    argumentHint: "<名称>",
    source: "tui",
    channel: "cli",
    allowedWhileRunning: true,
    mutatesGlobalDefaults: false,
    tuiOnly: true,
  },
  {
    agentKind: "opencode",
    name: "/share",
    description: "共享当前会话。",
    source: "native",
    channel: "http",
    allowedWhileRunning: false,
    mutatesGlobalDefaults: false,
    tuiOnly: false,
  },
];

function baseProps(): SessionWorkbenchProps {
  return {
    sessions: [piSession, openCodeSession],
    selectedSession: piSession,
    messages,
    commands,
    draft: "",
    commandFilter: "",
    loading: false,
    sending: false,
    aborting: false,
    error: undefined,
    onSelectSession: vi.fn(),
    onDraftChange: vi.fn(),
    onCommandFilterChange: vi.fn(),
    onSend: vi.fn(),
    onAbort: vi.fn(),
  };
}

function renderWorkbench(overrides: Partial<SessionWorkbenchProps> = {}) {
  const props = { ...baseProps(), ...overrides };
  return { props, ...render(<SessionWorkbench {...props} />) };
}

describe("SessionWorkbench", () => {
  it("renders scoped structured messages and delegates session, send, and abort actions", () => {
    const { props } = renderWorkbench({ draft: "/compact" });

    expect(screen.getByRole("region", { name: "受管会话工作台" })).toBeInTheDocument();
    expect(screen.getByText("请检查登录流程。")).toBeInTheDocument();
    expect(screen.getByText("我会先检查当前实现。")).toBeInTheDocument();
    expect(screen.queryByText("不应显示在 Pi 会话中。")).not.toBeInTheDocument();
    expect(screen.getByText("原生")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /审查变更/u }));
    expect(props.onSelectSession).toHaveBeenCalledWith(openCodeSession);

    fireEvent.click(screen.getByRole("button", { name: "发送消息" }));
    expect(props.onSend).toHaveBeenCalledWith(piSession, "/compact");

    fireEvent.click(screen.getByRole("button", { name: "中止当前会话" }));
    expect(props.onAbort).toHaveBeenCalledWith(piSession);
  });

  it("shows a loading state while the controlled session snapshot is pending", () => {
    renderWorkbench({ sessions: [], selectedSession: undefined, messages: [], commands: [], loading: true });

    expect(screen.getByRole("region", { name: "受管会话工作台" })).toHaveAttribute("aria-busy", "true");
    expect(screen.getByText("正在加载受管会话...")).toBeInTheDocument();
    expect(screen.getByLabelText("正在加载会话")).toBeInTheDocument();
  });

  it("shows an empty state without enabling a message input", () => {
    renderWorkbench({ sessions: [], selectedSession: undefined, messages: [], commands: [] });

    expect(screen.getByText("当前工作区尚无受管会话。")).toBeInTheDocument();
    expect(screen.getByText("选择一个受管会话以查看消息。")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "会话消息" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "中止当前会话" })).toBeDisabled();
  });

  it("renders a controlled error without hiding the current message history", () => {
    renderWorkbench({ error: "无法加载最新原生命令。" });

    expect(screen.getByRole("alert")).toHaveTextContent("无法加载最新原生命令。");
    expect(screen.getByText("请检查登录流程。")).toBeInTheDocument();
  });

  it("scopes the selected session by both agent kind and opaque native ID", () => {
    const sharedId = "native-session";
    const selectedOpenCodeSession = { ...openCodeSession, sessionId: sharedId };
    renderWorkbench({
      sessions: [{ ...piSession, sessionId: sharedId }, selectedOpenCodeSession],
      selectedSession: selectedOpenCodeSession,
      messages: [
        { agentKind: "pi", sessionId: sharedId, ordinal: 0, role: "assistant", text: "Pi 消息。" },
        { agentKind: "opencode", sessionId: sharedId, ordinal: 0, role: "assistant", text: "OpenCode 消息。" },
      ],
    });

    expect(screen.getByText("OpenCode 消息。")).toBeInTheDocument();
    expect(screen.queryByText("Pi 消息。")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "插入命令 /share" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "插入命令 /compact" })).not.toBeInTheDocument();
  });

  it("filters only the selected agent command directory and inserts a command as draft text", () => {
    function Harness(): JSX.Element {
      const [commandFilter, setCommandFilter] = useState("");
      const [draft, setDraft] = useState("");
      return (
        <SessionWorkbench
          {...baseProps()}
          commandFilter={commandFilter}
          draft={draft}
          onCommandFilterChange={setCommandFilter}
          onDraftChange={setDraft}
        />
      );
    }

    render(<Harness />);
    const filter = screen.getByRole("searchbox", { name: "筛选原生命令" });
    fireEvent.change(filter, { target: { value: "compact" } });

    expect(screen.getByText("/compact")).toBeInTheDocument();
    expect(screen.queryByText("/model")).not.toBeInTheDocument();
    expect(screen.queryByText("/share")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "插入命令 /compact" }));
    expect(screen.getByRole("textbox", { name: "会话消息" })).toHaveValue("/compact");

    fireEvent.change(screen.getByRole("textbox", { name: "会话消息" }), { target: { value: "/model" } });
    expect(screen.getByRole("button", { name: "插入命令 /model" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "插入命令 /compact" })).not.toBeInTheDocument();
  });
});
