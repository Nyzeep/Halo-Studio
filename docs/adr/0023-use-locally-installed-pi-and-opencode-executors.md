---
status: superseded by ADR-0072 for P0 executor scope
---

# 使用本机已安装的 Pi 与 OpenCode 执行器

Halo Studio 首期将 Pi 和 OpenCode 作为用户本机已安装的原生命令行执行器分别探测、能力验证并启动，不下载、打包或升级它们。BitFun 内置 Code Agent 继续由 BitFun Runtime 承载；外部执行器只有在真实探测、启动和就绪检查通过后才可被选为受管执行器，使其版本、账户和供应链边界保持在用户可见的原生工具中。

ADR-0072 将 P0 改为本机 Pi RPC 单一生产受管执行适配；OpenCode Server 与 BitFun 内置 Code Agent 保留为历史或 P0 之后需要重新立项和验收的可能方向。
