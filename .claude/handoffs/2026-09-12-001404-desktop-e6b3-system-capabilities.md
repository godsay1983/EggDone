# Handoff: 桌面端 E6b3 系统能力状态

## Session Metadata
- 日期：2026-09-12，Asia/Shanghai。
- 工程：D:/Develop/EggDone；分支：codex/experience-roadmap。
- 版本：1.0.10，未提升版本。
- 功能提交：483f007，feat(settings): 区分系统能力状态并支持只读刷新。
- 前阶段提交：b7d02ba，feat(preferences): 完善原生偏好持久化与失败保护。
- 本文在功能提交后单独提交；没有推送。

## Handoff Chain
- Continues from: [上一份交接](2026-09-11-190210-desktop-e5a-note-save.md)。
- 对端：[鸿蒙 E6b3 交接](D:/Develop/EggDoneHarmony/.claude/handoffs/2026-09-12-001407-harmony-e6b3-notification-status.md)，功能提交 ddd5dd9。
- 本文更新阶段状态，不覆盖旧文档的历史测试记录。

## Current State Summary
双端体验收敛已推进到 E6b3：区分用户设置意图与系统能力实际状态。桌面设置独立读取快捷键偏好、实际注册状态和开机启动状态，支持只读刷新、未知状态、未注册提示及主动重试。功能已本地提交，本文仅完成用户要求的交接，不启动下一阶段。

旧交接之后，便签附件问题已获用户真机复测通过，任务撤销和返回上下文保护已有后续提交。E6b1～E6b3 的人工验收仍未确认，不能把自动化通过写成全部验收通过。E6c、E6d、E7 未在本次启动。

## Important Context
- 当前授权是双端 handoff 和本地提交，不推送、不合并 main、不发布、不提升版本、不继续 E6c。
- 快捷键“希望启用”与“系统已注册”分开；注册失败不覆盖用户偏好。原生读取失败不得用默认值覆盖已保存设置。
- 打开设置、窗口获得焦点只读刷新，不隐式注册快捷键或强制开启开机启动。
- 浏览器使用模拟系统能力，不代表 Windows 原生快捷键冲突或 Linux/macOS 验收。
- 鸿蒙已通过审核的实况窗、小艺、同步协议及凭据处理没有在本次修改。

## Architecture Overview
Svelte 设置面板负责状态和命令；desktopSettings API 独立编排各项原生能力。CapabilityStatus 区分 unknown、enabled、disabled、inactive、unsupported。初始化迁移与只读刷新分离，一项查询失败不阻止其他项显示。前阶段 b7d02ba 的原生偏好持久化是依赖，本次没有 Rust、数据库、版本或依赖变更。

## Critical Files
| 文件 | 用途 |
| --- | --- |
| src/lib/api/desktopSettings.ts | 原生偏好、注册状态、开机启动及只读刷新 |
| src/lib/components/SettingsPanel.svelte | 状态展示、焦点刷新及操作反馈 |
| src/lib/api/desktopSettings.test.ts | 失败、恢复及注册状态回归 |
| scripts/check-system-capabilities.mjs | 浏览器设置面板回归 |
| src/lib/utils/preferenceStorage.ts | 既有偏好存储入口 |
| src-tauri/src/general_preferences.rs | 前阶段原生偏好持久化 |

## Files Modified
483f007 共 11 个文件，391 行新增、48 行删除，包含 API、设置 UI、测试、中英文资源及 README/计划/系统能力状态说明。精确清单以 git show 483f007 --stat 为准。本交接提交只新增本文。

## Decisions Made
- 偏好读取成功才允许修改；失败显示未知并提供刷新。
- 已启用但未注册显示 inactive，由用户主动重试。
- 操作完成后重新查询，保留既有更新失败回滚。
- 保留 mounted/busy 防护，避免迟到读取覆盖销毁或操作中的状态。

## Verification
本次提交前重新执行：pnpm exec vitest run src/lib/api/desktopSettings.test.ts，1 个文件、15 个测试通过；暂存差异检查 git diff --cached --check 通过，并已审阅生产代码范围。

上一开发轮证据，非本次交接重跑：全量 259 个测试、25 个文件通过；系统能力浏览器回归 8 个 UI 场景、6 个失败场景，以及静态检查和构建通过。前阶段 Rust 偏好测试未在本次重跑。

待人工确认：真实 Tauri 快捷键占用、开机启动状态、设置重启保留，以及 E6b1～E6b3 完整体验。没有新编译 Linux 包或验证 Linux GUI。

## Immediate Next Steps
1. 读取当前 Git 状态、本交接和 roadmap，避免按旧 handoff 重复开发。
2. 收集 E6b1～E6b3 人工验收：搜索/滚动上下文、偏好重启保留、系统能力状态及重试。不要以自动化替代。
3. 用户明确继续后再进入 E6c 固定常用筛选，先对齐双端固定偏好与临时筛选状态的边界。
4. 当前只完成交接提交，不自行开始 E6c、E6d 或后续关联功能。

## Assumptions Made
用户此前对附件、撤销、返回的验收结论仍有效，但不外推为系统能力阶段已通过。分支与版本本次已核对，恢复会话仍需重新读取。

## Potential Gotchas
isRegistered 查询失败与未注册不同；unknown 不能显示为关闭。浏览器 unsupported 不代表原生系统结果。旧交接待办可能已由后续提交完成。本次未启动后台服务，也未停止用户既有进程；恢复时检查实际状态。

## Environment and Commands
Windows PowerShell，Node 位于本机 nvm 的 v24.20.0。中文脚本设置 PYTHONUTF8=1。在仓库根目录执行：
```powershell
pnpm.cmd exec vitest run src/lib/api/desktopSettings.test.ts
git status --short --branch
git log -5 --oneline
```

## Related Resources
- [体验 roadmap](../../docs/EXPERIENCE_EVOLUTION_ROADMAP.md)。
- [系统能力状态](../../docs/SYSTEM_CAPABILITY_STATUS.md)。
- [设置持久化](../../docs/PREFERENCE_PERSISTENCE.md)。
