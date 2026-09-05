# 自定义重复任务：时区转换与推进计算

日期：2026-09-05
阶段：DNS5-C1 / HNS5-C1
状态：计算层已实现，未接入任务写入、完成入口、S3 或正式 UI。

本文件记录 C1 边界；后续内部写入事务进展见 [C2 统一事务契约](RECURRENCE_TRANSACTION_CONTRACT.md)。

## 已实现边界

- 输入计划日期、墙上时刻和规则内固定 IANA 时区，输出 UTC 毫秒。
- 全天日期输出空时间戳，保留原始 YYYY-MM-DD；不转换为设备午夜，不冒充“无日期任务”。
- 夏令时空缺按实际空缺长度向后平移；重复时刻选择较早的真实时刻。
- 时间转换不得改写 occurrence key 中的日期/墙上时刻，跨设备仍生成同一个 UUID。
- 不支持的时区拒绝生成计划，不静默回落到 GMT、UTC 或当前设备时区。
- 现有快捷重复、提醒、实况窗、数据库版本和同步格式不变。

## 平台适配

桌面端使用 Jiff 的 compatible 消歧策略，关闭默认特性，只启用 std 与 tzdb-bundle-always。
运行时显式使用随包发布的 IANA 数据库，Windows、Linux、macOS 不读取设备默认时区。
Cargo.lock 固定实际依赖版本；后续更新时区数据必须重跑共享 fixtures。

鸿蒙端通过 LocalizationKit 的 TimeZone.getAvailableIDs 校验 ID，getOffset 读取指定时刻偏移，
getZoneRules().nextTransition 读取真实跳变点及前后偏移。均不调用设置系统/应用默认时区的 API。
转换算法枚举墙上时刻前后两日内的原生跳变点，验证偏移与跳变一致后解析候选时刻；
重复点、非整数、超界偏移、异常跳变序列均拒绝，不靠按小时采样猜测。
当前工程最低兼容 API 23，所用 ZoneRules API 从 API 20 提供，无新增权限。

双端时区数据库更新节奏仍可能不同。共享 fixtures 是回归基线，不保证未来所有地区的政策更新立即一致。
已有下一任务由后续同步合并处理，禁止单凭本机新计算的偏移覆盖已有任务。

参考：
- [Jiff 消歧策略](https://docs.rs/jiff/latest/jiff/tz/enum.Disambiguation.html)
- [Jiff 时区数据库](https://docs.rs/jiff/latest/jiff/tz/struct.TimeZoneDatabase.html)
- [HarmonyOS 夏令时跳变](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/i18n-dst-transition)

## 只读推进计划

planRecurrenceAdvance / plan_recurrence_advance 接收已验证规则、当前任务证据、操作时间和设备 ID。

1. 没有当前任务、不匹配当前 UUID、未完成且未删除、仅归档、规则已停用/耗尽：不推进。
2. 当前任务已完成（允许已归档），或当前任务已删除：只推进一次，只生成紧邻下一项，不补齐历史逾期项。
3. 到达次数/日期上限：标记规则 exhausted，不生成新任务。
4. 下一任务使用确定性 UUID，日期与索引由现有日历引擎派生，时间戳交给平台适配器。
5. 更新时间取 max(操作时间, 原更新时间+1)，时钟回拨不使规则退回；超出安全整数范围则拒绝。
6. 不修改输入对象；同一证据在已推进规则上重放无动作。离线双端计算得到相同下一任务身份。

输出仅含更新后的规则及下一 occurrence 描述，不是可以直接接受客户端提交的写入命令。
此层没有事务、未查询真实 Todo、未写入数据，也不处理下一 UUID 已存在时的数据库碰撞。

## 统一事务前的门槛（C2）

- 从数据库事务内读取当前 Todo 和规则，不使用 UI 的旧副本作为证据。
- 明确全天日期的物理映射：桌面有 due_date，鸿蒙主要保存 due_at，不能直接把本层 null 写成“没有日期”。
- 固定标题、备注、分组、优先级、置顶、排序、提醒偏移及旧 repeat_rule 字段继承规则。
- 当前任务完成/删除、下一任务插入、规则推进、dirty 和通知回执同事务提交。
- 串行协调规则合并和任务完成，不嵌套各自 Repository 的独立事务。
- 下一 UUID 已存在时验证归属，保留已同步编辑；禁止 INSERT OR REPLACE 覆盖用户数据。
- 注入插入/规则写入失败，验证当前任务、下一任务、规则、dirty、回执全部回滚。
- 统一事务通过后，再接入独立规则 S3 对象及上传顺序；这些完成前继续关闭自定义规则 UI。

## 自动化与设备验收

共享 fixtures：
- recurrence-time-v1.json：29 组，常规偏移、UTC、15 分钟偏移、夏令时空缺/重复及边界、30 分钟跳变、日期变更线、非法输入。
- recurrence-progress-v1.json：22 组，完成、删除跳过、归档、孤立规则、重复证据、耗尽、定时、回拨、并发计划身份及错误拒绝。

Rust 直接运行生产实现和随包 IANA 数据库。鸿蒙主机脚本使用真实 ArkTS 源码、Node ICU 偏移和明确列出的跳变数据模拟平台接口，
并在 UTC、Asia/Shanghai、America/New_York 三种主机 TZ 下重放，不把主机通过当作原生 API 通过。
LocalTest 通过纯转换器的合成跳变用例；RecurrenceTimeNative.test.ets 提供 6 项只读设备测试，
覆盖原生 ID 校验、上海/UTC、纽约空缺/重复、Lord Howe 半小时跳变和 Apia 日期变更线。
本阶段不宣称已通过原生设备测试、任务写入事务或真实 S3 双端并发验收。
