# 任务与便签关联协议 v1

更新：2026-09-12。E7/L1 的双端冻结契约；后续实现必须遵循本文件。L1 仅交付协议、合并内核、迁移和持久化，尚未接入用户入口、同步或备份。原子业务操作在 L2、网络与备份在 L3、界面在 L4。

## 文档与身份

独立对象 task-note-links.json，不修改 Todo.note、Todo/Note 主记录、自定义重复或附件协议。JSON 根对象严格只有 format_version、links，版本为数字 1。每条记录严格含以下七个字段，deleted_at 不能省略，活动关联必须显式写 null。未知字段、版本、无效值、重复 UUID 整笔拒绝。

```json
{
  "format_version": 1,
  "links": [{
    "uuid": "b5d16c58-d3c9-5596-a7f4-774b530c3920",
    "todo_uuid": "123e4567-e89b-42d3-a456-426614174000",
    "note_uuid": "123e4567-e89b-42d3-a456-426614174001",
    "created_at": 1000,
    "updated_at": 2000,
    "updated_by": "device-a",
    "deleted_at": null
  }]
}
```

端点是小写、带连字符的 RFC4122 variant UUID，版本 1-5。UUID v5 派生使用 DNS namespace 6ba7b810-9dad-11d1-80b4-00c04fd430c8；name 的精确 ASCII 内容为 eggdone:task-note-link:v1:<todo_uuid>:<note_uuid>。顺序固定为任务、便签，不可互换。身份辅助函数可接收大写并转小写，不去空白；传输记录必须已经规范化。SHA1 仅用于标准 UUID v5，不作为加密或身份认证。

相同端点对唯一；uuid 必须匹配派生值。端点身份不可修改；同 UUID 不同端点整笔拒绝。链接不存正文、标题快照、附件或排序字段。

## 字段与限额

- 时间为毫秒安全整数：0 <= created_at <= updated_at <= 9007199254740991。
- deleted_at 为 null 或 created_at <= deleted_at <= updated_at 的安全整数。
- updated_by 为 1-128 个 ASCII 字母、数字、点、下划线、冒号或减号，不允许空白、换行和非 ASCII 字符。
- JSON 数字按安全整数数值验证，1.0、1e0 与 1 等价；字符串数字拒绝。L1 不按本机当前时间裁剪远端时钟。
- 每份文档最多 10000 条，包含墓碑；输入最多 4194304 个 UTF-16 code unit。规范编码仅输出 ASCII，编码后同样限额。
- 输出根字段及记录字段顺序按上述示例，记录按 uuid 升序。展示顺序另按 created_at、uuid，不能依赖传输数组顺序。
- 本地新增活动关联每任务最多 20 条，由 L2 在事务内检查；L1 仅冻结常量。同步并发导致超过 20 条时不截断已有关系，允许解绑，不允许继续本地新增。
- 合并后超文档总量/大小上限，整笔报错并保留本地原数据；不得静默丢弃尾部记录或墓碑。

## 合并与生命周期

对同一 uuid 选择下面元组按字典序最大的完整记录，不拼接字段：

(updated_at, updated_by, isDeleted, deleted_atOrMinusOne, created_at)

updated_by 按 ASCII 字典序；isDeleted 为 0/1。只有时钟及设备标识相同时才由删除优先决胜；较新活动记录可以代表显式重新关联。created_at 用作最终平手决胜，解决同一端点在不同设备首次创建时间不同的情况。完全相同记录幂等，数组顺序不影响结果；合并应满足交换律、结合律和幂等性。

L2 本机修改必须生成严格大于已观察时钟的时间，达到安全整数上限报错，不能回绕；重新关联保留已知 created_at。直接持久化/合并入口不代替本地业务命令，不能绕过 L2 的实体有效性、限额和单调时钟检查。

