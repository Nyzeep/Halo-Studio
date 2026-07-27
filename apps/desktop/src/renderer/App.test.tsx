import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RuntimeBinding, Workspace } from "@halo-studio/contracts";
import { App, type WorkbenchApi } from "./App.js";

const workspaceId = "a".repeat(64);
const candidate = { selectionId: "11111111-1111-4111-8111-111111111111", displayPath: "D:\\项目\\示例" };
const workspace: Workspace = {
  id: workspaceId,
  rootPath: "D:\\项目\\示例",
  realPath: "D:\\项目\\示例",
  trustState: "untrusted",
};
const unavailableCapability = {
  supported: false,
  channel: "unavailable" as const,
  restartRequired: false,
  reason: "当前阶段未开放。",
};

function binding(
  agentKind: "pi" | "opencode",
  health: RuntimeBinding["health"],
  sessionsSupported = false,
): RuntimeBinding {
  const sessions = sessionsSupported
    ? { supported: true, channel: agentKind === "pi" ? "rpc" as const : "http" as const, restartRequired: false }
    : unavailableCapability;
  return {
    agentKind,
    source: agentKind === "pi" ? "managed" : "bundled",
    health,
    capabilities: {
      sessions,
      streamingMessages: unavailableCapability,
      toolEvents: unavailableCapability,
      permissions: unavailableCapability,
      diff: unavailableCapability,
      commands: unavailableCapability,
      mcp: unavailableCapability,
      skills: unavailableCapability,
      prompts: unavailableCapability,
      extensions: unavailableCapability,
      packages: unavailableCapability,
      models: unavailableCapability,
      usage: unavailableCapability,
    },
  };
}

