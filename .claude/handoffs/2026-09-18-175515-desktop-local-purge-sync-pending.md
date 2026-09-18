# Handoff: 桌面本机清空已提交，正式同步清空待接入

## Session Metadata

- Created: 2026-09-18 17:55:15 +08:00
- Project: D:/Develop/EggDone
- Branch: main
- 功能基线：ae3bd99，前序 ca30607、febaa21。文档提交前比 origin/main 领先 4 个本地提交；未推送。
- 对端功能基线：D:/Develop/EggDoneHarmony，main / f9df63c。
- 版本：package.json / Tauri 1.2.0；SQLite schema 22。
- 最新请求：生成 handoff 并提交，仅文档收尾，不授权开始后续开发。

## Handoff Chain

- Continues from: [桌面归档搜索交接](2026-09-17-180416-desktop-archive-search-lc1b2b.md)。
- 本文取代该交接中后续阶段状态；归档历史背景仍可参考旧文档。
- 对端同期交接：D:/Develop/EggDoneHarmony/.claude/handoffs/2026-09-18-175513-harmony-local-purge-sync-pending.md。

## Current State Summary

双端本机清空内核与界面已提交，桌面功能提交 ae3bd99。当前只有从未配置同步的设备可实际清空；正式空间迁移、终态云同步及远端附件删除尚未实现，因此不能称完整功能已交付。用户已知此限制，要求先提交，再保存上下文。handoff 前工作树仅 src-tauri/Cargo.toml 存在原有换行符差异，无语义 diff，不能擅自还原或混入文档提交。

## Critical Files

| File | 用途 |
|------|------|
| src-tauri/src/purge.rs | 固定范围、指纹、50 项原子批次、终态、回执和本机文件清理 |
| src-tauri/src/purge_commands.rs | 三个 Tauri command、共享 SyncRuntime 锁、后台 worker |
| src-tauri/src/migrations/022_purge.sql | 新表、同步版本与终态防复活触发器 |
| src-tauri/src/data_exchange.rs | 条件 v6 备份、终态校验、导入事务、跨端测试桥 |
| src-tauri/src/s3_sync.rs | 阻止终态数据走旧协议；生产对象路径派生 |
| src-tauri/src/migration_preflight.rs | 本机状态盘点、终态/清理队列门槛 |
| src-tauri/src/purge_tests.rs | 冲突、回滚、105 项重启续作、回执与本机文件测试 |
| src/lib/api/purgeApi.ts | UI 到本机命令 |
| src/lib/components/TrashDialog.svelte | 清空/勾选/详情/确认/进度/重试 |
| scripts/test-trash-ui.mjs | 真实组件与隔离 IPC 的视觉及交互回归 |
| scripts/run-sync-core-s3.ps1 | 隔离临时 S3 回归入口 |

## Files Modified

功能提交 ae3bd99 包含 24 个文件，2169 additions / 26 deletions，详见 git show --stat ae3bd99。
包含上述内核与 UI、中文/英文文案、数据迁移、备份、预检及文档。不含无关 src-tauri/Cargo.toml。
本次只新增本交接文档，后续应分别核对另一仓的 handoff 提交，不自动推送。

## Key Patterns Discovered

Rust/SQLite 是桌面业务写入入口，Svelte 不能直接操作数据库。清空命令持有 SyncRuntime 排他锁；耗时准备与执行经 spawn_blocking，不阻塞 UI 主线程。正文/终态/队列和计划进度同一事务，文件 IO 失败单独保留队列。当前纯本机允许条件不能当正式新空间标识。

## Important Context

- 用户要的是后端、同步和界面完整交付，不接受把“本机按钮已出现”当成 LC2 完成；后续获得继续开发指令后，按完整链路连续推进，不需要每个内部子步骤重新询问。
- 最新请求仅生成 handoff 并提交。本轮不能据此开始迁移开发、升级版本、推送、合并其他分支或发布。
- 已接受的方案：参与设备升级，迁移前汇合离线修改，显式迁移并保留旧空间；旧版之后写入旧空间的内容不会自动进入新空间。接受方案不等于授权本次迁移/清空真实数据。
- 旧客户端会丢弃不认识的永久删除字段，因此不能只在旧 JSON 加字段或关闭校验。当前曾配置同步、有目标 epoch 或有同步历史的本机清空受阻；本机已有终态时，旧协议同步受阻。
- 未提交到仓库的用户数据库、签名、同步配置和云桶均不可拿来做破坏性试验。继续使用隔离 SQLite / host RDB 适配器和临时 S3 fixture。
- LC3 今日计划、LC4 等待处理仍排在完整安全清空之后。小艺意图和实况窗不在此次范围。
- 不保证清除 SQLite 空闲页、系统备份、用户导出文件及云桶历史版本；本轮永久删除是应用记录与所属文件的逻辑移除。

## Architecture Overview

