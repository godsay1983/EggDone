# Handoff: EggDone 桌面端自定义重复 E3A

## Session Metadata

- 日期：2026-09-06，Windows / PowerShell。
- 仓库：D:/Develop/EggDone；分支 main；当前功能提交：94021a7。
- 版本 1.0.7，SQLite schema17。本轮均未改变。
- 功能提交后工作树仅新增本文，无未提交业务代码。
- 本次计划将积累的功能提交和本文一起推送 origin/main；GitHub 镜像不在本次范围。

## Handoff Chain

- Continues from: [上一份桌面交接](./2026-09-05-104204-desktop-sync-runtime-ns1.md)。
- 本文替代上一份的“当前开发进度/下一步”，旧文保留 NS1 历史依据。
- 鸿蒙对应交接：[鸿蒙 E3A](D:/Develop/EggDoneHarmony/.claude/handoffs/2026-09-06-024725-harmony-recurrence-e3a.md)。

## Recent Commits

- 94021a7：规则创建、替换和停止事务 E3A。
- 25884a6：删除跳过与系列停止 E2。
- 425ae08：普通完成事务和幂等刷新 E1。
- 90b2d55：规则轮询与迟到探测隔离 B3。
- 5c0473a：规则发现进入主同步 B2。
- cce1bfc：规则同步适配与配置世代保护 B1。

## Current State Summary

当前开发主线为 NS5 自定义重复任务。E3A 已完成内部规则创建、替换和停止，E1 普通完成、E2 删除/跳过、规则同步主流程此前已接通。自定义规则 UI 仍关闭，没有新增用户可点的编辑入口；下一步是 E3B 规则备份恢复，然后 E3C 摘要/UI/提交后系统刷新及其他入口复核。不要依据旧 handoff 把当前进度退回 NS1，也不要把内核自动测试通过当作可发布验收。

## Important Context

- 用户本次明确要求两端生成 handoff、提交并推送。此次只推送两端当前跟踪的 origin/main，不推送桌面 github 镜像，不强制推送。
- 本文是在功能提交后、handoff 文档提交与推送之前生成的快照。文档提交自身哈希和最终推送状态以 Git 为准；新会话先核对状态，不能因本文的“推送前快照”而重复开发或假定仍未推送。
- 本轮不改版本号、数据库 schema、签名或已通过华为验收的专注实况窗。
- 两端业务使用 UUID 对齐，但各自数据库独立。规则在独立 recurrence-rules.json 对象同步，不能塞进旧 Todo JSON 的 repeat_rule 枚举。
- 保存入口要求新的规则为初始状态，且目标任务存在、未完成、未删除、未归档。仅普通未关联任务可直接附加规则；旧快捷重复转换、已停止/耗尽规则重启仍未实现。
- 修改规则不是原地改 schedule：新 UUID + 旧规则墓碑 + 当前项作为新首项，全部同事务；历史保留，新次数从 1 重新计算。
- 保存请求携带目标任务 updated_at 和旧规则完整快照，任何过期编辑拒绝，不覆盖同步或完成动作。
- 保存会应用新日期/时刻并清除旧提醒。后续 UI 必须明确提示并支持重新设置，不能静默丢失提醒；保存后列表/通知/卡片刷新尚未接编辑入口。
- recurrence.edit.v1 回执防止不确定结果的重试重复创建；必须保留原请求与 UUID。回执存在但规则缺失时不得复活。回执恢复策略是 E3B 的必做项。
- E2 的“删除本次”就是 skip；“删除整个系列”同时停止规则并删除本机已有活动项；stop-only 只停规则、保留任务和提醒。恢复任务不重启规则、不撤回已生成的下一项。
- 凭据、签名材料和真实用户数据不得写入代码或文档；本交接没有记录任何凭据值。

## Decisions Made

