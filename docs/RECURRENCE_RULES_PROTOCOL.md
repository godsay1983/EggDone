# 自定义重复规则同步协议 v1

日期：2026-09-05
阶段：DNS5-B / HNS5-B（规则协议与独立持久化）
状态：字段、校验、合并和本地存储已实现；网络编排、Todo 联动、时区转换和 UI 尚未开放。

## 文档结构

```json
{
  "format_version": 1,
  "rules": [{
    "uuid": "123e4567-e89b-42d3-a456-426614174000",
    "first_todo_uuid": "123e4567-e89b-42d3-a456-426614174001",
    "schedule": {
      "anchor_date": "2026-09-05",
      "frequency": "daily",
      "interval": 1,
      "weekdays": [],
      "month_day": null,
      "end_type": "never",
      "end_date": null,
      "max_occurrences": null,
      "local_time_minutes": null
    },
    "timezone_id": null,
    "current_todo_uuid": "123e4567-e89b-42d3-a456-426614174001",
    "current_date": "2026-09-05",
    "generated_count": 1,
    "exhausted": false,
    "updated_at": 1000,
    "updated_by": "device-a",
    "deleted_at": null
  }]
}
```

根对象只允许 format_version 和 rules；规则和 schedule 也拒绝未知字段、缺字段和错误类型。
nullable 字段必须显式写 null；数字必须为安全整数，1、1.0、1e0 视作相同整数。
每份文档最多 2000 条（含墓碑），JSON 文本最多 1048576 个 UTF-16 code unit；传输层另需字节数和超时上限。
编码按规则 UUID 排序，字段输出顺序固定；不依赖 JSON 输入顺序或设备 locale。

## 规则含义

- uuid：规则标识，小写 RFC4122 UUID v1..v5。
- first_todo_uuid：最初关联的普通 Todo，可复用既有任务 UUID。
- schedule：沿用 [日历与实例标识契约](RECURRENCE_CALENDAR_CONTRACT.md)，包含不可变 anchor_date。
- timezone_id：全天为 null；定时为固定 IANA 时区标识，例如 Asia/Shanghai、America/New_York、UTC。
- current_date：当前计划实例日期，不是完成日期或同步日期。
- generated_count：当前实例的序号，从 1 开始；必须与日历推导序号相同，不接受任意计数。
- current_todo_uuid：序号 1 必须等于 first_todo_uuid；之后必须等于日历契约派生的 UUID v5。
- exhausted：仅当下一实例不存在时可为 true。活动的最后一项在完成前仍为 false。
- updated_at：非负安全整数 UTC 毫秒；只用于同进度冲突决胜，不用于计算下一项。
- updated_by：1..128 个 ASCII 字符，允许字母、数字、点、下划线、冒号、减号。
- deleted_at：null 或不晚于 updated_at 的非负毫秒时间；非空代表整个系列结束。

规则 UUID、first_todo_uuid、schedule 和 timezone_id 不可在同一规则下修改。
修改未来周期须创建新 UUID，并将旧规则置为墓碑；恢复已结束系列也应创建新规则。
本轮 Repository 保存经过校验的完整记录，尚不提供 UI 创建或修改周期入口。

## 时区和漏过实例

- 定时规则绑定所存时区；出差或切换设备时区不自动改规则，也不改 occurrence key。
- UI 可以按当前设备时区显示实际到期时间，但规则编辑应明确原时区。
- 夏令时跳跃造成墙上时刻不存在时，按跳跃时长向后平移；重复时刻选择较早的瞬间。
- 平台转换适配器必须确认该时区可用；不支持时保留规则并报错，不能回退为设备时区或 UTC。
- 当前协议校验时区的字符串形状，不声称检查了时区数据库。双端 epoch 转换/fixtures 尚待实现。
- 每次完成或显式跳过最多生成一个紧邻的计划实例，不依据同步时的“今天”跳过日期，不批量补任务。
- 到达结束日期/次数后，当前任务完成只设置 exhausted，不再生成下一项。

此处明确细化原方案中“时区变化后保留墙上时刻”的表述：保留的是规则时区的墙上时刻，不自动跟随设备时区。

## 合并

1. 两份输入各自完整校验；同一文档重复规则 UUID 直接拒绝。
2. 按 uuid 合并；同 uuid 的不可变字段不同，返回 RECURRENCE_IMMUTABLE_CONFLICT，保留本地数据，不静默选一方。
3. 不可变字段相同时，按以下元组从大到小取整条记录：
   (是否墓碑, generated_count, exhausted, updated_at, updated_by ASCII 顺序, deleted_at)。
