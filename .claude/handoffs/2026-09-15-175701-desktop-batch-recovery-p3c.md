# Handoff: 桌面任务效率功能与批量恢复 P3c-1

## Session Metadata

- Created: 2026-09-15，Asia/Shanghai。
- Project: D:/Develop/EggDone；两端均在 codex/task-productivity 分支，不在 main。
- 本端功能提交：96b9874，feat(batch): 支持重启恢复与安全结束未确认批次。
- 配套功能提交：桌面 96b9874，鸿蒙 5c41b09。
- 版本保持桌面 1.1.0 / SQLite 21，鸿蒙 1.2.0 / 1000029 / RDB 22，bundle com.eggdone.todo。
- 功能提交后工作区无遗留代码；本文、README 最新交接链接及 roadmap 检查点作为独立文档提交。
- 用户本次授权生成 handoff 并本地提交。不推送、不合并、不升版、不发布、不直接开始下一阶段。

## Handoff Chain

- Continues from: [上次完整任务草稿交接](2026-09-14-181404-desktop-checklist-full-draft.md)。
- 当前开发状态以本文及 roadmap 为准。旧文档中“模板和批量创建尚未实现”等仅是历史状态，不再适用。
- 配套交接：[另一端当前交接](D:/Develop/EggDoneHarmony/.claude/handoffs/2026-09-15-175705-harmony-batch-recovery-p3c.md)。
- 本轮之前 P3b 已提交：桌面 9ddc2d3 / 鸿蒙 3b3bf4d；P3a：c9589db / fd77e88。
- P2c 模板界面：桌面 0af348a / 鸿蒙 5f52ee5；P2b 同步与备份：38caf0d / a482d75。

## Current State Summary

本轮三个功能为任务内检查清单、任务模板与快捷复制、多行文本批量创建。清单新建与即时勾选、快捷复制、模板存储/同步/备份/管理入口及批量预览入口已经实现，不应重新规划成尚未开发。当前已完成 P3c-1：用户确认提交的批次写入本机恢复记录，重启打开批量面板后可原样重试或二次确认结束恢复，明确区分任务创建、恢复记录清理和列表刷新失败。代码与隔离自动化完成；P3c-2 真机键盘、平板、真实批次同步和原生恢复验收仍待完成，P3c 整体未勾选完成。

## Important Context

- 桌面入口：首页更多中的“批量创建任务”，在任务模板之后、搜索任务之前。鸿蒙入口：新增任务“选项 → 批量创建任务”。
- 每批最多 50 个非空行，原文最多 20000 UTF-16 单元，标题最多 100。重复标题只提示、不自动去重；用户逐行修改/勾选，选统一分组后再创建。
- 首版不自动解析标题时间，不设置日期/提醒/重复，不将 Markdown 已勾选标记转成完成状态。
- 只有确认提交的完整请求持久化，未提交原文/预览仍是内存草稿。取消草稿零新增，不要宣称所有草稿都支持强杀恢复。
- 恢复记录 task.batch.pending.v1 与事务回执 task.batch.create.v1:operation_uuid 含义不同。记录在 app_metadata，仅本机保存，不加入同步对象或 JSON 备份，不新增 schema。
- prepare 记录提交后才允许任务事务开始；任务/脏标记/回执在同一事务写入。记录失败不得继续创建。
- 重试必须保持完整 payload、operation UUID 和全部 task UUID。恢复只展示最终选中项并重新编号，不重新解析标题前缀；不自动创建。
- 已有回执须核对任务当前状态。修改/删除/依赖变化时拒绝旧回执，不能为通过测试而降低校验或复活任务。
- 创建成功但清理记录失败：会话保留已确认结果，下一次仅清理；列表仍刷新、同步仍调度。重启后无内存确认，按原回执重新核对。
- “结束恢复”是清除匹配记录，不是删除任务、撤销已提交结果或删除回执；必须保留二次确认和重复创建风险提示。
- 并发或损坏记录阻止新批次；不要静默覆盖、自动过期删除，尤其不要清空用户数据库。
- 本轮未修改小艺能力，不恢复小艺问题调查；用户此前已暂停该方向。

## Architecture Overview

Svelte TaskBatchDialog 持有 BatchCreationSession，API 层仅 invoke。原生 create_task_batch 在锁内先调用 prepare，再调用原批量事务，提交后发 todos-changed；load/forget 各有 IPC。列表刷新与同步由 store 编排，不在 UI 直接访问 SQLite。面板最大宽 520px，百分比高度约束；正文 13px、标题 15px，输入/选择框用内侧 1px 焦点描边避免左右被滚动容器裁切。

## Critical Files