- 对方文档未包含某条关联，不代表删除；墓碑不按天数回收。
- 尚未收到任务或便签是悬挂关系，保留记录，不推断删除；L1 不查询端点是否存在。
- 明确的实体删除才由 L2/L3 协调关联墓碑；归档和完成保留关联。
- 解绑不删除任务、便签或附件。恢复实体不自动恢复关联；旧备份不能覆盖较新解绑。显式重新关联才可恢复关系。
- 重复任务首版只关联当前实例，不向后续任务传播。实体与关联的原子操作在 L2 实现，L1 未接入。

## Object Key 与兼容性

从现有 Todo Object Key 的同目录派生固定文件名 task-note-links.json，例如 data/todos.json -> data/task-note-links.json。拒绝空键、首尾空白、空路径段、点/双点路径段、反斜杠、控制字符及超过 1024 UTF-8 字节的键；拒绝与 Todo Key 或调用方提供的已占用 Key 重叠。

L3 必须传入便签、重复规则及其它保留对象的 Key 做碰撞检查，并遵循实体先于关联、配置世代隔离及有限 ETag 重试。L1 没有网络请求、上传 ACK 或备份版本变更。

旧客户端在其他设备仍可按旧任务/便签协议同步，不读取或清理此新对象；新客户端收到旧客户端的实体墓碑后再协调关联删除。此为跨设备协议兼容，不是本地数据库降级兼容：升级后的库不支持旧二进制直接打开，旧版会被版本保护拒绝。

## 持久化

桌面 SQLite schema 17 -> 18，鸿蒙 RDB schema 18 -> 19，产品版本号不变。新增独立 task_note_links 表、端点组合唯一约束及双向索引，active 为查询投影，record_json 是完整规范记录。没有外键级联，缺端点不会丢失关联。

独立 task_note_link_sync_state 预留 revision、synced_revision、etag。INSERT/DELETE 或 record_json 实际变化才由触发器递增 revision；相同记录重放不标脏。synced_revision 与 ETag 的写入、ACK 和配置世代绑定留待 L3，不代表本阶段已同步。

迁移中的建表、索引、触发器及迁移版本记录在同一事务，失败回滚后可以重试。Repository 使用 IMMEDIATE 写事务；全部合并、写入与 revision 一起提交或回滚。公开 mergeInTransaction/merge_in_transaction 供 L2 复用事务，调用方遇错必须回滚。并发写入忙错误向上传递，不静默丢弃；L2 负责串行化/有限重试。

独立 snapshot 先读 revision 再读记录，不承诺跨两次读取的原子快照；并发写入可能获得较新记录和较旧 revision，不能把它当成完成 ACK 的依据。事务内 snapshot 一致；L3 实现上传快照/ACK 时必须保留并发修改保护。

## 共享证据与运行

两端 docs/fixtures/task-note-links-v1.json 字节一致，包含 51 组：3 身份、5 有效、24 无效、7 合并、12 Key。额外运行大小、安全整数、顺序无关、结合律、并发超 20 条不丢失、墓碑、悬挂记录、迁移回滚/重试、写入故障及 revision 溢出测试。

桌面：在 src-tauri 运行 cargo test task_note_link_tests --lib（4 个聚合测试）；cargo test --lib 全量本轮 231 通过、2 项真实 S3 测试忽略。SQLite 文件库迁移/重开已验证。

鸿蒙：在仓库根运行 node scripts/test-task-note-links.cjs --desktop=D:/Develop/EggDone。该脚本执行生产 ArkTS，经宿主 SQLite/crypto 适配，不冒充原生 RDB。相关重复协议、日期和提醒回归也通过。

原生独立 suite 为 TaskNoteLinkNative，只有 aa test 显式传 -s taskNoteLinks 1 才运行，不改变默认 suite 数量。用独立临时 RDB 验证 SHA1 UUID、关闭重开、墓碑与幂等、迁移/写入故障回滚和重试；不使用用户 eggdone.db。使用覆盖安装，禁止 onDeviceTest 的卸载流程。结果与操作见 TASK_NOTE_LINK_ROADMAP.md。

