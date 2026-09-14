# Handoff: 桌面检查清单完整任务草稿 P1d-2b-2

## Session Metadata

- Created: 2026-09-14 (Asia/Shanghai)
- Project: D:/Develop/EggDone
- Branch: codex/task-productivity
- Version: 1.1.0；本轮不升版。
- 功能提交：ffa62b5，feat(checklist): 统一任务设置与清单编辑草稿
- 配套另一端功能提交：603f286。
- 文档随后单独提交，不推送。桌面/鸿蒙 P1d-1：5d42581/3f1867a；P1d-2a：b23a167/197e655；P1d-2b-1：9997e30/d59afb3。

## Handoff Chain

- Continues from: [上一份发布交接](2026-09-13-071148-desktop-1-1-0-main.md)
- 当前开发阶段以本文为准，上一份发布历史保留。
- 两端配套交接均位于各自仓库 .claude/handoffs 下，文件名日期 2026-09-14，主题 checklist-full-draft。


## Architecture Overview

Svelte 清单面板持有外层草稿，ChecklistTaskDraft 纯逻辑与鸿蒙主体一致。TaskChecklistEditorSession 保存不可变基线及稳定请求身份，API/store 调用已有 Rust 全字段原子事务；保存成功后才调度同步。新增 resolve_checklist_rule_time 是只读 IPC，委托现有时区引擎，不提前写库。自定义重复编辑器以 draftOnly 运行。提醒继续由现有数据库扫描读取提交值。

## Critical Files

| 文件 | 用途 |
| --- | --- |
| src/lib/components/TaskChecklistDialog.svelte | 外层会话、设置展开、子规则草稿与错误保留 |
| src/lib/components/ChecklistTaskFields.svelte | 紧凑任务设置控件 |
| src/lib/components/RecurrenceEditor.svelte | draftOnly、draftForm、onApply |
| src/lib/utils/checklistTaskDraft.ts | 跨端一致的完整草稿编排 |
| src/lib/utils/taskChecklistEditorSession.ts | 基线冲突与稳定重试 |
| src/lib/api/taskChecklistEditorApi.ts | 完整读写和只读时间解析 |
| src-tauri/src/task_checklist_editor.rs | 任务/规则/定义/子项原子保存 |
| src-tauri/src/task_checklist_commands.rs | 时间解析命令 |
| scripts/test-task-checklist-ui.mjs | 浏览器 mock IPC 回归 |

## Files Modified

功能提交 ffa62b5 共 18 文件：新字段组件、完整草稿工具和测试；清单面板、重复编辑器、TodoItem groups 传递、API、Rust 注册；双语文案、共享测试样例、UI fixture、README 和阶段文档。用 git show --stat ffa62b5 核对精确清单。文档提交只更新 handoff、README 链接及阶段检查点。

## Validation

完整验证来自功能实现阶段，本次交接未重复全部耗时测试：
- pnpm check：0 errors / 0 warnings；pnpm build 通过。
- pnpm i18n:check：761 个键对齐，硬编码检查通过。
- pnpm exec vitest run --maxWorkers=1：40 文件、359 项通过。
- cargo fmt -- --check、cargo check --locked 通过；保留既有 TraySnapshot.locale 未使用警告。
- cargo test --locked --lib：306 通过、13 默认忽略、0 失败；忽略项需额外环境，不能计作通过。
- UI fixture：24 基础 + 8 密度 + 8 范围 + 8 完整设置，共 48 组；覆盖中英文、亮暗、小大宽度、150% 缩放，另验证内外层取消、转换与稳定重试。
- 真实 Svelte/Vite + Edge headless 使用模拟 IPC，不等于原生 Tauri 数据库验收。临时截图目录 eggdone-checklist-ui-1789380248975，不提交；部分截图有故意注入的失败提示。
- 本次交接重跑配套鸿蒙完整草稿 10 组/源码一致性及日期选择器回调，均通过。文档验证在提交前执行。

## Environment State

Windows PowerShell。pnpm runtime：C:/Users/caozhipeng/.cache/codex-runtimes/codex-primary-runtime/dependencies/bin/fallback，加入进程 PATH。PLAYWRIGHT_PATH 使用该 runtime 的 dependencies/node/node_modules/playwright。Rust 命令在 src-tauri 目录运行。相关变量名称：PATH、PLAYWRIGHT_PATH、PYTHONUTF8，不存凭据。

无本轮遗留的测试/构建会话。UI fixture 在 finally 关闭自身服务器与浏览器；它与 pnpm check/build 必须顺序运行，避免生成配置冲突。原生验收须重建重启二进制，新 IPC 不能仅热更新前端。

提交前已知 src-tauri/Cargo.toml 显示 modified 但无文本差异，属于既有换行/元数据噪声，本次未暂存或回退，后续不要擅自清理。


## Current State Summary

任务效率计划仅包含任务内检查清单、任务模板与快捷复制、多行文本批量创建。P0、P1a、P1b、P1c 及 P1d-1/P1d-2a/P1d-2b 已实现，当前收口 P1d-2b-2“任务设置与检查清单统一草稿”。已有任务的清单面板内可统一编辑任务字段、重复规则和子项。新建时附带清单、已保存详情独立即时勾选、模板与批量创建尚未交付。本次只提交当前功能与交接，不开始下一阶段，不推送、合并或发布。

