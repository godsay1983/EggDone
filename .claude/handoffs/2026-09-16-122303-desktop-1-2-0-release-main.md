# Handoff: 桌面 1.2.0 发布准备与主线交接

## Session Metadata

- Created: 2026-09-16，Asia/Shanghai。
- Project: D:/Develop/EggDone。
- 文档生成时分支：codex/task-productivity；版本提交 3bf88fb。
- 合并状态：尚未执行，已验证 main（5117432）是当前分支祖先，可快进合并。
- 用户明确授权：生成 handoff、提交、双端合并本地 main；未授权推送、打正式安装包或发布。
- 两端版本：桌面 1.2.0 / SQLite 21；鸿蒙 1.3.0 / 1000030 / buildVersion 1 / RDB 22。

## Handoff Chain

- Continues from: [上一阶段批量恢复交接](./2026-09-15-175701-desktop-batch-recovery-p3c.md)。
- 配套交接：[另一端本轮交接](D:/Develop/EggDoneHarmony/.claude/handoffs/2026-09-16-122304-harmony-1-3-0-release-main.md)。
- 本文覆盖旧 handoff 的当前状态；历史验证与设计决策继续保留，不删除历史文档。
- 回归收尾提交：桌面 2038db6；鸿蒙 7554898。
- 升版提交：桌面 3bf88fb；鸿蒙 f18c870。鸿蒙两项样式优化提交 0a5b75b。

## Current State Summary

用户选定的任务内检查清单、任务模板与快捷复制、多行文本批量创建均已实现，用户确认双端基础功能正常、同步正常及鸿蒙两处样式验收通过。本阶段完成自动化收尾、补齐鸿蒙中文资源、更新版本与发布说明，现仅做文档提交及本地 main 合并，不再开始新功能。正式安装包、推送与商店提交尚未执行，专项验收边界详见发布前检查文档，不把构建通过说成全部设备验收通过。

## Important Context

- 不要把清单、模板、批量入口重新规划成未开发。README 顶部、当前候选版本与本文优先于历史过程段落。
- 用户已通过的基础操作不重复要求；只补尚未明确覆盖的设备专项。
- 清单详情即时勾选，编辑模式统一草稿保存，取消不落库；子项全部完成不会自动完成主任务。
- 重复任务“仅本次/本次及以后”分别修改当前实例/系列定义；新实例子项重置完成状态，不复活明确删除项。
- 复制/模板使用先生成草稿再确认，清空旧日期、提醒、重复和完成状态，不携带旧关联/附件。模板与原任务、已生成任务彼此独立。
- 批量最多 50 个非空行、原文 20000 UTF-16 单元、标题 100；重复标题仅提示、不自动去重，不解析时间或自动完成 Markdown 勾选行。
- 只持久化已确认提交的完整批次请求；未提交原文/预览不跨进程保存。原样重试保留 operation UUID 和所有任务 UUID，不重复创建。
- 任务创建成功、清理恢复记录、刷新列表分别处理；已确认但清理失败时重试只清理，不能重复创建。
- 结束恢复只清除匹配的本机恢复记录，不删除任务或撤销已提交结果，保留二次确认。记录不参与同步/备份，不注入损坏到用户库。
- 备份格式 v5；旧版不能导入 v5，旧二进制不能直接降级打开迁移后的本地库。发布建议双端同时升级。
- 本轮不重新调查小艺，不增加专注统计、自动备份等第四项功能。合并不等于发布，不推送。

## Architecture Overview

桌面使用 Svelte/TypeScript → API/store → Tauri command → Rust/SQLite，UI 不直连数据库。清单、模板和批量创建均复用生产事务与同步机制。版本显示来自包元数据，不手工改界面字符串。
清单/系列定义和模板通过独立同步对象及 revision/ACK/配置世代保护；网络不持有本地写事务。本轮只修复资源/测试与版本元数据，没有改变业务协议、数据库 schema 或依赖。

## Critical Files