| 决策 | 原因 |
| --- | --- |
| 规则不可变字段修改必须换 UUID | 保持跨端冲突合并和确定性下一实例一致 |
| 任务与规则共同事务 | 避免只停止旧规则或只修改任务的半完成状态 |
| 本地保存回执与乐观并发校验 | 区分完全相同重试和过期编辑，不倒退进度 |
| 编辑写任务时校验标准 UUID 设备标识 | 规则协议允许 ASCII 设备名，但 Todo 同步要求 UUID |
| 全天保留字面日期、定时固定 IANA 时区 | 跨端同步不受设备时区漂移影响；未知时区不能回退 |
| UI 等备份与其他入口复核后再开放 | 不能提供用户可编辑却无法安全备份/恢复的功能 |

## Immediate Next Steps

1. 核对两端 main、origin 跟踪状态和最新 handoff；阅读两端 roadmap 的 E3A/E3B 段及规则编辑契约，不重新实现 E1/E2。
2. 开始 E3B 前检查现有 JSON、完整附件备份、数据库备份和恢复路径，明确每种格式是否包含规则；冻结双端一致的新备份兼容方案。
3. 设计旧备份缺少规则时的保留/合并语义，以及规则墓碑、当前实例关联、实例回执和编辑回执的恢复/失效策略。不要直接复制本机 ETag、synced_revision 或运行状态到另一安装。
4. 同时实现两端备份规则元数据与校验，覆盖旧备份、格式损坏、任务缺失、停止规则不复活、恢复失败回滚和恢复后同步，不开放 UI。
5. 再推进 E3C 规则可读摘要、创建/编辑/停止交互和错误国际化，确认清除旧提醒提示、提交后刷新、快捷规则转换及停止后重新创建交互。
6. 清空已完成/归档及其他任务编辑入口必须专项复核。后续记录真实 S3 离线并发、原生系统和视觉验收；无证据的人工门槛保持未完成。

## Assumptions Made

- 本轮只处理本地规则事务，S3 同步仍沿用前台触发和约 60 秒轮询，不承诺后台实时。
- 新任务自身仍由普通 Todo 新增入口保存；规则编辑内核给已保存任务附加规则，不宣称新任务与规则已一次原子创建。
- 停止前另一设备已生成的任务按照 Todo 合并规则保留；不保证删除尚未下载到本机的任务。
- 离线两端同时改周期可能产生规则关联冲突。当前协议阻止静默覆盖，自动解决与用户交互未完成。

## Potential Gotchas

- 老 roadmap 的早期阶段段落是历史记录；进度优先看最新 E3A、共享契约和实际代码，不把旧“尚未接入”段落直接当现状。
- 自定义重复与旧四种快捷重复算法必须隔离；不能通过清空 repeat_series_uuid 或墓碑强行“修复”冲突。
- 回执、规则进度和 dirty 同事务；不得用吞异常、重复生成或只重载界面掩盖失败。
- 恢复备份时不能复用原桶 ETag 或清除尚未上传的新修改；规则上传前必须确保相应 Todo 已上传。
- 手动测试可以后补，但构建、宿主 SQLite、平台替身不能代替原生 RDB、通知、卡片及真实 S3 证据。

## Pending Work

- [ ] E3B 规则元数据与回执备份恢复。
- [ ] E3C 摘要、UI、本地化、编辑后系统刷新及快捷重复转换。
- [ ] 清空已完成、归档、其他日期/重复编辑入口复核。
- [ ] 真机/真实 S3/离线并发及中英文亮暗视觉验收。
- [ ] 后续发布版本、签名、商店材料；当前用户没有要求升级版本或发布应用。

## Related Resources

- [规则编辑契约](../../docs/RECURRENCE_EDITOR_CONTRACT.md)
- [普通操作契约](../../docs/RECURRENCE_TASK_ACTION_CONTRACT.md)
- [规则同步会话契约](../../docs/RECURRENCE_SYNC_SESSION_CONTRACT.md)
- [规则 v1 协议](../../docs/RECURRENCE_RULES_PROTOCOL.md)
- [现有附件备份格式](../../docs/NOTES_ATTACHMENTS_BACKUP_FORMAT.md)

