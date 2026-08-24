# 03 - 建立受管事件事实契约与内存 Adapter seam

**待实现内容：** 让 Halo Workbench Runtime 拥有一个小而稳定的事件事实 Interface，并提供纯内存 Adapter 作为可替换测试实现。调用方可以为单个受管任务追加经允许的、版本化的事实并读取其有序历史；相同事实身份的重复提交具有确定的幂等结果。这个垂直切片交付可测试的 Halo 事实契约，而不宣称已经持久化、投影证据或接入真实执行器。

**阻塞于：** 无（可立即开始）。

**状态：** ready-for-agent

## 实现边界与架构边界

- Interface 只表达 Halo 本地事实：任务身份、事实身份、顺序、记录时间、schema 版本、闭合事实种类和 Runtime 所有的 opaque 摘要值。它不暴露通用 JSON、原始 Pi/DSH 事件、原始 session 标识、工具参数、工具输出或凭据的字段；只有后续 Runtime normalizer 才能产生生产摘要。
- 事件事实 Module 是 Runtime 的内部依赖 seam；内存 Adapter 与后续持久化 Adapter 满足同一 Interface。Module 把去重、顺序、旧 schema 读取和不可变读取视图收口，调用方和测试不窥探其实现。
- 交付证据、任务基线、中断快照、Renderer 广播和 Pi RPC Adapter 保持各自职责。现有瞬时 UI event 与替换式中断记录都不得被重命名为事件事实日志。
- 首片不将存储读取能力暴露给前端，也不引入新的 Tauri command、数据库、文件格式、后台服务器或第二执行 Adapter。

## 验收标准

- [ ] 可通过窄的 typed Interface 追加并读取单个任务的 Halo 事件事实；读取结果按任务内顺序稳定排列且不允许调用方修改已保存事实。
- [ ] 相同事实身份的重复 append 不生成第二条历史；不同身份但相同摘要仍保留为独立事实。
- [ ] 事实种类是闭合且可审计的，至少能表达后续生命周期、消息摘要、工具活动、一次性决议、文件指纹和证据变化；未知/不安全 schema 或种类有明确的 fail-closed 结果。
- [ ] 内存 Adapter 证明同一 Interface 可被测试替身满足；测试通过 Interface 断言顺序、重复和旧 schema 读取，而不是检查私有集合或锁。
- [ ] 契约以 opaque 摘要值而非原始外部载荷表达内容，因而没有接收完整 prompt/response、密钥、原始工具参数/输出、原始 Pi/DSH JSONL 或外部执行器原始 session log 的结构化入口；统一脱敏与大小上限 normalizer 在 04 实现，首片不提前宣称内容过滤已经完成。
- [ ] 首片验证不声称真实 Pi、真实 DSH、持久化、重启恢复或交付证据已经通过。

## 验证要求

- 先写红色契约测试，再以最小实现转绿；每轮只覆盖一个可观察行为。
- 运行事件事实 contract crate 的 focused test、Workbench Runtime 既有公开契约测试和 `git diff --check`。
- 变更到 Rust 时按产品规则运行最小格式化与边界检查；完整工作区测试留给最终集成验证。

## 不在本票

- 将事实 Module 注入生产 Runtime、磁盘持久化、删除策略、重启恢复。
- 交付证据投影、新鲜度失效和 UI 展示。
- DSH/Pi 的真实 session、JSONL 或模型验收。
