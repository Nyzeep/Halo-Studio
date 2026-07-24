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

function binding(agentKind: "pi" | "opencode", health: RuntimeBinding["health"]): RuntimeBinding {
  return {
    agentKind,
    source: agentKind === "pi" ? "managed" : "bundled",
    health,
    capabilities: {
      sessions: unavailableCapability,
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
} = {}): WorkbenchApi & {
  readonly workspace: WorkbenchApi["workspace"] & { readonly pick: ReturnType<typeof vi.fn>; readonly open: ReturnType<typeof vi.fn>; readonly setTrust: ReturnType<typeof vi.fn> };
  readonly runtime: WorkbenchApi["runtime"] & { readonly probe: ReturnType<typeof vi.fn>; readonly start: ReturnType<typeof vi.fn> };
} {
  const activeBindings = options.bindings ?? [binding("pi", "ready"), binding("opencode", "healthy")];
  const pick = vi.fn(async () => ({ ok: true as const, data: candidate }));
  const open = vi.fn(async () => ({ ok: true as const, data: workspace }));
  const snapshot = vi.fn(async () => ({ ok: true as const, data: options.initialWorkspaces ?? [workspace] }));
  const setTrust = vi.fn(async () => ({ ok: true as const, data: { ...workspace, trustState: "trusted" as const } }));
  const probe = vi.fn(async () => ({ ok: true as const, data: activeBindings }));
  const start = vi.fn(async () => ({ ok: true as const, data: binding("opencode", "healthy") }));

  return {
    workspace: { pick, open, snapshot, setTrust },
    runtime: {
      probe,
      start,
      stop: vi.fn(),
      snapshot: vi.fn(),
    },
  } as unknown as WorkbenchApi & {
    readonly workspace: WorkbenchApi["workspace"] & { readonly pick: ReturnType<typeof vi.fn>; readonly open: ReturnType<typeof vi.fn>; readonly setTrust: ReturnType<typeof vi.fn> };
    readonly runtime: WorkbenchApi["runtime"] & { readonly probe: ReturnType<typeof vi.fn>; readonly start: ReturnType<typeof vi.fn> };
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
    expect(screen.getByText("Pi 资源")).toBeInTheDocument();
    expect(screen.getByText("OpenCode MCP")).toBeInTheDocument();
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

  it("trusts first, refreshes state, then starts only OpenCode", async () => {
    const api = createApi();
    render(<App api={api} />);

    fireEvent.click(await screen.findByRole("button", { name: "信任并启动" }));
    await waitFor(() => expect(api.runtime.start).toHaveBeenCalledWith({ workspaceId, agentKind: "opencode" }));
    expect(api.workspace.setTrust).toHaveBeenCalledWith({ workspaceId, trustState: "trusted" });
    expect(api.workspace.setTrust.mock.invocationCallOrder[0]).toBeLessThan(api.runtime.start.mock.invocationCallOrder[0]!);
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
