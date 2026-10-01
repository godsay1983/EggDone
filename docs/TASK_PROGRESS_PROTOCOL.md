# 任务进展记录协议 v1

- 日期：2026-10-01
- 状态：TP-0 契约；存储、用户界面和正式同步接入属于 TP-1。
- 方案：[TASK_PROGRESS_IMPLEMENTATION_PLAN.md](TASK_PROGRESS_IMPLEMENTATION_PLAN.md)。
- 样例：[fixtures/task-progress-v1.json](fixtures/task-progress-v1.json)。
- 两端协议和样例必须一致。schema 27、数据导出 v9 当前仍可用，实施迁移时再次核对。

## 1. 文档与记录

文档只有 format_version 和 entries，format_version 必须为数字 1，entries 必须为数组。记录字段全部必填，包括值可为 null 的 deleted_at；拒绝未知字段、重复 JSON 对象成员、重复记录 UUID 和无效类型。

规范编码按下面的字段顺序生成紧凑 JSON，entries 按 uuid 的 ASCII 升序排列：

```json
{
  "format_version": 1,
  "entries": [{
    "uuid": "11111111-1111-4111-8111-000000000001",
    "task_uuid": "11111111-1111-4111-8111-000000000010",
    "body": "完成字段映射，还需要测试中文路径。",
    "created_at": 1790810400000,
    "created_by": "desktop",
    "updated_at": 1790810400000,
    "updated_by": "desktop",
    "clock": 1,
    "deleted_at": null
  }]
}
```

uuid、task_uuid 为小写规范 RFC4122 UUID，版本 1 至 5，与现有任务 UUID 校验一致。created_by、updated_by 长度 1 至 128，仅允许 ASCII 字母、数字、点、下划线、冒号和连字符，不展示原始设备标识。

clock 是 1 至 9007199254740991 的整数。所有时间是 0 至 9007199254740991 的 UTC 毫秒整数，updated_at 不早于 created_at。数字 1.0 和 1e0 与 1 等价；按原始十进制数字的精确值校验，不允许将小数或越界值舍入成合法整数。拒绝带负号（包括 -0）、小数值、非有限值、布尔值、数字字符串或越过安全整数范围的值。

创建时 clock=1；时钟推进辅助函数可用本地哨兵 0 返回 1，0 不允许出现在记录文档中。后续写入取已知记录的 clock+1，达到最大值时返回 PROGRESS_LIMIT。本地写入时间取 max(当前时间, created_at, 上次 updated_at)，容忍系统时间回拨。协议不将逻辑时钟当成展示时间。

## 2. 正文、墓碑与容量

有效正文长度为 1 至 1000 个 UTF-16 code unit，超出长度返回 PROGRESS_LIMIT。允许换行和制表符，禁止未配对代理项、C0 控制符（TAB/LF/CR 除外）、DEL、C1 控制符及 U+202A-U+202E、U+2066-U+2069 双向控制字符。

写入前去除首尾的 ECMAScript trim 空白字符：U+0009-U+000D、U+0020、U+00A0、U+1680、U+2000-U+200A、U+2028-U+2029、U+202F、U+205F、U+3000、U+FEFF。保留内部文字和换行，不做 Unicode 归一化，不把 CRLF 改成 LF。

同步文档中的有效正文必须已经完成上述 trim；全空白或首尾仍有这些空白时返回 PROGRESS_INVALID。输入框可以接受未修整文本，服务层先规范化再保存。两端不能分别使用语言默认 trim 而产生不同结果。

删除记录要求 body="" 且 deleted_at=updated_at，保留不可变身份字段及版本。deleted_at 非空永久表示删除；同 UUID 不支持恢复，重新新增使用新 UUID。

最多 20000 条记录，包含墓碑。原始 JSON 和规范编码均最多 16777216 个 UTF-8 字节。超限返回 PROGRESS_LIMIT，不裁剪记录、正文或墓碑。合并结果也必须满足容量限制。

## 3. 确定性合并

两份文档先分别严格校验，再按 UUID 取并集。相同 UUID 的 task_uuid、created_at、created_by 必须完全相同，否则返回 PROGRESS_CONFLICT，调用方不得改写任何一边原有数据。

同 UUID 先比较是否已删除，删除永久胜过有效记录，不受 clock 高低影响。两条均有效或均为墓碑时，按下列元组取较大者：

    (clock, updated_by 的 ASCII 顺序, body 的 UTF-16 顺序, updated_at)

该元组覆盖全部可变字段，墓碑的 deleted_at 等于 updated_at；不存在未定义的同分取舍。不使用 localeCompare、Rust 的 Unicode scalar 字符顺序或 JSON 属性顺序。输出按 UUID 排序。

同条离线编辑只保留一个版本，不保存完整修订历史。TP-1 写入采用 expected_record 校验当前规范记录版本；正常同步发现未确认本地编辑被替代时提示，未提交输入保持可用。

记录展示顺序为 created_at 倒序，同时间 uuid ASCII 倒序；分页游标为 (created_at, uuid)。编辑不改变创建时间。展示排序与规范传输排序各有用途，不能混用。

## 4. 永久删除与父任务

纯协议合并函数不读取数据库。另提供基于永久删除任务 UUID 集合的过滤函数，过滤本地和远端合并后的所有相关记录，且重新校验输出容量。这个集合必须来自当前目标的已验证生命周期终态，不能来自普通“缺失任务”列表。