| File | Purpose |
| --- | --- |
| package.json / src-tauri/Cargo.toml / src-tauri/Cargo.lock / src-tauri/tauri.conf.json | 四处版本均为 1.2.0 |
| src/lib/components/TaskChecklistDialog.svelte | 清单统一编辑草稿 |
| src/lib/components/TaskChecklistDetails.svelte | 已保存清单即时勾选 |
| src/lib/components/TaskTemplateDialog.svelte | 模板管理与使用入口 |
| src/lib/components/TaskBatchDialog.svelte | 批量输入、预览与恢复 |
| src/lib/utils/batchCreationSession.ts | 稳定请求与持久化恢复会话 |
| src-tauri/src/task_batch.rs | 事务、回执及本机恢复记录 |
| scripts/test-task-checklist-regression.ps1 | 已扩展至三项功能的双端回归入口 |
| docs/TASK_PRODUCTIVITY_ROADMAP.md | 代码、基础验收和完整发布门槛分别记录 |
| docs/TASK_PRODUCTIVITY_RELEASE_REVIEW.md | 最完整的回归证据、升级限制和剩余门槛 |
| CHANGELOG.md | 本次候选版本的三项功能说明 |
| README.md | 当前版本及最新交接入口 |

## Files Modified

本次 handoff 操作只生成本文、更新 README 最新交接入口及合并状态记录。之前收尾改动已提交，不将构建产物、临时日志、测试数据库、私钥或用户数据纳入 Git。
- 桌面收尾 2038db6：扩展共享回归入口，更新 changelog、README、roadmap、发布前检查。
- 鸿蒙收尾 7554898：额外补齐 41 条 zh_CN batch 文案，修复两处旧测试。
- 升版 3bf88fb/f18c870：版本元数据及配套说明；精确文件清单可用 git show --stat 查看。

## Work Completed

- [x] 清单、模板/复制、批量创建功能与双端基础操作验收。
- [x] 鸿蒙批量成功面板自适应高度、模板按钮 30vp 高度统一，用户验收通过。
- [x] 发布前回归、双端更新日志与版本元数据维护。
- [x] 迁移测试夹具及旧样式断言修正，缺失中文资源补齐。
- [ ] 文档校验与提交后执行本地 main 快进合并，完成后更新本项。
- [ ] 正式包、完整原生专项、旧端混用、推送及上架未执行。

## Validation

本轮交接引用本会话前面已完成的证据，不声称生成文档时重新跑完全量：
- 桌面 Vitest 45 文件 / 397 项通过，Rust 334 通过 / 15 默认忽略。清单/模板备份和 HTTP 的 4 个忽略项被跨端脚本另行显式执行；其他 11 个不计通过。
- 汇总脚本 44 步通过：协议、旧库迁移与失败回滚、重复继承、删除恢复、同步、备份交换、模板、批量/恢复及桌面检查。
- 浏览器清单 88、模板 24、批量 24 组，共 136 组通过；隔离 IPC 替身，不是原生 Tauri 窗口/设备测试。
- 鸿蒙国际化随后单独通过：957 条英文伪本地化、三套资源键/占位符及 AppGallery 名称检查。最终回归脚本已加入该门槛，下次同配置为 45 步，未将原 44 步日志改写。
- 三处发现：迁移夹具错误保留 v22 标记、旧按钮测试只认两行、zh_CN 缺 41 条 batch 文案。均已修复并复测；前两处只改测试，生产迁移不变。
- 升版后：桌面 Svelte 0 errors / 0 warnings，pnpm build、cargo fmt/check --locked、cargo metadata --locked 通过；18 组共享草稿/文档一致性复查通过。
- 升版后鸿蒙 Debug 构建通过（16.533s，33 tasks），新 HAP 的 pack.info 为 1.3.0 / 1000030 / build 1。
- 已有警告保留：桌面 TraySnapshot.locale 未使用，鸿蒙弃用 API/异常处理等，不在本轮扩展重构。
- 真实手机/平板完整软键盘、大字/分屏矩阵、原生退出恢复、系统提醒、含真实附件的完整备份、真实旧端混用未逐项证明。用户已有同步反馈保留，但本轮代理未访问用户 S3 或用户库。

## Decisions Made

| Decision | Rationale |
| --- | --- |
| 已验收基础功能不再重复测试 | 避免每阶段要求重复人工操作，专项未测项单独记录 |
| 旧测试先修复夹具/断言 | 不能为过时测试改坏已验收的生产行为 |
| 回归与升版分开提交 | 便于定位产品变更与元数据维护 |
| 选择快进合并，不重写历史 | 两端 main 都是开发分支祖先且工作区干净 |
| 保留开发分支，不推送 | 用户只授权本地提交与合并 |

## Immediate Next Steps

