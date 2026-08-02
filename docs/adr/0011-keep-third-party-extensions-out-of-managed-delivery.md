---
status: accepted
---

# 首期受管交付不加载第三方扩展

Halo Studio 保留 BitFun 的 MCP、Skills、插件和自定义 Agent 能力给标准编码模式使用，但首期受管交付仅允许 Pi RPC 的固定原生工具集和一份 Halo 第一方、固定版本的决议 extension。P0 以 `--no-extensions` 禁止发现式加载，只显式加载经过来源、依赖、权限和许可证审计的 extension；不加载项目本地、用户全局或任意 npm/git 第三方扩展。历史 OpenCode Server 不构成受管回退。未来若要纳入其他扩展，必须另行定义来源审核、版本追踪、权限和证据边界。该决定避免扩展生态在首期绕过受管交付的可审查性。
