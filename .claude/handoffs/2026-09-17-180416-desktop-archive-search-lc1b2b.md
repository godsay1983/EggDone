# Handoff: 桌面归档搜索恢复与列表层次 LC1b-2b

## Session Metadata

- Created: 2026-09-17 18:04，Asia/Shanghai。
- Project: D:/Develop/EggDone
- Branch: main
- Implementation commit: 468159b; peer implementation commit: 1f374bc。
- 本次为上一实现回合后的复核、生成 handoff 和本地提交，不启动下一开发阶段。
- 文档单独提交，最终文档提交号请查 git log -2；无需在文件中记录自身尚未生成的哈希。

## Handoff Chain

Continues from: [2026-09-16-131415-desktop-window-resize.md](./2026-09-16-131415-desktop-window-resize.md)。
本交接覆盖当前生命周期开发进度，旧交接中的发布、窗口修复与小艺背景仅作历史参考。完整历史提交依次为内核、单条入口、批量管理、紧凑布局及本轮搜索详情，见 git log -6。

## Codebase Understanding

## Architecture Overview

Svelte 组件通过 store/API 调用 Rust 命令；SQLite 查询在计数和分页前过滤归档，恢复使用现有事务。独立 ArchiveNoteReader 只编排读取，不写任务或便签。

## Critical Files

| File | Purpose |
|---|---|
| src/lib/components/management-dialog.css | 归档/回收站行底色、边框、悬停和选中 |
| src/lib/components/ArchiveDialog.svelte | initialItem 搜索进入、确认、关联预览与返回 |
| src/lib/components/ContentSearchDialog.svelte | 过滤开关和返回后的刷新/滚动恢复 |
| src/lib/components/TodoPanel.svelte | 搜索与归档导航及删除确认文案 |
| src/lib/stores/contentSearchStore.ts | 查询代次、分页及末页回退 |
| src/lib/stores/archiveNoteReader.ts | 有效关联过滤、重新校验和文字读取 |
| src-tauri/src/content_search.rs | 数据库归档过滤 |
| src-tauri/src/content_search_commands.rs | 可选 include_archived 参数兼容旧调用 |
| scripts/test-content-search-ui.mjs | 生产父级搜索/归档导航回归 |
| scripts/test-archive-ui.mjs | 列表布局、批量恢复及关联预览回归 |

### Key Patterns Discovered

保持原有不可变 ArchiveRequest 重试、提交后刷新失败仅重试刷新。搜索组件保持挂载，归档详情用 initialItem 标明返回来源；不会为了恢复而修改全局筛选偏好。


## Current State Summary

LC1b-2b 双端代码和自动化已完成并本地提交：搜索归档过滤与恢复、列表轻底色/细边框/间距及选中层次、删除已完成文案、分组和原日期已过提示、关联便签只读文字预览。此前 LC1a 归档内核、LC1b-1 单条管理、LC1b-2a 可恢复批量管理均已提交。用户本次仅要求生成 handoff 并提交，不启动新阶段。原生桌面、鸿蒙手机/平板视觉与真实隔离同步仍属于 LC1c，不能用构建或 host 测试替代验收。

## Important Context

- 两端都是 main，无需再次合并；本轮未推送、未发布、未升版，未改 schema、同步协议、Object Key、备份格式或小艺意图。
- 桌面版本 1.2.0、schema 21；鸿蒙版本 1.3.0、versionCode 1000030、RDB schema 22；备份 v5。
- 包含归档默认开启，只影响任务，在数据库计数及分页之前过滤；当前会话保留开关，不增加持久偏好。
- 搜索命中归档任务使用同一 ArchiveStore 和归档确认流程。取消归档保留完成状态，重新打开设为单次未完成；不重启旧提醒或重复规则。
- 返回搜索刷新当前页，保留查询、分类、分页、滚动；末页变空时退回最后有效页。异步查询通过 generation/request 隔离。
- 关联便签首版只读文字预览，不加载附件、不开放编辑；打开前重查源归档任务、有效关联和目标便签。不能把这个入口描述为完整便签编辑器。
- “删除已完成”仍是软删除，明确可到回收站恢复；彻底删除/清空仍未开放。
- 不提交用户数据库、附件、签名、凭据、截图或构建产物。不强制关闭测试应用。
- 桌面 Cargo.toml 原有换行状态差异已确认没有文本 diff，未纳入本轮提交；下一会话若仍显示 dirty，不要回滚或顺手 stage。