## Architecture Overview

SvelteKit 管理 UI/API/Store；Rust 管理 SQLite 事务、同步和规则计算。recurrence_editor 模块不是新 Tauri command，前端尚不能创建自定义规则。普通完成和删除由 commands 调用共享事务；同步快照绑定/补算规则后依次上传 Todo 与规则，按版本确认。全天日期在桌面存 due_date，due_at 为空；定时日期使用固定 IANA epoch。规则生命周期业务不要移进组件或托盘模块。

## Critical Files

| File | Purpose |
| --- | --- |
| src-tauri/src/recurrence_editor.rs | E3A save_rule、stop_rule、RuleEditRequest 和回执 |
| src-tauri/src/recurrence_editor_tests.rs | 10 项编辑事务回归 |
| src-tauri/src/recurrence_transaction.rs | complete、skip、reconcile 与 stop-series 内核 |
| src-tauri/src/recurrence_store.rs | 严格快照、规则合并、dirty ACK |
| src-tauri/src/recurrence_snapshot.rs | 原子 Todo/规则同步快照及补算 |
| src-tauri/src/recurrence_sync_session.rs | 规则生产会话、配置世代和主同步边界 |
| src-tauri/src/commands.rs | 普通任务入口、同步及已有导入导出入口 |
| src/lib/stores/todoStore.ts | 完成/删除结果 UUID 去重、批量部分成功刷新 |
| docs/NEXT_STAGE_ROADMAP.md | 两端功能推进与待验收门槛 |

## Work Completed

- [x] 两端规则同步发现、适配、配置切换与迟到探测隔离。
- [x] 普通完成、删除/跳过、系列停止及恢复语义。
- [x] E3A 普通任务附加规则、当前项换周期、停止规则内核。
- [x] 编辑过期检查、重试回执、不可变规则与历史保留。
- [x] 回归及共享契约/README/roadmap 更新。
- [x] 中文功能提交；本次 handoff 独立文档提交随后执行。

## Files Modified

94021a7 包含新增 recurrence_editor.rs、recurrence_editor_tests.rs 和共享编辑契约；在 lib.rs 注册模块，并更新 README、roadmap、普通操作契约。没有新增 UI 命令。此前 E2 已修改 commands、recurrence_transaction、Todo API/Store 和回归，已在 25884a6 中提交。本次后续只新增本文。

## Verification Evidence

- 上一开发回合完整 pnpm release:check：206 Rust / 119 前端测试通过；Svelte 0 errors / 0 warnings；生产构建、Cargo fmt/check 通过。
- 本交接回合重新执行 cargo test recurrence_editor --lib：10 passed / 0 failed。
- 鸿蒙本交接回合重新执行 32 项规则编辑宿主场景通过；此前 19 快照、22 删除、19 完成、39 同步会话通过。
- git diff --cached 已复核；git diff --cached --check 无错误。
- 保留原有 TraySnapshot.locale 未使用警告。本轮没有执行桌面窗口/托盘/通知、真实 S3、升级安装或其他人工验收。

## Environment State

- 运行目录：桌面命令从仓库根执行，Cargo 从 src-tauri 子目录执行。
- 可复现命令：pnpm release:check；cargo test recurrence_editor --lib。
- 日志在 Windows TEMP：eggdone-editor-release.log、eggdone-editor-handoff-tests.log。日志为本机证据，不提交仓库。
- 当前没有本任务遗留的构建、测试或开发服务器进程。
- 交接脚本使用 PYTHONUTF8 避免中文编码问题；不记录任何凭据值。
- 后续 Git 状态先用 git status --short --branch 和 git log -3 核对，再读取本地和远端 main，勿强制覆盖远端。

## Resume Entry

优先读取 [桌面 roadmap](../../docs/NEXT_STAGE_ROADMAP.md) 的 DNS5-E3A，按 Immediate Next Steps 实施 E3B。不要直接启用自定义规则 UI，也不要把两端全部 NS5 勾为完成。
