---
status: superseded by ADR-0072 for P0 execution transport
---

# Pi/OpenCode 受管工作台边界

Halo Studio 只将 Pi 与 OpenCode 视为受管应用，首个可发布版本交付其工作区、信任、配置安全边界和运行时生命周期，而不是完整 IDE。VS Code 风格布局不等于复制 VS Code；完整编辑器、结构化会话、命令目录与调试终端属于后续阶段。届时终端必须是工作区绑定的受管 TUI 会话，不提供 Renderer 可调用的任意 Shell 或通用 PTY。

## Considered Options

- 继续旧的四 Agent、MCP 与通用 PTY 路线。
- 在首发阶段加入完整编辑器、聊天和任意开发 Shell。

两者都会扩大权限边界并与当前仅支持 Pi/OpenCode 的产品定位冲突，因此不采用。

ADR-0071 曾将 P0 收窄为 OpenCode Server；该决策已由 ADR-0072 改为本机 Pi RPC。本文只保留早期 Pi/OpenCode 产品边界和安全范围历史，不作为当前执行器协议来源。
