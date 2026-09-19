# 双端任务生命周期综合回归

日期：2026-09-19。阶段：LC5 自动化收尾，不等于发布或完整真机验收。

## 当前结论

- 用户已反馈上一轮等待界面与任务菜单修复测试无问题。
- 2026-09-19 用户确认跨端往返验收通过：桌面加入今日计划并设为等待（保留计划），同步到鸿蒙同时显示两个标签；鸿蒙恢复处理并同步回桌面，等待标签消失、今日计划保留。
- 综合回归已本地提交：桌面 `660f3b6`，鸿蒙 `eb68078`；未推送。本次验收记录为后续文档更新。
- 上一轮 UI 修复本地提交：桌面 `7212bdc`，鸿蒙 `564415c`；未推送。
- 本轮增加双端生产命令/仓储串联测试，未修改产品业务逻辑、数据库版本、同步协议或应用版本。
- 测试使用隔离 SQLite、测试传输和样例数据，不读写用户资料或云端空间。
- 用户未逐项确认的跨端同步、原生 RDB、系统能力和设备矩阵仍保留；不重复要求验收已确认的界面修复。

## 新增串联测试

桌面：`src-tauri/src/task_lifecycle_journey_tests.rs`。
鸿蒙：`scripts/test-task-lifecycle-journey.cjs`。
共用已有 `docs/fixtures/archive-storage-v1.json` 种子，调用生产代码；仅平台接口由 host adapter 代替。

两端各 4 条：新库/重建 schema 25 后升级库，分别覆盖等待时保留计划/同时移出计划。

1. 已归档任务重新打开，重复同一操作幂等；保留原截止日期，不复活旧提醒或重复规则。
2. 加入今日计划并设置等待，任务正文及原字段不变；复查日期和计划保留/移除正确。
3. 完成后等待消失；保留计划的任务显示完成；再次归档保留清单和有效关联。
4. 取消归档保持完成，重新打开后旧计划/等待副本不能重新生效。
5. 删除后恢复；过期恢复预览拒绝；清单保留，不复活已删除的关联，独立便签保留。
6. 显式重新计划/等待可用；再次删除后选定单项彻底删除，同一批次重试幂等。
7. 旧计划/等待合并及旧备份不能复活彻底删除的任务；无关归档任务和便签不受影响。

这不是从界面点击到真实云端的端到端测试。schema 25 是由当前库移除等待域后重建的测试库，不是用户历史数据库。

## 本轮验证

桌面在 `src-tauri` 目录执行：

| 命令 | 结果 |
| --- | --- |
| `cargo test --lib lifecycle_journey -j 2` | 新增 4 项通过 |
| `cargo test --lib -j 2` | 481 通过，40 按原配置忽略，0 失败 |
| `cargo check` | 通过，保留既有未使用代码告警 |
| `cargo fmt --all -- --check` | 通过 |

鸿蒙在仓库根目录用 `node scripts/<文件名>` 执行。以下每个脚本均通过：

| 范围 | 脚本 |
| --- | --- |
| 新增串联 | `test-task-lifecycle-journey.cjs`，4 条 |
| 归档、回收站 | `test-archive-storage.cjs`、`test-trash.cjs`、`test-purge.cjs` |
| 清理协调与远端保护 | `test-purge-sync-coordination.cjs`、`test-purge-remote.cjs` |
| 今日计划 | `test-daily-planning.cjs`、`test-daily-planning-sync.cjs`、`test-daily-planning-remote-apply.cjs`、`test-daily-planning-backup.cjs` |
| 等待处理 | `test-task-workflow.cjs`、`test-task-workflow-store.cjs`、`test-task-workflow-integration.cjs`、`test-task-workflow-http.cjs` |
| 重复及完成 | `test-recurrence-backup.cjs`、`test-todo-completion-recurrence.cjs` |
| 清单、模板 | `test-task-checklist-inheritance.cjs`、`test-task-checklist-backup.cjs`、`test-task-template-backup.cjs` |
| 自动接续、意图、提醒 | `test-auto-join-host.cjs`、`test-intent-capture.cjs`、`test-reminder-completion.cjs` |

完整回归修正了三份过时测试适配：
- 清理协调测试补齐新增的设置/计划刷新和同步确认接口，并断言失败时不确认。
- 旧 v5 备份样例去除新版等待域字段，保持生产格式校验不变。
- 提醒完成测试复用现有生产迁移/仓储 loader，补齐清理、计划和等待域依赖；9 项通过，未模拟跳过迁移。
- 备份 harness 支持无重复种子的隔离空库，供新串联测试使用；原 22 条重复备份用例继续通过。

没有重新运行原生安装包构建、浏览器视觉矩阵或真实 S3 套件；上一轮的 UI/build 结果不冒充本轮执行。被忽略的 Rust 测试不计为通过。

## 用户验收与剩余范围

本轮不新增功能。上述计划/等待/恢复处理跨端往返已由用户确认通过，不再要求重复测试；没有将此反馈扩大到原因、复查日期、跨日或并发组合。下面的额外组合尚未逐项确认，仅保留为发布验收范围：

1. 等待原因和复查日期跨端一致，跨日前后状态正确。
2. 等待任务完成后不再出现在等待列表；删除并恢复后，旧计划/等待不自动复活，重新安排仍可用。
3. 对测试任务单项彻底删除并同步后，另一端不再出现，正常任务和关联便签仍在。不必再次清空已有回收站资料。

手机/平板、分屏、语言/主题/字号、跨日、离线并发、真实附件清理、通知/卡片/小艺/实况窗未覆盖的组合继续作为发布风险记录，不用 host 测试替代。小艺平台审核与真实语音触发也不在本轮测试结论中。

跨端核心联动验收通过后，用户授权桌面升至 1.3.0、鸿蒙升至 1.4.0 / 1000031，并生成交接、本地提交。CHANGELOG、README、roadmap 与交接同步更新；不新增迁移或后台开发阶段，不推送、安装或正式发布。

## 升版验证

2026-09-19 本次重新执行：桌面 `pnpm check`（0 错误、0 警告）、`pnpm test`（486 通过）、`pnpm build`、`cargo fmt --all -- --check`、`cargo check --locked` 及版本一致性检查通过。保留前端大 chunk 和 Rust 未使用代码告警。

鸿蒙 `devecocli build --product default --build-mode debug` 成功（42 个任务，32 执行、10 最新；42.946 秒）；生成 HAP 内版本与 pack.info 均为 1.4.0 / 1000031，buildVersion 保持 1。保留 ArkTS 弃用、可能抛异常、权限提示及重复组件 ID 等既有警告，不扩大为正式包签名或设备安装验收。

上文 481 项 Rust 与 22 个鸿蒙脚本是升版前的本轮综合回归，不声称升版后再次全跑。版本变更没有修改业务源码或 schema；未测试矩阵仍保留。