准备 -> 固定 UUID 和内容指纹 -> 勾选不可撤销确认 -> 每批 50 项短事务 -> 写终态及清理队列 -> 提交 -> 按 UUID 清理本机附件 -> 展示结果或同 ID 重试。
计划只存 ID、摘要、计数和状态，不保存正文。已恢复、已变化或活动重复规则当前实例跳过，不拓宽本次范围。
任务所属清单、关联记录及便签历史随父项清理；独立任务/便签、归档及独立模板保留。
触发器阻止终态父项及所属记录重建。相关批量创建仍有待恢复请求时先拒绝清空，避免丢弃其他创建草稿。
已完成批次保留；失败批次整体回滚。附件失败单独排队，失败项轮换到后面，刷新失败不能再次执行删除。

新表：lifecycle_terminals、lifecycle_sync_state、purge_plans、purge_targets、purge_cleanup。
终态备份采用 v6，无终态仍导出 v5。v6 不能静默清除目标库已有父项；旧备份撞到终态时当前实现整次拒绝并回滚，不是逐项排除预览。
remote_done 只是将来远端清理字段，没有生产远端删除执行器。生产终态云同步尚未实现。

## Decisions Made

| 决策 | 原因与边界 |
|------|------------|
| 旧同步空间保持执行阻断 | 防离线旧副本把正文重新写回来，不用关闭同步作为绕过方式 |
| 本机清空与 UI 先接入真实源码 | 提供可测试链路，但仍不宣称已配置同步的设备可用 |
| 事务进度与终态同批提交 | 崩溃、重试和回复丢失不重复删除、不扩大目标 |
| 正式迁移采用独立空间且保留旧空间 | 用户已接受成本；原生迁移和加入入口仍缺失 |
| 备份按有无终态选择 v5/v6 | 普通备份兼容不无故变更，永久删除身份不可被降版遗漏 |
| 每端一个功能提交，handoff 另提交 | 代码与文档状态明确，不混入版本发布或私有配置 |

## Work Completed

- [x] 双端真实本机清空内核、schema migration、终态保护、单项/全部/批量入口、确认、暂停/重试、文件清理。
- [x] 备份 v6 终态导入导出及 Harmony -> Rust -> Harmony 生产解析器往返。
- [x] 本机迁移预检查纳入终态与清理队列，40 组共享用例。它仍不提供完整备份证明或迁移执行。
- [x] 代码、测试、README 与路线文档已分别纳入桌面 ae3bd99、鸿蒙 f9df63c。
- [x] handoff 前一次提交已检查暂存 diff、排除无关换行改动；无推送，无真实数据操作。

## Verification

以下是同一会话前序已完成的自动化证据；本次 handoff 是文档操作，不重复跑整套构建。
- 桌面全量 Rust：360 passed、23 ignored；check 0 errors / 0 warnings；build 通过，保留 bundle size 提示。
- 桌面 Svelte + 隔离 IPC mock：24 个中英文/亮暗/320、480、1000 宽度/1、1.5 缩放组合，另有 4 个清空确认、旧空间拦截与原 ID 重试场景通过。
- 鸿蒙生产 Repository + host SQLite：固定目标、冲突、原子回滚、105 项跨重启续作、回执正文清理、文件失败重试通过。
- 鸿蒙生产面板方法：确认、取消、分批、暂停、丢回复重试、仅刷新重试、旧空间阻断通过，不包含 ArkUI 渲染。
- 双端终态 v6 备份往返通过；旧备份不能恢复终态项目。
- 本机迁移预检 40 组共享 fixtures 通过；旧完整同步服务隔离 S3 四阶段回归通过，不代表新终态同步可用。
- 鸿蒙 devecocli Debug 构建通过；没有给用户真机安装此开发版本、执行迁移或清空。
- 最近提交前重跑：桌面 cargo test --lib purge --quiet 为 14 passed、3 ignored；鸿蒙本机清空及面板脚本通过；暂存 diff check 通过。
- 未验证：鸿蒙原生 RDB 执行、手机/平板/分屏/大字体视觉、真实双端原生 IPC 到云端的新清空链路。构建通过不能替代这些验收。

## Immediate Next Steps

1. 先核对两仓 Git 状态、功能提交和 schema，再读 docs/TASK_PURGE_NATIVE_PROGRESS.md 与 docs/TASK_LIFECYCLE_ROADMAP.md。不要仅依据旧 handoff 中“尚无入口”的状态。
2. 正式继续开发时，从“完整本机/附件备份证明 + 持久迁移计划绑定”接起。涵盖终态、已删除附件、本地历史、清理队列，最终切换前再次校验源状态。
3. 把迁移原型接入原生服务和 UI：旧空间复制、发布清单、源 ETag 复核、CAS/读回、崩溃恢复、原子切换和新设备加入。保留旧空间，不把测试固定路径用于生产。
4. 接通新空间强制终态账本及所有领域过滤，覆盖离线恢复/清空并发、缺失或损坏账本、旧备份。先确认终态发布，再清理新空间正文和所属云端附件；条件删除与重试不得影响旧空间或独立内容。
5. 做完整端到端隔离测试和手机/平板验收，再解除旧空间限制并标记 LC2 完成；仍不进入今日计划/等待处理。不要为了让按钮可点直接移除安全判断。