## Immediate Next Steps

1. 读取本 handoff 与 docs/TASK_LIFECYCLE_ROADMAP.md，核对双方 git status 和最新提交；用户未要求继续时不要自行启动新开发。
2. 按 docs/TASK_ARCHIVE_SEARCH_UI.md 的 LC1c 清单验收原生桌面、手机、平板：列表层次、亮暗主题、大字体/窄窗、搜索归档开关、恢复确认和返回位置、有效/失效关联。
3. 使用授权的隔离同步环境验证一端归档、另一端取消归档/重新打开；不重启旧提醒、不推进旧重复实例。不要把历史“同步测试通过”泛化到新归档流程。
4. 用户确认具体验收范围后再更新 roadmap。下一开发阶段是 LC2a 安全清空的旧端兼容原型，不是直接增加清空按钮，更不能跳到今日计划或等待处理。

## Work Completed

- [x] LC1a：事务内核、版本快照、不可变重试、权威时间/设备身份、共享契约和跨端 JSON 自动化。
- [x] LC1b-1：已归档与回收站相邻入口、列表/搜索/分页/详情、单条取消归档/重新打开/删除及确认。
- [x] LC1b-2a：固定目标批量处理、成功/跳过明细、中断与最终回复丢失后恢复、结束剩余处理保护。
- [x] 两轮列表优化：左侧 checkbox、固定搜索工具栏、批量按钮布局；之后轻底色、细边框、6px/vp 间距与淡黄色选中。
- [x] LC1b-2b：统一搜索恢复及上下文保留、归档过滤、详情分组/日期/关联、删除文案和测试。
- [x] README、实现方案、roadmap、阶段验证文档同步；双端共享文档内容一致。
- [x] 本次提交前再次运行桌面 415 项 Vitest、鸿蒙归档详情与搜索导航 host 检查，全部通过；git diff --cached --check 通过。

## Decisions Made

| 决策 | 原因 |
|---|---|
| 轻量行底色和细边框，不恢复厚重大卡片 | 满足用户区分每条任务的要求，同时保留紧凑密度 |
| 复用归档服务和确认组件 | 搜索入口不能形成另一套恢复、重复或提醒逻辑 |
| 默认搜索包含归档，过滤早于计数分页 | 保持原行为，并避免总数与列表不一致 |
| 关联仅预览当前文字 | 保持归档只读与清晰返回路径，避免意外编辑或附件下载 |
| LC1c 验收与 LC2a 兼容门槛独立保留 | 自动化不能证明原生体验或旧端不会让已清空数据复活 |

## Validation

上一实现回合执行并通过（本次只做上述快速复核，未声称重跑全部）：

- 桌面 pnpm test：50 文件、415 项；pnpm check：0 错误/0 警告；pnpm build 成功；i18n:check：894 对齐键。
- cargo test --lib：344 通过、16 条件忽略；cargo fmt -- --check 通过。本轮新增归档过滤总数和分页测试，未以忽略项充作通过。
- 桌面归档 24 组语言/主题/窗口/缩放、8 组批量恢复及丢失最终回复；新增关联预览/返回/失效零写入检查通过。
- 桌面统一搜索 24 组布局与实际父级导航、包含归档开关、搜索内取消归档及刷新返回通过；回收站 24 组及取消/冲突/忙状态/刷新重试/分页通过。
- 鸿蒙 test-content-search.cjs：23 共享契约、过滤计数与分页、120 条长文本分页；搜索 UI host、归档单条/批量/详情、回收站 store host 均通过。
- 鸿蒙 devecocli Debug 构建通过，原有 SDK 弃用、可抛异常等警告保留。桌面生产构建有大 chunk 提示。
- UI 自动化使用生产组件及隔离 IPC；host 使用隔离端口或 SQLite，不是原生设备 RDB/S3 验收。本轮未安装覆盖设备应用，未修改用户任务或存储。

## Pending Work

