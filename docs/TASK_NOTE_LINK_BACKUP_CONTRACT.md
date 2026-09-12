# 任务与便签关联备份恢复契约

更新：2026-09-12。E7/L3c已本地提交：桌面 `94ec48b`、鸿蒙 `565a8f3`。本文件保留L3c阶段证据；后续隔离S3集成见[TASK_NOTE_LINK_S3_INTEGRATION.md](TASK_NOTE_LINK_S3_INTEGRATION.md)。不升版、不推送、不发布。

## 1. 格式与兼容

普通 JSON 和完整 ZIP 内的 `data.json` 均升级为数据 `format_version: 3`。ZIP 的 `manifest.json.format_version` 仍为 1，路径、校验和、附件限额和文件回滚方式不变。

| 数据版本 | recurrence | task_note_links |
| --- | --- | --- |
| v1 | 不得出现 | 不得出现 |
| v2 | 必需 | 不得出现 |
| v3 | 必需，沿用 v2 结构 | 必需，即使没有链接也写空文档 |

新字段为 `task_note_links: { format_version: 1, links: [...] }`，使用[关联协议](TASK_NOTE_LINK_PROTOCOL.md)规定的完整记录，包括解绑墓碑。共享可导入示例见 [task-note-link-backup-v3.json](fixtures/task-note-link-backup-v3.json)。

- 不允许 null、错误版本、未知链接字段、缺少必需字段、错误 UUID、非安全整数、重复身份或超出协议限额；整笔拒绝，不跳过坏记录。
- 继续支持有效 v1/v2；缺少链接字段不意味着清空本地链接。
- v3 仍包含原有任务、分组、便签、附件字段与重复规则及实例回执。不复制便签正文或附件到链接记录。
- 不导出链接创建操作回执、revision、ACK、ETag、配置世代、凭据或本地缓存路径。重复实例的可移植回执维持既有契约，不等同于链接操作回执。
- 仅支持 v1/v2 的旧客户端不能导入 v3。双端均升级后再交换新备份；不能通过降低版本号或去掉字段绕过校验。
- S3 中各独立对象的格式和 Key 不变，不能把备份 v3 交给同步对象解析器。数据库版本本轮不变。

## 2. 合并及解绑保护

恢复是合并，不是全库替换，也不等同于用户显式重新关联。

1. 在同一数据库事务内合并任务、便签、附件元数据与重复规则。
2. 如有 links，先读取本地链接墓碑。对应 UUID 在本地已解绑时，不接受备份中的活动链接，即使备份时间戳更大或来自未来。
3. 其他链接使用既有完整记录冲突决胜规则；较新的本地显式重新关联不因旧墓碑被覆盖。缺失端点保留为悬挂关系，不制造任务或便签。
4. 所有版本恢复都检查合并后的显式实体删除标记，并在同事务写链接墓碑；仅缺失、完成或归档不是删除依据。恢复实体不会自动恢复已解绑关系。
5. v3 恢复将链接标为待同步：revision 递增、synced_revision 归零、ETag 清空。空 links 或内容相同也重新验证远端状态，不把备份当作同步成功回执。
6. v1/v2 不因缺少链接字段而改写链接 ACK；若实体删除实际影响链接，仍正常标脏。
7. 上述规则只保证备份恢复不会复活本地解绑；后续远端合法变更仍遵守正常同步冲突规则，不宣称覆盖任意未来云端操作。

## 3. 原子性与失败

- 桌面复用全局同步运行锁和 SQLite 事务；鸿蒙复用 SyncSessionLock、既有写入队列与 RDB 事务。
- 任一解析、合并、链接写入、ACK 更新、时钟/计数上限或事务提交失败，整笔数据库恢复回滚，重试从当前状态重新执行。
- 完整 ZIP 复用既有预检、SHA-256、附件恢复和文件回滚。没有引入新的文件权限/API。
- 不恢复来自备份的远端回执；切换同步目标、同步冲突预算、已通过审核的实况窗和小艺均不改动。

## 4. 用户可见变化

现有“数据管理”的普通导入、完整备份恢复预览增加：

