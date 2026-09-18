# 双端任务生命周期共享契约

日期：2026-09-17。阶段：LC0b，候选纯规则与共享样例已落地；不是生产 API、数据库迁移或完整同步协议。
配套：[实现方案](TASK_LIFECYCLE_IMPLEMENTATION_PLAN.md)、[Roadmap](TASK_LIFECYCLE_ROADMAP.md)、[共享样例](fixtures/task-lifecycle-v1.json)。

## 1. 本阶段交付边界

冻结业务动作、归档接口语义、版本快照、安排失效规则与后续兼容验收门槛。
两个仓库的本文、fixture 和 Node 候选规则/测试保持逐字节相同；分别运行不等于已验证 Rust、ArkTS 生产实现。
不改 UI、数据库、备份 v5、应用版本、用户数据或小艺意图。LC1a 才实现归档 repository。

## 2. 基线核对与接线清单

以下均为本轮读取的源码位置，路径相对于各自仓库。它们是下一阶段影响清单，不表示本轮已修改。

| 现有路径 | 桌面端 | 鸿蒙端 | 必须保留/补充的边界 |
| --- | --- | --- | --- |
| 归档已完成 | src-tauri/src/commands.rs: archive_completed_todos_in_connection | EggDone/entry/src/main/ets/data/repositories/TodoRepository.ets: archiveCompleted | 当前只设置归档/更新字段；LC1a 新增独立恢复服务，不调用完成动作 |
| 普通删除撤销 | commands.rs: restore_todo_in_connection | TodoRepository.ets: restoreNow / restoreIfUnchanged | 现有行为只恢复当前行，不倒退重复规则；鸿蒙短时回执校验删除时间与版本，不应退化 |
| 回收站恢复 | src-tauri/src/trash.rs: restore | data/repositories/TrashRepository.ets: restore | 清除提醒/重复绑定/归档，保留完成状态；失效分组置空；关系不自动复活 |
| 完成、删除与重复 | commands.rs、todo_completion_recurrence_tests.rs、todo_deletion_recurrence_tests.rs | TodoRepository.ets、RecurrenceTransactionRepository.ets | 当前重复实例可能推进规则；归档历史操作不走推进入口 |
| 统一搜索 | src-tauri/src/content_search.rs | data/repositories/ContentSearchRepository.ets | 目前包含归档，排除删除；LC1b 加动作而不是另写恢复 SQL |
| 主同步 | src-tauri/src/sync.rs | services/sync/SyncMergeService.ets | 现有任务快照合并不能记录所有中间生命周期事件 |
| 备份导入 | src-tauri/src/data_exchange.rs、recurrence_backup.rs | services/data/ 下的数据导入和 RecurrenceBackup.ets | 普通导入、完整备份、系统备份均需接入新终态检查，不能仅保护回收站按钮 |
| 其他恢复入口 | src-tauri/src/notes.rs、note_attachments.rs、note_history.rs | NoteRepository.ets、NoteAttachmentRepository.ets、NoteHistoryRepository.ets | LC2 审计恢复/撤销/版本还原，禁止通过旧入口绕过终态 |
| 幂等正文副本 | task_checklist_operations、task_template_operations 等实际回执及 app_metadata | 对应 checklist/template/batch/intent 回执 | LC2 按实际表名盘点 payload；独立模板内容保留，不把全部模板当缓存删掉 |

当前 recurrence.instance.v1 回执及 reject_purged_restore / rejectPurgedRestore 只保护部分重复实例缺失场景，不能冒充全实体永久删除凭据。
LC1a 不更改普通撤销语义；LC3/LC4 接入时只增加“旧安排不恢复”的新维度规则。

### 已知触发器与清理风险