| File | Purpose |
| --- | --- |
| src/lib/components/TaskBatchDialog.svelte | 加载、恢复、二次确认结束、刷新和焦点描边 |
| src/lib/utils/batchCreationSession.ts | 稳定请求、重启加载与已确认结果缓存 |
| src/lib/api/taskBatchApi.ts | create/load/forget Tauri IPC |
| src/lib/stores/taskBatchStore.ts | 会话工厂、列表刷新与同步调度 |
| src-tauri/src/task_batch.rs | 本机恢复记录以及批量任务与回执事务 |
| src-tauri/src/task_batch_commands.rs | prepare 先持久化，再调用批量事务 |
| src-tauri/src/task_batch_tests.rs | 文件数据库重开、失败、重复及墓碑保护 |
| scripts/test-batch-creation-ui.mjs | 生产 Svelte 组件浏览器矩阵及重载恢复 |
| docs/TASK_BATCH_RECOVERY.md | 本阶段完整行为与验证边界 |
| docs/TASK_PRODUCTIVITY_ROADMAP.md | 已完成代码与待人工验收分别记录 |
| docs/TASK_TEMPLATE_UI.md | P2c 模板入口和 P2d 待验收 |

## Files Modified

功能提交 96b9874 共 16 个文件。包括上表中的恢复相关业务文件、共享会话测试样例、中英文资源、README 和阶段文档；精确清单使用 git show --stat 96b9874。本次文档提交仅新增本文并更新 README 链接和 roadmap 检查点，不重复改业务代码，不包含构建产物、测试数据库、签名或临时截图。

## Work Completed

- [x] P3a 整批事务与稳定回执；P3b 双端输入、粘贴、预览、修改/勾选/分组、确认入口。
- [x] P3c-1 持久化提交请求、重启读取、安全重试、主动结束恢复和独立失败状态。
- [x] 源码检查、构建、共享会话、数据库重开及桌面浏览器回归。
- [x] 手机模拟器保留数据覆盖安装，批量入口、空态、焦点、取消冒烟。
- [ ] P3c-2 物理手机/平板键盘、分屏、大字体、真实批次同步与原生强杀恢复验收。
- [ ] P2d 模板完整人工矩阵、P3d 文档收口及 P4 发布回归，不因局部“测试通过”自动全选。

## Validation

以下完整证据来自 P3c-1 开发阶段；本次交接复跑快速项，并非再次跑完所有构建和设备测试：
- 桌面 Vitest：45 文件 / 397 项通过；提交前重跑 batchCreationSession 13 项通过。
- Rust：全量 334 通过 / 15 默认忽略；批量相关 11 通过；cargo check 与 fmt 检查通过。忽略项不能算作通过。
- 桌面 pnpm check：0 errors / 0 warnings；pnpm build 通过；双语 845 key 对齐及可见文本硬编码检查通过。
- 浏览器：24 组中英文、亮暗、320/480/1100 宽、1x/1.5x；另覆盖页面重载、清理失败只重试清理、二次确认取消、读失败、粘贴、50/51 行及刷新失败。
- 鸿蒙共享会话 13 项、生产 Store/Repository/Session 恢复脚本、生产面板方法测试通过；P3a 42 场景通过。
- 本次提交前复跑鸿蒙共享会话/双端源码一致性、恢复脚本、P0 18 组草稿/文档一致性，均通过。
- 鸿蒙最终 Debug 构建通过：21s721ms，33 tasks，19 executed / 14 up-to-date。
- MCP 会话/依赖、Store 和 Repository 无错误；初次 titleIssue 未导出报错在连同依赖重检后消失，实际文件已导出，不要重复修源码。面板资源异常注释、主题建议及项目既有警告仍存在。
- Windows TraySnapshot.locale 未使用警告保留，不在本阶段顺手重构。
- 手机模拟器 Mate 80 Pro Max，127.0.0.1:5555：覆盖安装保留数据，能打开面板、输入框获得光标、取消返回，未新增/修改任务。未出现软键盘，不能宣称键盘避让通过。
- 模拟器启动按用户现有配置自行同步；不是本轮新建批次的真实跨端同步验收。没有物理手机或平板测试证据。
- 最后清理提示调整已重新构建；模拟器安装/冒烟在该小调整之前，不能说模拟器已安装最终 HAP。

## Decisions Made

| Decision | Rationale |
| --- | --- |
| 恢复记录先于任务事务持久化 | 崩溃和回包丢失后仍能找到原请求 |
| 完整请求比较后才删除记录 | 不覆盖或清理另一个批次 |
| 成功清理、列表刷新独立于创建 | 防止用户因刷新失败重新创建 |
| 不持久化普通未提交草稿 | 保持本阶段只解决未确认提交恢复 |
| 保留旧回执状态校验 | 防止删除或修改后的任务被错误重建 |
| 不推送、合并、升版或发布 | 用户当前只授权 handoff 与本地提交 |

## Immediate Next Steps