1. 新会话先核对两个仓库的分支、git status 和最新提交；读本文及发布前检查，确认 main 已包含版本提交，不按旧 handoff 重新升版。
2. 本次用户授权到 handoff、本地提交与 main 合并为止；完成后停下，不自动启动打包或发布。若用户要求继续，先确定 Windows 安装器、Harmony 正式 App 包或 Linux 包的具体目标。
3. 打正式桌面包前确认正在运行的客户端已退出或获得用户同意，不杀用户进程。鸿蒙正式包需核对签名与版本，Debug signed HAP 不是商店签名证明。
4. 保留未测边界，按发布需要补专项，不覆盖用户数据做故障实验。推送、标签、上传商店需另行授权。

## Assumptions Made

- 用户“验证通过”覆盖其实际测试过的双端功能及本轮鸿蒙样式，不扩展到未逐项说明的设备矩阵。
- 本次合并以已检查的本地 main 为基线，没有执行 fetch；远端之后是否更新未知，推送前须重新核对。
- 升版不会自动更新正在运行的旧进程或设备安装包，应以实际包/运行版本为准。

## Potential Gotchas

- 旧库迁移失败测试模拟 v20 时，必须同时移除模板表及 v21/v22 标记，否则 MAX(version)=22 会跳过迁移。
- 桌面原生客户端可能被用户的 tauri dev 自动重建；不要把它算成本轮正式安装包构建，也不要擅自结束进程。
- 浏览器与 pnpm check/build 顺序运行，防止生成配置触发 HMR 误失败。
- 回归脚本仍叫 test-task-checklist-regression.ps1，但已经覆盖三项功能；两端脚本和共享方案保持相同。
- -SkipBuild 跳过原生构建；桌面前端构建仍执行。不能把 skipBuild=true 汇总当完整安装包构建证明。
- 不手动编辑已完成历史 handoff 来伪造过去状态；当前状态用本文和 README 顶部覆盖。

## Environment State

Windows PowerShell。桌面 D:/Develop/EggDone；鸿蒙 D:/Develop/EggDoneHarmony，构建在其 EggDone 子目录。生成/校验脚本运行前设置 PYTHONUTF8 为 1，避免中文解码问题。
- 本机 devecocli 的 NVM shim 有空格路径问题；使用现有 Node 显式调用 CLI：
  - C:/Users/caozhipeng/AppData/Local/Author Software/nvm/installs/v24.21.0/node.exe
  - C:/Users/caozhipeng/AppData/Local/Author Software/nvm/installs/v24.21.0/node_modules/@deveco/deveco-cli/dist/cli.js
- PLAYWRIGHT_PATH 可指向 C:/Users/caozhipeng/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright；TYPESCRIPT_PATH 可覆盖宿主默认 DevEco TypeScript。
- 本次读取到桌面 eggdone.exe 仍运行，PID 34040；PID 会变化，使用前重查，不操作用户进程。
- 没有本任务遗留的构建、日志跟踪或浏览器测试进程。设备由用户管理，不因交接而停止模拟器。
- 不记录同步凭据、签名口令等敏感配置。

## Artifacts And Evidence

- 完整回归日志：C:/Users/caozhipeng/AppData/Local/Temp/eggdone-checklist-regression-c626883acfb0495697cfbd4f62a54d7c/results.json，44 步 passed=true，skipUi=false、skipBuild=true。
- 初次迁移测试失败记录和修复过程在发布前检查中保留，不改写失败日志。
- 鸿蒙 Debug HAP：D:/Develop/EggDoneHarmony/EggDone/entry/build/default/outputs/default/entry-default-signed.hap。
- 该包 SHA256：1306AEF06E1DE5DD93914764EDE4A08E18B19E83BBBF760B8FC5240CC2324FBD。本次未安装或上传。
- 桌面前端构建目录为 D:/Develop/EggDone/build；本轮未生成正式 Windows/Linux 安装包，旧安装包不能作为新版本产物。
- 临时日志和截图会被清理，不提交到仓库；复跑脚本是长期依据。

## Related Resources

- docs/TASK_PRODUCTIVITY_RELEASE_REVIEW.md
- docs/TASK_PRODUCTIVITY_ROADMAP.md
- docs/TASK_PRODUCTIVITY_CONTRACT.md
- docs/TASK_BATCH_RECOVERY.md
- docs/TASK_TEMPLATE_UI.md