- [ ] LC1c：实际桌面、手机、平板体验和真实隔离同步，记录具体设备及结果。
- [ ] LC2a：终态删除凭据、旧端隔离和受控迁移原型；失败则不能开放清空。
- [ ] LC2b-d：本地终态/清理队列、同步与备份、清空入口及验收，需按 roadmap 顺序推进。
- [ ] LC3 今日计划、LC4 等待处理、LC5 收尾，均未在本轮实现。
- 没有当前代码或构建阻塞；未完成项是验收/后续阶段，不应擅自标记全部完成。

## Environment State

- Windows / PowerShell；两个独立仓库 D:/Develop/EggDone 与 D:/Develop/EggDoneHarmony；鸿蒙嵌套工程位于 EggDone 子目录。
- 本轮开启的测试进程、隔离浏览器和 Vite 测试服务均已结束；不保证用户原有应用是否仍在运行，操作前检查。
- 脚本环境变量名称：PYTHONUTF8、PLAYWRIGHT_PATH、TYPESCRIPT_PATH。不要记录真实同步凭据。
- Windows 中文 handoff 工具建议 PYTHONUTF8=1；先校验，后单独提交文档，不推送。

## Potential Gotchas

- 不要同时运行 pnpm check/test/build 与 Vite UI 自动化：生成的 tsconfig 会触发 full reload，使测试在中途失去界面。各 UI 脚本顺序执行。
- Playwright 本机依赖在 C:/Users/caozhipeng/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright，浏览器脚本使用 msedge。换机器先检查路径。
- 鸿蒙 host 面板测试从 onLoad 属性开始提取到首个 @Builder；新增被测试状态若放在切片外，会变成 undefined，不能将其误当原生运行问题。
- 同步永久清空不能只删 tombstone 或隐藏 UI；旧端晚到更新、旧备份和离线恢复的兼容门槛见共享契约。
- README 下方包含旧阶段历史“未提交/待验收”文本，不要用全文件替换改写历史或“未提交草稿”的业务含义。

## Related Resources

- docs/TASK_LIFECYCLE_ROADMAP.md：当前阶段与后续门槛。
- docs/TASK_LIFECYCLE_IMPLEMENTATION_PLAN.md：方案及状态语义。
- docs/TASK_LIFECYCLE_CONTRACT.md：共享契约与兼容原型要求。
- docs/TASK_ARCHIVE_STORAGE.md：内核边界和跨端验证。
- docs/TASK_ARCHIVE_BATCH_UI.md：可恢复批量处理。
- docs/TASK_ARCHIVE_SEARCH_UI.md：本轮实现、测试与 LC1c 操作清单。
- docs/TASK_MANAGEMENT_LIST_UI.md：第一轮列表优化，最新层次调整以本轮文档为准。


## Reproduction Commands

```powershell
pnpm test
pnpm check
pnpm build
pnpm i18n:check
cargo test --lib --manifest-path src-tauri/Cargo.toml
```

UI 脚本在设置 PLAYWRIGHT_PATH 后分别运行 scripts/test-content-search-ui.mjs、scripts/test-archive-ui.mjs、scripts/test-trash-ui.mjs；不要与上述生成配置的命令同时执行。

## Files Modified

本轮完整清单可通过 git show --stat 468159b 获取。Critical Files 表列出主要业务入口；同一提交另含对应单元/界面回归、三份共享生命周期文档、列表说明及 README。未修改版本元数据或存储迁移。此文档为后续单独 docs 提交，不混入实现提交。

## Assumptions Made

当前授权是将上一轮实现和 handoff 本地提交，未授权新阶段开发或推送发布。用户尚未确认本轮 LC1c 原生/真实同步验收；只有已记录的自动化结果可标记完成。旧阶段的验收反馈不能推定覆盖本轮新增入口。

## Commit Boundary

本轮实现提交为 468159b，包含业务代码、测试、README 和阶段文档；handoff 另作 docs 提交。当前授权不包含推送、发布、版本提升、下一阶段开发或真实存储配置变更。最终工作区应无本轮剩余改动；桌面既有 Cargo.toml 换行状态差异除外。

## Screenshot Evidence

截图位于当前 Windows 用户临时目录，未入库：eggdone-archive-ui-1789638134208、eggdone-content-search-ui-1789638064482、eggdone-trash-ui-1789638244331。这是隔离浏览器数据，不是真机屏幕。跨会话临时目录可能被清理，可靠复现以已提交脚本为准。