| 来源 | 现有行为 | 后续处理要求 |
| --- | --- | --- |
| 桌面 db.rs / 鸿蒙 MigrationRunner.ets | notes_soft_delete_attachments、notes_restore_attachments：按删除时间级联附件软删除/恢复 | 永久删除不能先恢复父记录；恢复时只恢复同批附件，不误恢复更早单独删除的附件 |
| 桌面 migrations/019_note_history.sql / 鸿蒙 NoteHistoryMigration.ets | 修改标题/正文捕获旧版本，保留数量限制；删除便签行后删除本机历史 | 不通过“正文置空”伪装清空，否则可能新建历史副本；文件元数据与队列仍需显式处理 |
| 桌面 db.rs / 鸿蒙 MigrationRunner.ets | sync_dirty_* 标记主领域修改 | purge 事务同时保留终态凭据与 dirty 标记，不能误 ACK 未上传变更 |
| migrations/017-021 / 对应 Recurrence、TaskNoteLink、TaskChecklist、TaskTemplateMigration.ets | 独立领域 dirty/revision 触发器 | 领域顺序、revision 上界与回滚都要测；本轮不改触发器 |
| 清单 items / definitions | 按 todo_uuid / rule_uuid 存 JSON，并有创建操作 payload | 不能假定物理删除任务会自动级联正文；保留独立规则定义及仍有效的模板 |
| 关系与重复回执 | 不由一个通用外键承担全部约束 | 删除当前实体不删除关联实体、后继实例、独立模板；保留必要去重证据 |

此表是源码风险盘点，不是触发器注入或真实数据库测试结果。LC1a/LC2b 必须在新库、升级库和失败注入上验证。

## 3. 状态与动作表

生命周期投影优先级：purged > deleted > archived > completed > active。
只有 active 才解释 ready/waiting 及可执行计划；归档的历史未完成异常记录可取消归档，但不自动设为完成。
“等待”与“计划”正交，等待仍可留在计划列表，不计入可立即执行数量。

| 动作 | 父任务结果 | 新安排边界 | 提醒/规则/清单/关系 |
| --- | --- | --- | --- |
| 改标题、备注、日期、分组 | 原生命周期 | 不产生边界 | 现有编辑规则；不能让普通编辑清掉计划 |
| 加入/移出计划 | 不变 | 使用当前边界集合 | 只改该日期成员关系；不改截止/全局排序 |
| 设等待/恢复处理 | 不变 | 使用当前边界集合 | ready 必须显式记录；可选移出今日计划同事务 |
| 完成 | completed | 新增边界 | 既有后继规则不变；记录只读当日完成计划快照 |
| 普通取消完成 | active | 再新增边界 | 保持原完成撤销逻辑，不恢复旧安排 |
| 归档 | archived | 新增边界 | 不重启规则；有效关联保留 |
| 取消归档 | 保留原 completed/completed_at | 新增边界 | 提醒清空，不注册；保留重复历史绑定、清单勾选及有效关联 |
| 重新打开归档任务 | active；completed_at=null | 新增边界 | reminder_at、repeat_rule、repeat_next_due_date、repeat_series_uuid 清空；清单勾选不变 |
| 软删除 | deleted | 新增边界 | 既有关系删除语义；归档删除不推进后继 |
| 短时撤销删除 | 沿用旧父行恢复逻辑 | 新增边界 | 不恢复旧等待/计划；不擅自替换成回收站的单次恢复 |
| 回收站恢复 | 沿用已有恢复语义 | 新增边界 | 单次任务，无旧提醒/关系重建 |
| 复制、模板、新重复实例 | 新 UUID | 空边界、无安排 | 不继承计划或等待原因 |
| 永久删除 | purged 终态 | 终态压过所有安排 | 保留必要凭据；具体协议待 LC2a |

取消归档和重新打开均清空旧 reminder_at，避免稍后取消完成时意外重新注册历史提醒。
归档三种写动作遇到活动规则仍以目标为 current_todo_uuid 时均返回冲突；只看 repeat_rule 是否为空不够。
检查清单与有效关联只保留，不以归档恢复触发清单重新生成；解绑历史系列不删除 recurrence.instance.v1 等防重复生成凭据。
到期日期保持原值；过期由界面提示，不自动移动到今天。

## 4. LC1 归档命令契约

### 输入/列表/预览

服务名称先固定为 listArchived、previewArchived、applyArchiveAction、applyArchiveBatch；Tauri 命令名与 ArkTS 类名由 LC1a 沿用各端命名风格。
列表条件：archived_at 非空且 deleted_at 为空，并排除未来终态凭据；按 archived_at DESC、uuid ASC 排序。
query 按字面子串搜索标题/备注，不解释 SQL 通配符；最多 200 UTF-16 单元。limit 1..100，默认 50。
游标包含上一条 archived_at、uuid 和查询条件指纹，采用相同排序的 keyset；更换查询重置游标。
这不是冻结全库的历史快照：跨页发生更新时刷新列表，按 UUID 去重。批量“全部”另取事务一致的固定目标，不拼当前屏幕页。