TP-1 在同一事务中先导入任务和 lifecycle_terminals，再过滤/合并进展域。待同步记录缺少父任务时保留但隐藏；归档和软删除保留记录且只读；永久删除终态阻止任何旧写入或旧备份带回正文。

永久删除前的快照指纹必须覆盖该任务的进展记录；新增记录导致计划过期时沿用现有重新检查流程。清理正文、操作回执、输入及分页缓存后增加 progress revision，远端确认前不能把新域标为已同步。

## 5. 自动对象路径与同步会话

任务 Object Key 先经当前自动加入/目标切换流程解析为有效路径，使用该路径的原始 UTF-8 字节计算 SHA-256 小写十六进制，不对路径做 trim、大小写变化、URL 解码或 Unicode 归一化：

    eggdone-progress/v1/<sha256>/entries.json

沿用现有 task-note-link 的任务路径合法性校验，并检查与任务对象及其他占用对象路径碰撞。返回 PROGRESS_KEY_INVALID 或 PROGRESS_KEY_COLLISION。不得让用户手填路径，也不得自行创建新空间。

补齐两种运行时已有空白判断的差异：路径首尾的 ECMAScript trim 字符和 U+0085 均拒绝，路径内容不修整。现有 link 派生路径发生碰撞本身不表示任务路径非法，进展域只检查自己的派生路径是否碰撞。

TP-1 接入点：

| 环节 | 桌面端 | 鸿蒙端 |
| --- | --- | --- |
| 独立 revision/generation/ACK | task_progress_sync_state 与上传快照 | TaskProgressSyncSnapshot 与 repository |
| 数据域传输与会话 | task_progress_session / s3_sync | TaskDocumentDomain / TaskProgressSyncSession / SyncService |
| dirty 与同步最终状态 | sync_runtime_state、会话最终 token | SyncRuntime 状态、会话最终 token |
| 目标切换失效 | sync_target，自动加入与空间切换 | 当前目标 gate、sync_state 重置 |
| 旧固定域迁移保护 | migration_preflight / migration_backup | 当前迁移检查与恢复备份入口 |
| 页面刷新 | 新域状态 store 与既有变更通知 | 新域 store 与页面状态绑定 |

每个 ACK 固定 epoch、generation、发送 revision 和规范文档摘要。同步期间有新写入，旧快照 ACK 不能确认新 revision。ETag 来自条件读写，旧目标结果不能写入新目标状态。

首次未发现对象：返回空域，若本地也空则不 PUT。确认见过对象后再遇到 404：PROGRESS_REMOTE_MISSING，保留已有副本。网络、权限、格式错误不能按空文档合并。重试沿用当前有限退避和 ETag 冲突机制。

旧客户端不写此独立对象；旧版永久删除后，新版收到终态负责补清理新域。功能不要求用户手动迁移，不能在任何旧恢复/切换路径中静默丢弃新域。

## 6. 本地写入与备份契约

TP-1 的本地动作带 operation_uuid 与稳定的记录 UUID，编辑/删除带 expected_record。回执保存摘要与记录标识，不复制正文；同操作相同请求重试确认原结果，同操作不同请求返回 PROGRESS_CONFLICT。

本地命令字段固定为 operation_uuid、task_uuid、record_uuid、action、body、expected_record。action 为 create/edit/delete；create 的 expected_record=null，edit/delete 为当前单条记录规范文档（format_version=1、entries 只含该条）的 UTF-8 SHA-256。create/edit 的 body 先规范化，delete 的 body 必须为空。请求摘要使用上述固定字段顺序编码后计算 SHA-256，不包含执行时间；创建和更新时间由服务层生成，不信任 UI 传入时间。

相同操作 UUID 的重试先确认摘要及原提交结果，再检查当前业务权限，避免任务刚被归档造成“提交已成功、重试却报失败”。父任务永久删除后清除回执并阻止重放；其终态优先于幂等成功响应。事务同时提交记录、版本推进和回执，任何一步失败全部回滚。

用户可理解的错误至少区分：正文无效、数量/长度超限、记录已变化、记录已删除、任务只读/不存在、存储失败、网络失败和配置改变。PROGRESS_* 内部码不得代替界面解释。

数据导出 v9 必须包含 task_progress（含空域）；v1-v8 缺域表示保留现有本地域，提供新域则拒绝。v9 无新域或新域无效时整体拒绝。

v9 延续 v8 现有领域的可选/必填语义，仅新增进展域；各已有备份模块需更新版本门禁。完整备份外层容器版本维持现状。导入和同步使用同一合并规则，已知永久删除终态始终优先，不导入另一设备的 ACK、会话或操作回执。

## 7. TP-0 证明边界

共享向量覆盖有效/无效文档、非标准整数书写、Unicode、缺失 nullable 字段、UUID 身份冲突、双向合并、三方结合律、删除优先、永久删除过滤以及路径哈希与碰撞。

Rust 和 ArkTS 的生产协议代码直接执行同一份向量，另外测试 UTF-16 长度边界、逻辑时钟溢出和文档容量边界。Host 转译测试验证 ArkTS 业务函数；DevEco check/build 验证编译。此时未进行数据库迁移、正式网络传输或设备数据写入，不代表 TP-1 已完成。
