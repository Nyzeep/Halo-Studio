---
status: superseded by ADR-0072 for P0 executor scope; amended by ADR-0078
---

# 使用本机已安装的 Pi 与 OpenCode 执行器

Halo Studio 首期将 Pi 和 OpenCode 作为用户本机已安装的原生命令行执行器分别探测、能力验证并启动，不下载、打包或升级它们。上游（原 BitFun）内置 Code Agent 继续由上游运行时承载（历史对照）；外部执行器只有在真实探测、启动和就绪检查通过后才可被选为受管执行器，使其版本、账户和供应链边界保持在用户可见的原生工具中。

ADR-0072 将 P0 改为本机 Pi RPC 单一生产受管执行适配；OpenCode Server 与上游（原 BitFun）内置 Code Agent 保留为历史或 P0 之后需要重新立项和验收的可能方向。

ADR-0078 进一步以双受管 Adapter 与统一执行器端口取代单一 Pi 路径，并修订本 ADR：Pi 继续作为本机已安装执行器，OpenCode 部分随之失效，不进入 2.0 受管路径。