preview 返回展示内容和不可变 expected 快照。expected 必须由权威层构造：
- 身份、全部任务业务字段（不含本机自增 ID、UI 状态）及 updated_at / updated_by。
- 关联分组是否存在/已删除及版本。
- 指向目标的重复规则和绑定证据，稳定排序，包含活动标记与版本。
- 清单、关联的相关行身份与版本/删除标记；JSON 规范化后稳定排序。
- 数据库/同步空间 generation，未来终态证据与生命周期边界版本。

applyArchiveAction 接收 operation_uuid、action（unarchive/reopen/delete）、expected；不允许调用者指定新时间戳或新分组。
服务在同一写事务重新取完整快照，不仅比较 updated_at；即使同时间戳被另一端改过正文/设备/子项，也应冲突。
新 updated_at = max(now, observed_task_updated_at + 1)，必须处于 0..9007199254740991；规则/关系实际写入使用各自单调版本规则。
UUID 输入先严格校验；新契约规范化为小写标准格式，遗留允许形式在入口转换，不改既有数据身份。

### 事务与回执

归档取消/重开保留 UUID、标题、备注、到期、排序、重要程度、置顶及清单；失效分组降为未分组。
归档删除只软删除目标，tombstone 对应关系，不删除便签本身，不推进任何重复实例。
事务必须覆盖任务修改、关系写入（如需）、未来生命周期失效、dirty 状态及本地操作回执；异常全部回滚。
回执以 operation_uuid 唯一，保存 action、目标/快照指纹、结果版本及警告，不额外持久复制正文。
同操作/同参数重试读回执，返回 already_applied 与当前实体状态；目标随后改变时也不重放旧操作。
同操作 ID 改了参数返回 ARCHIVE_OPERATION_CONFLICT。回执不存在而 expected 过时，返回冲突，不能把当前行再次修改当幂等。
导入/切换库或同步空间使旧快照失效；清除旧操作回执或纳入 generation 隔离。回执仅保证本地命令重试，不是跨端全局事务。

结果包含 outcome=applied/already_applied、uuid、action、result_version、warnings；重新读取的当前状态不能伪装成旧操作结果。
warnings 可含 GROUP_RESET、OVERDUE_DATE、POST_COMMIT_REFRESH_FAILED；后者明确“写入成功、刷新失败”，不返回可重放写操作的普通错误。
提交后统一刷新列表、搜索、提醒和同步状态；不在事务内调用系统通知或网络。

### 批量

首版仅 unarchive/delete。准备阶段固定所有目标 UUID + expected，按确认顺序去重（重复输入直接报错），不接受重新执行查询字符串来扩大目标。
默认单批最多 50 条；整个任务可跨批，持久保存 operation_uuid、固定目标指纹/版本及每项结果，避免正文副本。
每批写事务预检所有目标；过期项记录 skipped_conflict，缺失/已改变状态项分别报告，不重删其他设备恢复的任务。
有效项在该批内原子提交；数据库失败回滚该批及其结果，已成功的早前批不回滚。继续同 ID 只处理未终结项。
取消在提交前零写入；运行后停止不回滚已完成批，报告已完成/未执行。新进归档/回收站项目不加入旧批。
LC0b 只验证固定目标复制和纯命令冲突；持久进度、批事务与回执将在 LC1a 测试，不在本轮声称实现。

### 错误码

| 类型 | 候选码 | UI 处理 |
| --- | --- | --- |
| 输入错误 | ARCHIVE_INVALID_ACTION、ARCHIVE_INVALID_ID、ARCHIVE_INVALID_PAGE、ARCHIVE_INVALID_VERSION、ARCHIVE_EMPTY_SELECTION、ARCHIVE_DUPLICATE_TARGET | 保留面板，定位输入，不自动重试 |
| 记录变化 | ARCHIVE_NOT_FOUND、ARCHIVE_NOT_ARCHIVED、ARCHIVE_CONFLICT、ARCHIVE_RULE_ACTIVE、ARCHIVE_TERMINAL | 刷新预览并解释，不能自动覆盖 |
| 操作回执冲突 | ARCHIVE_OPERATION_CONFLICT | 换新操作需重新确认，禁止透明生成新 ID 重试 |
| 数据库失败 | ARCHIVE_DATABASE_FAILED | 保留草稿/选择，允许同操作 ID 重试 |
| 候选公共校验 | INVALID_ID、INVALID_VERSION、INVALID_DATE、INVALID_REASON、INVALID_BARRIERS、INVALID_WORKFLOW | 生产 adapter 映射至领域码，不将原始 SQL/正文写日志 |