4. 因此结束系列不会被较新的旧活动记录恢复；有效实例进度不会因时钟偏差倒退。
5. 合并结果按 uuid 升序输出。最终若两条未删除、未耗尽规则指向同一个 Todo，返回 RECURRENCE_LINK_CONFLICT。
6. 远端缺少某条规则不是删除；只有墓碑表示结束。不自动清理墓碑。
7. 未找到关联 Todo 的规则允许保存为待关联状态，但禁止因此凭空生成任务。

日历+UUID 一致仅是去重基础。当前 Repository 只合并规则，不会修改或生成 Todo。

## 独立 Object Key 和传输约束

- 从 Todo Object Key 的目录推导 recurrence-rules.json，例如 account/todos.json 对应 account/recurrence-rules.json。
- 目录、键名保持原样，不做 URL 解码；拒绝空段、.、..、反斜杠、控制字符和超过 1024 UTF-8 字节的键。
- 推导结果不能与 Todo、便签或附件索引键重合；调用方必须传入这些已配置键做碰撞检查。
- 同一目录只应保存一套任务数据。不同数据集需使用不同目录，不能仅靠更换 Todo 文件名隔离规则。
- 下载 404 才视为首次创建；403、超时、无效 JSON 等不得当空库处理。
- 首次写入使用 If-None-Match: *；更新使用本次下载的 ETag + If-Match，禁止无条件覆盖。
- 412 冲突最多 3 次完整下载/合并/条件写入尝试；仍失败保留 dirty，交由用户重试。
- 旧客户端不认识该对象，自定义实例保持 repeat_rule=null，不改旧 Todo JSON 字段或枚举。
- 本轮只实现键名推导和本地 ETag/版本状态保存；上述请求流程必须在后续 S3 编排阶段落实并测试。

## 本地 schema17

双端新增 recurrence_rules 与 recurrence_sync_state，不改 Todo 表或旧同步格式。
recurrence_rules 保存完整、已校验的规范 JSON，同时维护 UUID、当前 Todo 和 active 索引列；
索引列只由 Repository 从 JSON 派生，不允许 UI 或其他服务直接写表。

活动当前 Todo 有唯一索引。关联不设跨表外键，以支持不同同步对象到达顺序；未关联规则不能用于生成。
采用独立的 revision、synced_revision、etag；INSERT/有效 UPDATE/DELETE 触发器在同一事务内递增 revision。
内容相同的重放不增加 revision；墓碑先写入，释放活动关联，再写入替代规则。
批量合并失败则规则和 revision 一并回滚。

只有上传成功且确认版本仍等于本地 revision 时，acknowledge 才更新 synced_revision 和 ETag。
过期确认返回 false，不能清除新修改。每个配置的上传尝试仍必须串行；ACK 不是并发 S3 写入协调器。
配置切换、恢复备份和规则域启用时，须在后续编排中重置对应 ETag，不能跨桶复用。
该独立状态当前不参与既有“全部同步成功”UI；规则创建入口关闭，所以不会产生用户可编辑但未上传的规则。

鸿蒙端规则 Repository 串行合并、快照和确认。后续接入 Todo 完成时必须使用跨 Repository 的同一事务所有者，
不能在当前 merge() 外再嵌套事务，也不能与 TodoRepository 的单独完成队列拼接后宣称原子性。

## 删除与完成的后续编排

- 用户“结束系列”：写规则墓碑；保留已存在任务，是否删除当前任务由独立用户动作决定。
- 删除单个当前实例：视为跳过一次；新客户端在当前 Todo 墓碑可用时补算下一项。
- 当前任务完成：生成稳定 UUID 的下一项与推进规则必须同事务；耗尽时只标记 exhausted。
- 已归档但完成的当前任务仍可作为完成证据；单纯归档未完成任务不自动生成。
- 当前 Todo 缺失、旧版移除了记录或关联冲突：停止补算并提示，不能用当前时间猜测。
- 同步顺序：Todo 下载合并、规则下载合并、确定性补算、Todo 条件上传、规则条件上传、按版本确认。
- 规则上传前必须确保其当前 Todo 已成功上传；失败保留待同步状态，重试不得重复创建。

以上编排尚未接入生产。备份纳入规则、时区适配、旧客户端真实互操作、网络失败和跨设备并发测试也是开放 UI 的门槛。

## 自动验证

- Rust：cargo test recurrence --lib；测试生产 schema17、Repository、协议和共用 fixtures。
- 鸿蒙：node scripts/test-recurrence-protocol.cjs --desktop=D:/Develop/EggDone。
- 共用 47 组协议/合并/Object Key fixtures，另测大小限制、数字形式和 Unicode 键。
- 主机 SQLite 执行实际鸿蒙 migration 与 Repository；不是鸿蒙 RDB 真机验收。
- 原生 SHA1、IANA/DST 转换、S3 条件请求、应用升级与两端离线同时完成仍需后续集成验证。