1. 新会话先检查两仓库 git status、分支和最新提交，读取本文及 roadmap；不要按旧 handoff 重做已实现功能。当前用户请求到文档提交为止，不自动执行下一阶段。
2. 用户同意继续后，优先 P3c-2：用最终构建在真机手机/平板验证主动粘贴、软键盘、长预览、旋转/分屏、大字体、取消与重复点击。安装时保留数据，不使用卸载。
3. 让用户用明确测试标题创建少量任务，核对两端数量/分组/顺序和断网重连；不在用户库注入损坏。恢复故障注入继续使用隔离测试，必要时另经用户确认做原生进程退出实验。
4. 记录实际通过范围后进入 P3d，补 P2d/P4 未测项。只有另外授权才提交后续改动、升版、打包、合并或推送。

## Assumptions Made

- 目前两端同步语义及用户已有设置继续有效；本机脏标记测试不等同商业对象存储/物理设备验收。
- 未确认记录仍在本机库，正常同步/备份不得移到另一设备自动执行。
- 模拟器输入框只有光标的具体原因未诊断，不应直接认定为应用键盘缺陷或已通过键盘测试。

## Potential Gotchas

- 桌面新增 IPC 必须重建并重启原生程序，仅刷新前端会出现 command 不存在。
- 浏览器回归和 pnpm check/build 顺序执行；SvelteKit 生成配置会让 Vite 全页重载，导致测试误失败。
- 桌面 1.5x CSS zoom 下 computed outlineWidth 可能量化成 0.666667px，断言应验证内侧范围，不硬断言 1px。
- RDB 同时一个写事务，忙错误可重试原请求；不要绕过持久化记录或逐条新增。
- 原版 P3b 只有进程内恢复；本轮恢复记录在打开批量面板时加载，不是启动就自动创建。
- 包名 signed 不代表正式发布签名；当前 HAP 是 Debug 构建。
- 不删除旧 handoff；它们记录历史版本和验证边界，不是当前阶段完成表。

## Environment State

Windows PowerShell。两个独立仓库，鸿蒙实际工程根目录是 D:/Develop/EggDoneHarmony/EggDone。桌面 Rust 命令从 src-tauri 执行。使用 session-handoff、commit-work 生成和校验交接；业务阶段使用 DevEco CLI、MCP、ArkUI 开发和多设备适配技能。

当前 PATH 的 devecocli.exe 经 NVM 代理可能因 Author Software 路径空格失败。已验证绕过方式是在 PowerShell 用 & 调用以下 node.exe，并把 cli.js 作为下一个参数：
- C:/Users/caozhipeng/AppData/Local/Author Software/nvm/installs/v24.21.0/node.exe
- C:/Users/caozhipeng/AppData/Local/Author Software/nvm/installs/v24.21.0/node_modules/@deveco/deveco-cli/dist/cli.js

随后追加 build --product default --modules entry@default --build-mode debug。需要设备时先 device list；本轮仅一个手机模拟器，下一轮仍需重新查询，不猜测串号。
PLAYWRIGHT_PATH 指向 C:/Users/caozhipeng/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright。
宿主 ArkTS 脚本默认复用 DevEco Studio TypeScript；必要时提供 TYPESCRIPT_PATH。
相关环境变量：PATH、PLAYWRIGHT_PATH、TYPESCRIPT_PATH、PYTHONUTF8。交接脚本以 UTF-8 运行，不记录任何凭据。

本轮没有遗留的日志跟踪、构建或测试会话；UI 测试脚本自行关闭临时 Vite 和浏览器。模拟器由用户环境管理，不擅自停止。

## Artifacts And Evidence

- 最终鸿蒙 Debug 包：D:/Develop/EggDoneHarmony/EggDone/entry/build/default/outputs/default/entry-default-signed.hap；10371596 bytes，2026-09-15 17:43:26。未提交包文件。
- 桌面 pnpm build 产物在 D:/Develop/EggDone/build；本轮未生成新版桌面安装包。
- 临时构建/测试日志位于当前用户 TEMP，文件名 eggdone-p3c-build-final.log、eggdone-p3c-browser-final.log、eggdone-p3c-rust.log、eggdone-p3c-vitest.log。
- 最后浏览器截图目录：C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-batch-ui-1789465475600；模拟器截图 eggdone-p3c-keyboard.png 实际只有光标，无软键盘。
- 临时日志和截图可能被系统清理；以可复跑脚本与提交源码为持久依据，不提交这些产物。

## Related Resources

- [本阶段恢复与验证边界](../../../docs/TASK_BATCH_RECOVERY.md) 的实际仓库路径为 docs/TASK_BATCH_RECOVERY.md；导航优先使用 README 中链接。
- 规划：`docs/TASK_PRODUCTIVITY_IMPLEMENTATION_PLAN.md`、`docs/TASK_PRODUCTIVITY_ROADMAP.md`。
- 功能：`docs/TASK_BATCH_UI.md`、`docs/TASK_BATCH_CREATION.md`、`docs/TASK_TEMPLATE_UI.md`。