- 备份中的关联记录总数及已解绑记录数，明确保留本地解绑。
- v1/v2 显示“不含关联信息”；保留现有关联，但随实体删除处理受影响的链接。
- 数量表示备份记录数（含墓碑），不是实际新增数量，也不是可见活动关联数量。
- 中英文均提供关联校验/恢复失败摘要。附件文件是否包含的提示继续独立展示。

本阶段没有开放任务或便签的关联按钮；不能要求用户通过尚未开发的 UI 生成测试关系。

## 5. 自动化证据

2026-09-12，本机验证：

- 桌面前端 269 项通过；Svelte 检查 0 错误/0 警告；构建、618 键国际化检查通过。
- Rust 全量 248 项通过、2 项真实 S3 测试保持忽略；新增 4 个聚合测试覆盖共享 v3、旧版本、解绑保护、错误输入、故障回滚/重试与真实 ZIP 读写/校验和。
- 鸿蒙备份专项 21 个场景通过；重复规则备份 22 组、关联快照 15 组、生产同步会话 9 组回归通过；资源检查、Debug 与 ohosTest 构建通过。
- 鸿蒙宿主 SQLite 与平台文件操作适配不等于原生 ZIP/文件选择器验收。完整文件恢复的故障注入使用测试替身，明确与桌面真实 ZIP 校验测试区分。
- 手机模拟器 Mate 80 Pro Max（127.0.0.1:5557）和 MatePad Pro 13（127.0.0.1:5555）各 5 项 TaskNoteLinkNative 全部通过，0 失败/忽略，完整报告经 check-device-test-report 校验。
- 原生新增验证：备份较新活动链接不能覆盖本地解绑；注入 ACK 写入失败后便签与链接 revision 一并回滚；旧备份不清空链接回执。其余 4 项覆盖既有协议、迁移、重开、原子操作和同步快照。
- 两个模拟器仅覆盖安装，没有卸载或清空用户数据；测试仅操作独立临时库。未执行真实 S3 或用户库恢复。

原生日志（本机临时目录，不提交）：
- `C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-link-l3c-phone-71e321843f104366bd8fa2aadf0f9e3a.txt`
- `C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-link-l3c-tablet-bcf150a3de3d4b17b1f80d429b0e899e.txt`

包 SHA-256：
- 主包：`CD9D287312F4566651AAB3FEE2D7798F8B6CB03C4309B34EB72BCC6C8ABD69CF`
- 测试包：`36E93AD8374E7119F85A01AEE208B14571B08D74F6EA7B23E772866ED55EC614`

可重复命令（相应仓库根目录）：
- 桌面：`pnpm test`、`pnpm check`、`pnpm build`、`pnpm i18n:check`；src-tauri 下 `cargo test --lib`。
- 鸿蒙：`node scripts/test-task-note-link-backup.cjs --desktop=D:/Develop/EggDone`、`node scripts/test-recurrence-backup.cjs --desktop=D:/Develop/EggDone`、`node scripts/test-task-note-link-sync.cjs`、`node scripts/test-task-note-link-session.cjs`。
- 嵌套 EggDone 工程：`devecocli build --product default --modules entry@default --build-mode debug` 及 `--modules entry@ohosTest`。
- 原生测试用覆盖安装和 `aa test ... -s taskNoteLinks 1`，套件预期为 TaskNoteLinkNative:5；禁止改成卸载重装或复用用户数据库。

## 6. 后续验收

L3c 的代码与上述自动化完成；L3 整体尚未完成。下一步 L3d 真实 S3、跨端离线创建/解绑/冲突/失败恢复，以及真实用户旧库升级验收，再进入 L4 双端 UI。

文件选择器和设备恢复验收应使用隔离测试数据：先导出旧数据作安全备份，再从数据管理分别预览 v1/v2/v3，核对关联计数；导入 v3 后再导出检查一致性；完整 ZIP 核对附件可打开；坏备份确认拒绝且原内容保留。已有真实数据不可作为故障注入对象。旧库和真实云端验收未获证据前保持未完成，既有 E6 人工验证缺口也不因本轮自动化通过而关闭。
