# Handoff: EggDone 桌面端 NS7 候选版回归

## Session Metadata
- Created: 2026-09-06
- Project: D:/Develop/EggDone
- Branch: main
- Candidate version: 1.0.8
- Continues from: [E3A](2026-09-06-024722-desktop-recurrence-e3a.md)

## Current State Summary

用户要求先提交上一项，再完成剩余开发，每项分别中文提交。E3B 备份、E3C 自定义重复 UI、NS6 后续能力设计现已完成；NS7 自动回归和候选版资料已完成，但真实设备、跨平台、真实 S3 及正式升级验收尚未齐备，不能标记正式发布就绪。本轮只本地提交，没有推送或发布。版本元数据升到 1.0.8，关于页仍读取 package.json，不维护第二份硬编码。

## Important Context

- E3A handoff 中的“UI 未开放、备份未实现”已过时；候选版两项均已实现，优先读当前 roadmap E3B/E3C 与 NS7。
- 稳定版实况窗已通过华为验收；本轮没有改鸿蒙实况窗或签名。
- 桌面 DB v17、鸿蒙 DB v18；独立 recurrence-rules.json v1，备份 data v2。旧 v1 可导入，旧客户端不能导入 v2。
- 普通已保存任务才能附加规则；不宣称新任务与规则一次原子创建。完成只生成下一项，不批量补历史。
- 修改周期生成新规则 UUID、停止旧规则、保留历史并重新计数；失败重试保留原请求。停止/历史项只读，不能用旧快捷重复覆盖自定义系列。
- 规则恢复与同步互斥，ACK/ETag 不跨安装迁移；停止规则不复活、已清理实例不重建。
- NS6 是设计任务。任务便签关联 L1-L5 明确是 NS7 后的候选，不能递归扩成当前必做编码；检查清单、历史同步、加密均未实现。
- 推送、GitHub Release、应用市场上传未获本轮授权。不得使用用户生产桶注入冲突或清除真实数据。

## Work Completed

- 9920ea0：规则 v2 备份与原子恢复。
- ee08271：自定义重复编辑器、摘要、本地化、窄屏布局及旧入口保护。
- 1b7d7ea：下一阶段决策、任务便签关联方案/roadmap。
- d05ee9f：上传超时测试只限制被测 PUT，前置 GET 保留正常超时，断言请求确实发出。
- 本交接与 NS7 记录、README、版本 metadata 随候选版文档提交；最终提交号用 git log 查询，不在文档中自引用。
- 鸿蒙对应完成 UTC 原生兼容修复、安全测试脚本和 13 项原生回归。

## Verification Evidence

- 最终 pnpm release:check 成功：220 Rust、147 Vitest、Svelte 0 errors / 0 warnings、538 i18n keys、生产前端构建。
- scripts/check-recurrence-ui.cjs：17 真实 Svelte 场景通过（mock Tauri IPC）。320/480/920 宽度、亮暗、中英文、保存失败重试、停止、历史和未保存日期保护。
- 已查看 480-en-US-dark.png：无文本重叠、操作遮挡。不能据此声称全部 Tauri/DPI/多屏通过。
- 本轮完整检查首次失败于超时测试前置 GET 被误设 80ms；已修复测试条件并完整重跑，不是仅忽略或重跑失败用例。
- 保留原有 TraySnapshot.locale 未读警告；Vite HMR WebSocket 被 Chrome 本地网络检查阻止，但页面断言完成，未关闭安全检查。
- 日志：TEMP/eggdone-ns7-release-final.log、TEMP/eggdone-ns7-ui.log；截图 TEMP/eggdone-recurrence-ui。没有提交构建产物或凭据。
- 鸿蒙 98 LocalTest、20 宿主脚本、595 英文伪本地化和 assembleApp 通过；后续补充4项原生编辑器测试，手机/大平板模拟器各17 ohosTest通过。Stage应用上下文替代测试夹具过早获取前台页面；生产代码未随该测试修复变化。两台均 HarmonyOS 7，不能视为 HarmonyOS 6 实体设备/真实 S3 验收。
- 最终原生报告目录与大平板首页检查边界见双端同版 NS7_REGRESSION_AND_RELEASE.md；桌面本补充只更新记录，不重做上一提交的产品开发或重复计入验证次数。

## Critical Files

| File | Purpose |
| --- | --- |
| docs/NEXT_STAGE_ROADMAP.md | NS5 开发完成，NS7 人工门槛仍未完成 |
| docs/NS7_REGRESSION_AND_RELEASE.md | 双端相同发布记录、商店说明草稿、R1-R8 集中验收 |
| docs/RECURRENCE_EDITOR_UI_CONTRACT.md | E3C 入口与行为边界 |
| docs/RECURRENCE_BACKUP_CONTRACT.md | v2 恢复、回执与兼容 |
| src/lib/components/RecurrenceEditor.svelte | 自定义重复编辑器 |
| src/lib/utils/recurrenceForm.ts | 与 ArkTS 同语义表单及首项计算 |
| src-tauri/src/recurrence_commands.rs | 规则 UI IPC |
| src-tauri/src/recurrence_transport_tests.rs | 本轮修复的上传超时测试 |
| scripts/check-recurrence-ui.cjs | 隔离浏览器回归 |

## Immediate Next Steps

1. git status 和 git log 核对候选提交；没有需要继续补写的 E3B/E3C 功能代码。
2. 按 NS7 文档 R1-R8 补齐实际平台、独立测试桶及设备验收证据；未知或未运行项保持未勾选。
3. 重点真实双端离线完成、改周期冲突、404 初始化、旧客户端不碰规则对象、完整附件 v2 互导。
4. 发布前完成 Windows 托盘/通知/快捷键/自动更新和高 DPI，以及 Linux/macOS 系统差异。当前只有 Windows 主机，不能推断其他系统通过。
5. 全部门槛通过后才收口 NS7；需要用户明确授权再推送、打正式分发包、发布。关联功能需下一轮明确启动。

## Pending Work

- [ ] Windows 实际窗口/系统动作；Linux/macOS 打包及原生能力。
- [ ] 真实 S3/MinIO 双向、离线并发、旧客户端与 v2 互导。
- [ ] 鸿蒙手机/平板、正式 Profile 邀请测试升级、系统能力。
- [ ] 正式发布资料最终确认和发布操作。草稿不是审核通过证据。

## Decisions Made

使用既有 UI 风格和独立编辑器，不改业务框架。生产网络超时未因测试失败而放宽；仅把测试的短超时限定到被测操作。版本号按既有 patch 增长，不升级依赖或数据库。自动化完成与正式验收分开记录。

## Environment State

Windows PowerShell，D:/Develop/EggDone；Node/pnpm/Rust 可用。Vite 测试端口 1423 已停止，Chrome 测试实例已关闭，无本轮残留构建/测试会话。浏览器脚本需要 PLAYWRIGHT_PATH 指向已安装 Playwright，默认连接本地1423；不读取真实用户数据库或网络凭据。

## Related Resources

- [候选版记录](../../docs/NS7_REGRESSION_AND_RELEASE.md)
- [后续能力决策](../../docs/NEXT_CAPABILITIES_DECISIONS.md)
- [关联 roadmap，尚未启动](../../docs/TASK_NOTE_LINK_ROADMAP.md)