## 5. 计划/等待的生命周期栅栏

### 冻结的逻辑规则

使用每任务、每协议空间的单调增长“边界事件集合 B”，不是仅看 updated_at 或当前 completed。
完成、取消完成、归档、取消归档、重新打开、删除和恢复各生成独立事件 ID；命令重试复用同一事件，不按墙上时钟比大小。
事件在权威层与父状态写入同一事务；事件身份绑定空间、task UUID 和操作。跨端合并为集合并集，不能删除或靠时间过期。

安排记录附带创建时观察到的完整边界集合 basis（wire 可编码为可验证的等价摘要，LC3a 冻结编码）。
生效必须同时满足：空间与父身份匹配、父任务活动、非终态、basis 与已知 B 完全相等。
父记录/边界未到但子记录先到时暂不生效；不得用子记录自报 basis 安装权威边界。
普通文字编辑不产生边界，因此不影响计划。不同设备并发产生边界时并集使未观察全边界的旧安排失效，用户可在同步后重新安排。
新重复实例/复制有新父 UUID，旧子项不能借相同日期/标题迁入。

示例：A 看见 B=[] 加入计划；B 完成并重新打开，集合变为 [e1,e2]。即使 A 的计划时间戳非常大，basis=[] 仍不能在恢复后生效。
不能仅记录“最大的边界时间”：另一端较小时间戳的并发生命周期结束也必须使旧安排失效。
短时撤销删除/普通取消完成不恢复旧安排；撤销保留旧任务语义，与“恢复当天计划”是两回事。

### 历史展示与存储门槛

完成时可以另外保留只读完成计划凭据（task UUID、plan_date、completion event 与所观察的计划身份/版本），用于当天“已完成”折叠，不从失效计划推导可执行状态。
归档/删除/终态优先屏蔽历史展示；重新打开后旧完成凭据不作为当前计划。历史凭据不包含等待原因/正文。
该历史展示需 LC3a 专门 fixtures；本轮 active 投影测试不声称覆盖完成历史 UI。

边界集合是本轮证明因果失效的参考表示，不授权无限制生产事件堆积。LC3a 必须明确体积上限、分页/超限错误及有证明的压缩；未证明全设备因果覆盖前不丢事件。
仅靠旧主快照无法看到旧客户端离线“完成后又取消完成”的中间变化。因此 LC3/LC4 同样依赖 LC2a 验证后的能力兼容空间；不能宣称新旧端任意混用仍保证失效。
本轮不写真实 wire 字段、不选 Object Key、不分配数据库/备份版本。LC2a/LC3a 冻结传输编码与迁移后才接生产。

### 日期与等待

日期为真实公历 YYYY-MM-DD（0001..9999），不以 UTC 零点重新解释；today 由调用层按设备当地日期传入。
今日计划逻辑身份为 task_uuid + plan_date；basis 是该次安排的有效性证据，不用父任务日期替代。
每日移出和 ready 为显式状态；同一身份不同版本的合并必须确定性，具体版本字段/排序在 LC3a、LC4a 冻结。
等待原因最多 200 UTF-16 单元，不自动修改原输入；空原因和 null 查看日合法。非法日期拒绝，绝不“猜明天”。
查看日到期只标记 review_due，仍为 waiting，不自动创建计划/系统闹钟。
本轮候选投影只吃已验证、已合并的领域记录；不是不可信网络 JSON 解析器。

## 6. 兼容矩阵与 LC2a 原型门槛

