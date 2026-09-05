# 自定义重复规则一致快照契约

适用：桌面 DNS5-D2C1 / 鸿蒙 HNS5-D2C1。两端保持同一份契约，基于规则 v1、关联准备和全天日期映射，不改变同步 JSON、数据库版本或已开放 UI。

## 目标与边界

在一个 IMMEDIATE 事务中完成关联校验、必要补算和任务/规则序列化。避免把推进前的 Todo 与推进后的规则拼成上传数据。

这是内部本地准备接口，不是网络同步接口：
- 不发 HTTP 请求，不清除 dirty，不更新远端 ETag，不报告同步成功。
- 不注册系统提醒；后续生产编排需安排补算完成后的提醒及 UI 刷新。
- 不接入普通任务操作、快捷重复选项或自定义规则编辑器。
- 使用已有设备 ID，缺失时失败，不在快照事务中临时生成新身份。

## 原子步骤

1. 开启 IMMEDIATE 事务；鸿蒙通过 RecurrenceQueue 串行进入一次。
2. 用同一事务读取规则和关联任务，校验所有活跃规则的依赖。
3. 仅为未关联的首实例补齐 repeat_series_uuid；保留标题、日期及其他业务字段。
4. 有 missing/conflict/archived 时回滚整个准备过程，返回空 snapshot 和阻塞诊断。
5. 按规则 UUID 排序，选择第一个 reconcile，通过借用当前事务的推进接口补算。
6. 重新读取并校验全部关联，直到没有 reconcile。已完成或已删除的当前实例才提供推进依据，不依据“日期已过”自动完成。
7. 在同一事务读取完整 Todo 文档（含分组）和完整规则文档，沿用正式协议校验与全天日期映射。
8. 捕获 todos_dirty_version 与 recurrence_sync_state.revision；序列化为独立 JSON 字符串后提交。
9. 只有提交成功才返回非空 snapshot。写入、校验、序列化、提交任一失败均回滚。

单次最多执行 128 次推进（包括规则达到结束条件）。第 128 次后已达到稳定状态可成功；还需第 129 次则报 RECURRENCE_RECONCILE_LIMIT，全部回滚。返回空结果或超限都不能作为可上传快照，也不能自动无休止重试。

## 返回数据

| 字段 | 语义 |
| --- | --- |
| snapshot | 成功为快照，阻塞为 null / None |
| links | 稳定关联或阻塞诊断，不是远端上传证明 |
| todo_json | 已校验的完整 Todo 同步文档字符串 |
| rules_json | 已校验的完整规则同步文档字符串 |
| todo_revision | 当前事务内 Todo dirty 版本 |
| rule_revision | 当前事务内规则版本 |
| advanced_count | 本次实际推进次数，重放稳定状态为 0 |

阻塞诊断可以描述事务内曾经推进到的实例；返回时这些试算修改已回滚，诊断不可用于直接修改数据库。必须重新准备。

事务提交后的用户编辑不会修改已返回的字符串，但会令快照版本落后。因此：
- 快照不是长效上传许可，不应持久保存后跨同步尝试复用。
- 不能直接清除 dirty；后续 ACK 必须在上传成功后按捕获版本做条件确认。
- Todo 与规则有独立版本，不允许以任一成功代替另一方成功。

## 关联和数据保护

- 缺失源任务不重建，缺失后续实例不猜测关联。
- 已有实例生成回执但任务已被物理清除时，不重新创建；返回阻塞并回滚试算。
- 已存在的下一实例保留用户标题、改期和提醒等字段；如果已完成则继续校验补算。
- 后续实例属于其他系列、未关联或仅归档未完成时，失败或阻塞，不部分提交。
- 无活跃规则时仍可返回普通 Todo 文档，不产生额外业务写入。
- 稳定状态重放不重复生成、不递增 dirty；仅新的 generated_at 可随调用时间改变。

## 平台入口

- Rust：recurrence_snapshot::prepare_snapshot；复用 prepare_links_in_transaction / advance_in_transaction。
- ArkTS：RecurrenceSyncSnapshot.prepare；复用 prepareInTransaction / advanceInTransaction / buildDocumentInTransaction。
- 借用事务的方法不拥有 commit/rollback，禁止内部再次开启事务或排队，以免嵌套事务和死锁。
- 错误由拥有事务的上层处理，不能通过 catch 后继续上传隐藏失败。

## 自动验证

- 桌面 recurrence_snapshot_tests：关联、重放、独立快照、全天/定时、提醒偏移、规则结束、多步补算、后置阻塞、事务写入/提交失败、回执防复活、128 次边界。
- 鸿蒙 scripts/test-recurrence-snapshot.cjs：生产 ArkTS 经宿主 SQLite/crypto 适配执行，额外覆盖队列并发和提交失败重试。
- 复用 recurrence-document-v1、日期往返及关联 fixtures。日期时区与派生 UUID 仍由既有契约约束。
- 宿主 SQLite 不是原生 RDB 隔离验证；LocalTest 和构建不是设备或真实 S3 同步验收。

运行：

~~~powershell
# 桌面：D:/Develop/EggDone
pnpm release:check

# 鸿蒙：D:/Develop/EggDoneHarmony
node scripts/test-recurrence-snapshot.cjs --desktop=D:/Develop/EggDone
node scripts/test-recurrence-links.cjs --desktop=D:/Develop/EggDone
node scripts/test-recurrence-transaction.cjs
node scripts/test-todo-date-roundtrip.cjs --desktop=D:/Develop/EggDone
~~~

## D2C2 后续门槛

仍未完成：生产同步配置快照与切换防护、下载合并、Todo 先于规则条件上传、有限的完整流程重试、分领域 dirty CAS、错误文案和提醒刷新。网络操作不得持有本地数据库事务。

备份包含规则、普通操作接入和两端真实 S3 验收完成之前，自定义重复 UI 继续关闭。用户已确认当前开放功能同步正常，不据此勾选自定义规则同步验收。
