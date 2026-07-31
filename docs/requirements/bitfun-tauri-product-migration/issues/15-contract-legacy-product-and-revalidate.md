# 15 - 收缩旧产品实现并完整复验

**What to build:** 在 OpenCode-backed Tauri 产品通过全部迁移门槛后，维护者通过独立变更删除旧 QML、旧 Sidecar 和旧启动入口，使 Halo 只剩一个正式桌面产品与一个权威 Workbench Runtime，并在最终仓库状态重新获得发布证据。

**Blocked by:** 14 - 完成真实 OpenCode 原生 UI 验收.

**Status:** ready-for-agent

## 验收标准

- [ ] 删除范围由引用审计和工单 12 行为等价矩阵确定，只移除已被新产品替代的旧入口、Adapter、传输、测试和文档。
- [ ] 旧 `sidecar/crates/halo-runtime/src/opencode.rs` 中仍需保留的行为语义和测试夹具已迁入新的 OpenCode Server Adapter 后再删除；不保留 JSONL 或双运行时桥接。
- [ ] 仓库、脚本、权威文档和发布配置只指向 `product/Halo Studio` Tauri 产品，不再提供旧 PySide/QML、Python 或 Sidecar 产品入口。
- [ ] 新 Halo Workbench Runtime、OpenCode Adapter、受控替身和行为等价证据不得被误判为“旧 Sidecar”删除。
- [ ] 删除后重跑完整 Tauri 构建/打包、Rust/前端契约、OpenCode Adapter 集成、桌面端到端、行为等价矩阵、同步演练和许可证检查。
- [ ] 删除后再次完成真实 OpenCode 原生 UI 主链与中断验收；任何失败或未执行项阻止 P0 放行。
- [ ] 最终扫描不存在旧入口、外部 `D:\BitFun-main`/`D:\opencode-dev` 依赖、Pi/Code Agent P0 选择器或 OpenCode 内部源码副本。

## 安全边界

- 本票不自动删除用户工作区、应用数据、系统凭据、Git 分支、提交或远端。
- 删除清单必须先提交审查；任何来源不明或仍被正式构建引用的文件不得删除。