2026-09-18 进度：LC2a-1 已完成候选终态模型、任务单域迁移故障注入及隔离真实 S3 旧内核反例，详见[原型与证据](TASK_PURGE_COMPATIBILITY_PROTOTYPE.md)。下表是完整验收门槛，不因局部原型通过而视为全部完成。用户已接受显式迁移、升级参与设备及保留旧空间；正式 wire、全领域迁移和原生接线仍未冻结。

| 场景 | 必须结果 | 本轮证据 |
| --- | --- | --- |
| 旧端 + 旧空间 | 行为不变，永久删除入口不开启 | 设计约束，未运行旧客户端 |
| 新端仍在旧空间 | 不开放清空/新计划等待协议写入；归档 LC1 可独立工作 | 设计约束 |
| 新端迁入新空间，旧端继续上传旧空间 | 新端不自动吸收旧空间晚到写入；旧空间保留并告知分流 | 原型待 LC2a |
| 新端离线永久删除，另一新端恢复同 UUID | 合并后终态胜出，不能用未来时间戳/恢复操作覆盖 | 候选状态优先级已测；真实同步未测 |
| 旧备份导入已有新空间 | 保留当前终态/边界证据，拒绝同身份复活；缺失领域不代表删除 | 导入实现未开始 |
| 全新独立库导入旧备份 | 不能声称知道从未获得的远端终态；加入空间前先完整拉取凭据再合并/展示 | 原型待 LC2a |
| 本机模式后接旧同步配置 | 先兼容检查/预览，不能直接上传本机终态或合并旧实体 | 原型待 LC2a |
| 父/边界/子分批乱序 | 父不可用优先，未知 basis 不生效，不靠标题/日期匹配 | 候选投影已测；传输未测 |
| 同步中途切换配置 | 旧会话 ACK 不能确认新空间写入 | 接线未开始 |
| 并发迁移/进程终止 | 单一可验证迁移结果，旧空间不删，恢复不会混用新旧配置 | 原型待 LC2a |

LC2a 需在隔离桶/目录上运行旧客户端真实写入、两新端离线恢复竞争、ETag 失败、迁移每个中断点、备份回导和配置切换。
必须包括反例：仅加 manifest、新 JSON 字段或只检查在线设备版本，旧端可以忽略，判定为门槛失败。
迁移前展示备份、未汇合离线修改风险、旧设备升级要求；切换后不支持静默回退旧协议。需要迁移成本决策时先向用户确认。
本文件和参考脚本没有实现上述隔离。清空功能仍禁止开放。

## 7. 迁移分配与分期验收

| 阶段 | 迁移/备份影响 | 完成所需证据 |
| --- | --- | --- |
| LC0b | 无迁移、无格式升级 | 本文、共享 fixture、候选规则及双仓运行 |
| LC1a | 任务原字段够用；幂等/批量进度可复用合适元数据或单独迁移，开工检查实际最新 schema 后决定 | Rust/ArkTS 生产 repository、新库/旧库、失败回滚/并发/回执测试 |
| LC1b/c | 不因 UI 单独改同步格式 | 双端入口、搜索动作、设备与隔离同步验收 |
| LC2a-c | 新终态/清理队列/协议空间与备份格式；版本待落地时分配 | 旧端隔离、真实传输/导入、防复活、物理文件重试 |
| LC3a | 计划、边界证据、完成历史凭据及独立 dirty/ACK | 合并代数、乱序传输、体积上限、时区/跨日 |
| LC4a | workflow 及计划联合事务，沿用边界证据 | 原子失败、等待清除、不继承与迟到失效 |

当前版本仍桌面 1.2.0 / 鸿蒙 1.3.0，SQLite 21 / RDB 22、备份 v5；LC0b 不预占数字。
候选验证命令（分别在对应仓库执行）：

```powershell
node scripts/test-task-lifecycle-contract.cjs --peer=D:/Develop/EggDoneHarmony
node scripts/test-task-lifecycle-contract.cjs --peer=D:/Develop/EggDone
```

第一条用于桌面，第二条用于鸿蒙。测试覆盖归档字段保留、单次重开、同戳冲突、活动规则、非法版本、固定目标、状态优先级、旧 basis 失效、日期和 UTF-16 长度，以及集合合并交换/结合/幂等。
候选 API 不是 Rust/ArkTS adapter，未覆盖数据库事务、持久批量回执、系统提醒或真实 S3。下一阶段是 LC1a，不是 LC1 界面已经交付。