function createApi(options: {
  readonly initialWorkspaces?: readonly Workspace[];
  readonly bindings?: readonly RuntimeBinding[];
  readonly sessions?: readonly { readonly agentKind: "pi" | "opencode"; readonly sessionId: string; readonly title?: string; readonly active: boolean }[];
} = {}): WorkbenchApi & {
  readonly workspace: WorkbenchApi["workspace"] & { readonly pick: ReturnType<typeof vi.fn>; readonly open: ReturnType<typeof vi.fn>; readonly setTrust: ReturnType<typeof vi.fn> };
  readonly runtime: WorkbenchApi["runtime"] & { readonly probe: ReturnType<typeof vi.fn>; readonly start: ReturnType<typeof vi.fn>; readonly stop: ReturnType<typeof vi.fn> };
  readonly sessions: WorkbenchApi["sessions"] & { readonly snapshot: ReturnType<typeof vi.fn>; readonly send: ReturnType<typeof vi.fn> };
} {
  const activeBindings = options.bindings ?? [binding("pi", "ready"), binding("opencode", "healthy")];
  const pick = vi.fn(async () => ({ ok: true as const, data: candidate }));
  const open = vi.fn(async () => ({ ok: true as const, data: workspace }));
  const snapshot = vi.fn(async () => ({ ok: true as const, data: options.initialWorkspaces ?? [workspace] }));
  const setTrust = vi.fn(async () => ({ ok: true as const, data: { ...workspace, trustState: "trusted" as const } }));
  const probe = vi.fn(async () => ({ ok: true as const, data: activeBindings }));
  const start = vi.fn(async ({ agentKind }: { readonly agentKind: "pi" | "opencode" }) => ({
    ok: true as const,
    data: binding(agentKind, agentKind === "pi" ? "ready" : "healthy"),
  }));
  const stop = vi.fn(async ({ agentKind }: { readonly agentKind: "pi" | "opencode" }) => ({
    ok: true as const,
    data: binding(agentKind, "stopped"),
  }));
  const runtimeSnapshot = vi.fn(async () => ({ ok: true as const, data: activeBindings }));
  const sessionSnapshot = vi.fn(async () => ({ ok: true as const, data: options.sessions ?? [] }));
  const createSession = vi.fn(async ({ agentKind }: { readonly agentKind: "pi" | "opencode" }) => ({
    ok: true as const,
    data: { agentKind, sessionId: `${agentKind}-created`, title: "New managed session", active: true },
  }));
  const selectSession = vi.fn(async ({ agentKind, sessionId }: { readonly agentKind: "pi" | "opencode"; readonly sessionId: string }) => ({
    ok: true as const,
    data: { agentKind, sessionId, title: "Selected managed session", active: true },
  }));
  const sessionHistory = vi.fn(async ({ agentKind, sessionId }: { readonly agentKind: "pi" | "opencode"; readonly sessionId: string }) => ({
    ok: true as const,
    data: {
      session: { agentKind, sessionId, title: "Selected managed session", active: true },
      messages: [{ agentKind, sessionId, ordinal: 0, role: "assistant" as const, text: "Existing Pi message" }],
    },
  }));
  const sendSession = vi.fn(async ({ agentKind, sessionId, clientRequestId }: {
    readonly agentKind: "pi" | "opencode";
    readonly sessionId: string;
    readonly clientRequestId: string;
  }) => ({
    ok: true as const,
    data: {
      session: { agentKind, sessionId, title: "Selected managed session", active: true },
      clientRequestId,
      accepted: true as const,
    },
  }));
  const abortSession = vi.fn(async ({ agentKind, sessionId }: { readonly agentKind: "pi" | "opencode"; readonly sessionId: string }) => ({
    ok: true as const,
    data: { agentKind, sessionId, title: "Selected managed session", active: false },
  }));
  const subscribeSessions = vi.fn(() => () => undefined);
  const listCommands = vi.fn(async ({ agentKind }: { readonly agentKind: "pi" | "opencode" }) => ({
    ok: true as const,
    data: agentKind === "pi" ? [{
      name: "/compact",
      agentKind,
      source: "native" as const,
      channel: "rpc" as const,
      allowedWhileRunning: false,
      mutatesGlobalDefaults: false,
      tuiOnly: false,
    }] : [],
  }));

  return {
    workspace: { pick, open, snapshot, setTrust },
    runtime: {
      probe,
      start,
      stop,
      snapshot: runtimeSnapshot,
    },
    sessions: {
      snapshot: sessionSnapshot,
      create: createSession,
      select: selectSession,
      history: sessionHistory,
      send: sendSession,
      abort: abortSession,
      subscribe: subscribeSessions,
    },
    commands: { list: listCommands },
  } as unknown as WorkbenchApi & {
    readonly workspace: WorkbenchApi["workspace"] & { readonly pick: ReturnType<typeof vi.fn>; readonly open: ReturnType<typeof vi.fn>; readonly setTrust: ReturnType<typeof vi.fn> };
    readonly runtime: WorkbenchApi["runtime"] & { readonly probe: ReturnType<typeof vi.fn>; readonly start: ReturnType<typeof vi.fn>; readonly stop: ReturnType<typeof vi.fn> };
    readonly sessions: WorkbenchApi["sessions"] & { readonly snapshot: ReturnType<typeof vi.fn>; readonly send: ReturnType<typeof vi.fn> };
  };
}

