# 双端任务生命周期综合回归

日期：2026-09-19。阶段：LC5 自动化收尾，不等于发布或完整真机验收。

## 当前结论

- 用户已反馈上一轮等待界面与任务菜单修复测试无问题。
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

## 最短剩余验收

本轮不新增功能，只收尾现有四块能力。建议用一个临时测试任务确认下面未逐项记录的组合：

1. 桌面加入今日计划并设为等待，同步；鸿蒙确认任务、标签、原因和复查日期，恢复处理再同步，桌面等待标签消失。
2. 同一任务完成后不再出现在等待列表；删除并恢复后，旧计划/等待不自动复活，重新安排仍可用。
3. 仅对该测试任务执行单项彻底删除并同步；另一端同步后不再出现，正常任务和关联便签仍在。不必再次清空已有回收站资料。

手机/平板、分屏、语言/主题/字号、跨日、离线并发、真实附件清理、通知/卡片/小艺/实况窗未覆盖的组合继续作为发布风险记录，不用 host 测试替代。小艺平台审核与真实语音触发也不在本轮测试结论中。

用户确认后整理版本说明及交接；升版、打包、推送或发布需另行明确授权。本轮新增测试与文档按用户要求纳入本次本地提交，未推送。