## Work Completed

- 清单面板的“任务设置”默认折叠；原普通编辑入口保留，没有替换所有旧编辑流程。
- 同一草稿包含标题、备注、日期、时间、提醒、分组、重要程度、重复和清单；最终保存才原子写入，取消不落库，失败保留草稿。
- 自定义重复子面板确认仅更新外层草稿，子面板取消不应用临时配置；规则已应用值和子编辑器临时值分离。
- 全天日期与定时时间正确区分；修改定时任务日期保留时分。提醒独立，不随日期或规则暗中变化。
- 普通重复选择“本次及以后”时原子转换规则与清单定义；未来继承内容和顺序但不继承勾选，当前勾选和历史不重写。
- 停止重复保留历史、当前清单与原提醒；本次没有新增迁移、同步对象、权限或依赖。

## Decisions Made

| 决策 | 原因与边界 |
| --- | --- |
| 默认保留规则 | 仅打开面板不转换普通重复；明确选择预设或自定义才改变本次及以后 |
| 复用完整保存会话 | 原始不可变快照检查冲突，稳定请求标识支持失败重试，提交成功后的刷新失败不重复写入 |
| 草稿内勾选仍需保存 | 独立即时勾选属于 P1d-2d，不能提前宣称完成 |
| 后续定义上限 20 项 | 更大的导入清单允许仅当前编辑，不静默截断 |
| 复用时间引擎 | 不重新实现时区、工作日或每月日期计算；新提醒必须未来，未修改旧提醒可保留 |

## Important Context

当前开发分支是 codex/task-productivity，不是 main。不要依据旧 handoff 的 main 状态切分支。当前数据库 schema 为桌面 SQLite 20、鸿蒙 RDB 21，检查清单备份已到 v4；旧客户端不能恢复 v4，需保留旧备份。旧格式 v1/v2/v3 导入保留本地清单。已有隔离同步与跨端备份测试不代表用户设备和真实 S3 的验收。历史用户“测试通过”不应扩大为本轮完整设置的手机、平板、键盘、系统提醒验收。小艺、实况窗、加密、统计及旧体验 roadmap 不在当前任务范围。

## Immediate Next Steps

1. 先核对两端分支、HEAD 和工作树，阅读共享 roadmap、契约及完整草稿 UI 文档；本轮用户只授权 handoff 与提交，不自动继续编码。
2. 用户再次要求继续时执行 P1d-2c：新建任务附带清单，复用已有创建事务，取消零写入，稳定创建回执避免重试重复任务；先确认创建入口和数据契约。
3. 然后执行 P1d-2d 已保存详情独立即时勾选，之后 P1e 综合回归与原生验收；P2 模板/复制、P3 多行创建按顺序交付，不跳到发布。
4. 验收已有任务统一草稿：取消重开无变化；保存后任务字段与清单一致；普通重复转换后下一项未勾选；自定义内外层取消无写入；提醒修改/清除后验证系统通知及真实双端同步。

## Pending Work

无已知阻塞开发的工程问题。P1d-2c、P1d-2d、P1e、P2、P3 未完成；原生数据库、真实 S3、系统提醒、手机/平板键盘和完整语言/字体组合仍待专项验收。本次未安装设备、未读取用户数据库或同步凭据，没有授权升版、推送、发布。

## Assumptions Made

- 两端继续共享数据语义和草稿编排，不共享本地数据库文件。
- 本轮构建和宿主/浏览器测试仅证明其覆盖范围，不能替代物理设备及真实存储验收。
- 当前分支已有前序检查清单迁移、备份与同步实现，后续以当前源码和 roadmap 为准。

## Potential Gotchas

- 默认 keep + 本次及以后，若任务日期与旧规则不匹配会明确拒绝；选择仅本次或明确编辑自定义规则，不能偷偷修复日程。
- 完成、只读、历史或重复身份不明确的任务不可配置新规则；旧普通重复与活动自定义规则并存视为身份不明确。
- 自定义次数初值使用剩余次数（包含当前）；用户明确换预设或改次数时使用新配置，不能误保留旧总次数。
- 当前勾选保留，未来新项未勾选；不能把当前草稿保存成功等同于历史任务全部更新。
- 不提交构建产物、截图、签名、凭据或本地数据库。提交后刷新/同步错误不得触发重复保存。

## Related Resources

- [实施方案](../../docs/TASK_PRODUCTIVITY_IMPLEMENTATION_PLAN.md)
- [共享契约](../../docs/TASK_PRODUCTIVITY_CONTRACT.md)
- [阶段 roadmap](../../docs/TASK_PRODUCTIVITY_ROADMAP.md)
- [完整草稿 UI 与验收步骤](../../docs/TASK_CHECKLIST_FULL_DRAFT_UI.md)
- [编辑会话](../../docs/TASK_CHECKLIST_EDITOR_GATEWAY.md)
- [范围选择](../../docs/TASK_CHECKLIST_SCOPE_UI.md)