describe("App", () => {
  it("renders the shared workspace in both domains and reports IPC runtime state", async () => {
    const api = createApi();
    render(<App api={api} />);

    await screen.findByText("示例");
    expect(screen.getByRole("banner", { name: "标题栏" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "主活动栏" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "侧边栏" })).toBeInTheDocument();
    expect(screen.getByRole("main", { name: "编辑器区域" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Agent 面板" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "底部面板" })).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "状态栏" })).toBeInTheDocument();
    expect(await screen.findByText("已就绪")).toBeInTheDocument();
    expect(screen.getByText("运行正常")).toBeInTheDocument();
    expect(screen.queryByText("在线")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "配置" }));
    expect(screen.getByText("配置写入尚未开放")).toBeInTheDocument();
    expect(screen.getAllByText("示例").length).toBeGreaterThan(1);
  });

  it("opens a folder only through pick then open", async () => {
    const api = createApi({ initialWorkspaces: [] });
    render(<App api={api} />);

    fireEvent.click(await screen.findByRole("button", { name: "打开文件夹" }));
    await waitFor(() => expect(api.workspace.open).toHaveBeenCalledWith({ selectionId: candidate.selectionId }));
    expect(api.workspace.pick).toHaveBeenCalledWith({});
  });

  it("can replace an already opened workspace through the sidebar action", async () => {
    const api = createApi();
    render(<App api={api} />);

    const openFolderButtons = await screen.findAllByRole("button", { name: "打开文件夹" });
    expect(openFolderButtons).toHaveLength(1);
    await waitFor(() => expect(screen.getByRole("button", { name: "打开文件夹" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "打开文件夹" }));
    await waitFor(() => expect(api.workspace.open).toHaveBeenCalledWith({ selectionId: candidate.selectionId }));
  });

  it("moves focus into the command menu and restores it after Escape", async () => {
    const api = createApi();
    render(<App api={api} />);

    const trigger = screen.getByRole("button", { name: "命令中心" });
    fireEvent.click(trigger);
    const menu = await screen.findByRole("menu", { name: "命令中心" });
    await waitFor(() => expect(menu).toHaveFocus());
    fireEvent.keyDown(menu, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menu", { name: "命令中心" })).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });

  it("trusts a workspace without implicitly starting Pi or OpenCode", async () => {
    const api = createApi({
      bindings: [binding("pi", "detected"), binding("opencode", "stopped")],
    });
    render(<App api={api} />);

    fireEvent.click(await screen.findByRole("button", { name: "信任工作区" }));
    expect(api.workspace.setTrust).toHaveBeenCalledWith({ workspaceId, trustState: "trusted" });
    await screen.findByRole("button", { name: "使用受管启动配置启动 Pi" });
    expect(api.runtime.start).not.toHaveBeenCalled();
  });

  it("starts Pi only with the fixed trusted-workspace IPC input", async () => {
    const trustedWorkspace = { ...workspace, trustState: "trusted" as const };
    const api = createApi({
      initialWorkspaces: [trustedWorkspace],
      bindings: [binding("pi", "detected"), binding("opencode", "healthy")],
    });
    render(<App api={api} />);

    expect(await screen.findByText("受管启动配置")).toBeInTheDocument();
    const startButton = await screen.findByRole("button", { name: "使用受管启动配置启动 Pi" });
    fireEvent.click(startButton);

    await waitFor(() => expect(api.runtime.start).toHaveBeenCalledWith({ workspaceId, agentKind: "pi" }));
    expect(api.runtime.start).toHaveBeenCalledTimes(1);
  });

  it("stops a ready Pi through the fixed runtime stop channel", async () => {
    const trustedWorkspace = { ...workspace, trustState: "trusted" as const };
    const api = createApi({
      initialWorkspaces: [trustedWorkspace],
      bindings: [binding("pi", "ready"), binding("opencode", "healthy")],
    });
    render(<App api={api} />);

    fireEvent.click(await screen.findByRole("button", { name: "停止 Pi" }));
    await waitFor(() => expect(api.runtime.stop).toHaveBeenCalledWith({ workspaceId, agentKind: "pi" }));
    expect(api.runtime.start).not.toHaveBeenCalled();
  });

  it("retries a crashed Pi by releasing it before the fixed restart", async () => {
    const trustedWorkspace = { ...workspace, trustState: "trusted" as const };
    const api = createApi({
      initialWorkspaces: [trustedWorkspace],
      bindings: [binding("pi", "crashed"), binding("opencode", "healthy")],
    });
    render(<App api={api} />);

    fireEvent.click(await screen.findByRole("button", { name: "重试 Pi" }));
    await waitFor(() => expect(api.runtime.start).toHaveBeenCalledWith({ workspaceId, agentKind: "pi" }));
    expect(api.runtime.stop).toHaveBeenCalledWith({ workspaceId, agentKind: "pi" });
    expect(api.runtime.stop.mock.invocationCallOrder[0]).toBeLessThan(api.runtime.start.mock.invocationCallOrder[0]!);
  });

  it("keeps an unavailable Pi retryable without claiming it is installed", async () => {
    const trustedWorkspace = { ...workspace, trustState: "trusted" as const };
    const api = createApi({
      initialWorkspaces: [trustedWorkspace],
      bindings: [binding("pi", "unavailable"), binding("opencode", "healthy")],
    });
    render(<App api={api} />);

    expect(await screen.findByText("未检测到可启动的 Pi")).toBeInTheDocument();
    expect(screen.queryByText("已安装")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "重试 Pi" }));
    await waitFor(() => expect(api.runtime.start).toHaveBeenCalledWith({ workspaceId, agentKind: "pi" }));
  });

  it("does not present an unprobed Pi as installed or ready", async () => {
    const trustedWorkspace = { ...workspace, trustState: "trusted" as const };
    const api = createApi({
      initialWorkspaces: [trustedWorkspace],
      bindings: [binding("opencode", "healthy")],
    });
    render(<App api={api} />);

    expect(await screen.findByText("未检测 / 待启动")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "使用受管启动配置启动 Pi" })).not.toBeInTheDocument();
    expect(screen.queryByText("已安装")).not.toBeInTheDocument();
  });

  it("offers an OpenCode retry for an already trusted workspace without starting Pi", async () => {
    const trustedWorkspace = { ...workspace, trustState: "trusted" as const };
    const api = createApi({
      initialWorkspaces: [trustedWorkspace],
      bindings: [binding("pi", "ready"), binding("opencode", "stopped")],
    });
    render(<App api={api} />);

    const startButton = await screen.findByRole("button", { name: "启动 OpenCode" });
    await waitFor(() => expect(startButton).toBeEnabled());
    fireEvent.click(startButton);
    await waitFor(() => expect(api.runtime.start).toHaveBeenCalledWith({ workspaceId, agentKind: "opencode" }));
    expect(api.workspace.setTrust).not.toHaveBeenCalled();
  });

  it("uses only the fixed managed-session API in the trusted Agent workbench", async () => {
    const trustedWorkspace = { ...workspace, trustState: "trusted" as const };
    const api = createApi({
      initialWorkspaces: [trustedWorkspace],
      bindings: [binding("pi", "ready", true), binding("opencode", "healthy")],
      sessions: [{ agentKind: "pi", sessionId: "pi-current", title: "Current Pi session", active: true }],
    });
    render(<App api={api} />);

    fireEvent.click(await screen.findByRole("button", { name: "Agent" }));
    await waitFor(() => expect(api.sessions.snapshot).toHaveBeenCalledWith({ workspaceId }));
    expect((await screen.findAllByText("Selected managed session")).length).toBeGreaterThan(0);
    expect(await screen.findByText("Existing Pi message")).toBeInTheDocument();

    const composer = screen.getByRole("textbox");
    fireEvent.change(composer, { target: { value: "continue the native session" } });
    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    await waitFor(() => expect(api.sessions.send).toHaveBeenCalledWith(expect.objectContaining({
      workspaceId,
      agentKind: "pi",
      sessionId: "pi-current",
      message: "continue the native session",
    })));
    expect(api.runtime.start).not.toHaveBeenCalled();
  });

  it("refreshes a crashed OpenCode state before allowing a retry", async () => {
    const trustedWorkspace = { ...workspace, trustState: "trusted" as const };
    const api = createApi({
      initialWorkspaces: [trustedWorkspace],
      bindings: [binding("pi", "ready"), binding("opencode", "healthy")],
    });
    render(<App api={api} />);

    await screen.findByText("运行正常");
    api.runtime.probe.mockResolvedValue({ ok: true, data: [binding("pi", "ready"), binding("opencode", "crashed")] });
    fireEvent.click(screen.getByRole("button", { name: "刷新运行时状态" }));
    await screen.findByText("已崩溃");

    const startButton = screen.getByRole("button", { name: "启动 OpenCode" });
    expect(startButton).toBeEnabled();
    fireEvent.click(startButton);
    await waitFor(() => expect(api.runtime.start).toHaveBeenCalledWith({ workspaceId, agentKind: "opencode" }));
  });
});