## Pending Work

- [ ] 原生备份校验、迁移计划绑定、复制/发布/切换/恢复及正式 UI。
- [ ] 新设备加入新空间、终态账本生产同步、跨领域过滤与并发处理。
- [ ] 云端正文/附件清理、部分失败重试和准确同步状态。
- [ ] 备份导入终态冲突的用户可理解预览；当前是整次拒绝。
- [ ] 原生设备、全链路与故障矩阵验收。当前无外部平台阻断，是尚未实现的工作。
- [ ] 当前永久删除导入到有旧同步配置的库会使旧同步被阻断；必须随正式新空间迁移处理此用户流程。
- 今日计划和等待处理因顺序要求延后，不是本次新需求。

## Assumptions Made

- 保持个人工具、无自建服务端、现有 S3/MinIO 同步模型。新终态协议尚未完成，不假定原型已经正式启用。
- 本机 schema 升级不可由应用版本号推断；本次没有提升桌面 1.2.0 / 鸿蒙 1.3.0。
- 存储配置和原生测试目标需恢复工作时重新核对，不依赖旧临时 fixture 或旧设备连接状态。

## Potential Gotchas

- 迁移原型仅在 host 脚本运行，固定合成 namespace；实际附件 Object Key 由生产派生函数生成，与原型示例可能不同。
- 本机预检的“已静止”只表示本机 dirty/ACK 等门槛，不证明完整备份、远端汇合或所有设备已升级。
- 备份 v6 只在有终态时输出；旧客户端无法理解终态，不得把 v6 当普通旧协议文档上传。
- 备份导入与清空必须保留事务、锁和错误传播，不忽略终态冲突继续合并。
- 不要同时运行 pnpm check/build 与 Playwright 界面脚本：SvelteKit 配置重写或 build 热刷新曾中断用例；分开执行后通过。
- 活动重复规则当前实例会被跳过；不是“点清空后必须无条件删完所有行”。
- 全量 Rust 中的 ignored 用例依赖外部 fixture，未显式执行不能算通过。
- 勿杀用户运行的应用或 Rust 工具进程来解除文件锁；必要时请用户退出。
- 不记录真实云存储凭据、签名或用户正文，handoff 校验只能证明文档完整性，不能证明功能完成。

## Related Resources

- docs/TASK_PURGE_NATIVE_PROGRESS.md：当前实现、证据和剩余工作，优先于 README 的历史条目。
- docs/TASK_LIFECYCLE_IMPLEMENTATION_PLAN.md 与 docs/TASK_LIFECYCLE_ROADMAP.md：双端范围与顺序。
- docs/TASK_MIGRATION_LOCAL_PREFLIGHT.md：本机只读门槛。
- docs/TASK_MIGRATION_JOURNAL_PROTOTYPE.md 与 docs/TASK_MIGRATION_S3_VALIDATION.md：原型、生产解析器桥接及隔离 S3 边界。

## Environment State

- Windows / PowerShell。只允许以显式当前 Git 状态为准；src-tauri/Cargo.toml 的换行差异留给用户。
- 前序必要构建和测试进程均已完成；本 handoff 没有启动服务。用户/IDE 进程未做关闭操作，也不声明机器无其他进程。
- 运行环境变量名：PLAYWRIGHT_PATH、EGGDONE_DESKTOP_ROOT、EGGDONE_PURGE_BACKUP_INPUT、EGGDONE_PURGE_BACKUP_OUTPUT、PYTHONUTF8。不保存敏感值。
- Playwright 依赖路径使用 load_workspace_dependencies 重新确认；历史截图位于系统临时目录，不是仓库产物，也不是发布凭证。
- 以下命令分别运行，切勿将构建与 UI 回归同时运行：

```powershell
# D:/Develop/EggDone/src-tauri
cargo test --lib --quiet
cargo test --lib purge --quiet
cargo test --lib migration_preflight --quiet
cargo fmt --all -- --check

# D:/Develop/EggDone
pnpm check
pnpm build
node scripts/test-trash-ui.mjs
./scripts/run-sync-core-s3.ps1 -CrossClientSessions -HarmonyRoot D:/Develop/EggDoneHarmony

# D:/Develop/EggDoneHarmony
node scripts/test-purge.cjs --cross-client
```

## Validation And Commit

当前 handoff 的代码基线和未完成边界已从 Git、版本文件及实现进度文档核对；代码未因本次交接重写。使用 session-handoff 校验脚本检查完整性、文件引用和敏感信息后，仅提交本文件。不得把文档校验分数当成正式迁移或设备验收结果。
