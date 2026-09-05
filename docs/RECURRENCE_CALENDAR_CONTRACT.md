# 自定义重复：日历与实例标识契约

日期：2026-09-05
阶段：DNS5-A / HNS5-A，双端基础算法；尚未开放功能入口。

## 本轮范围

实现独立的日历规则校验、下一实例计算、occurrence key 和 UUID v5。
不接入任务完成、数据库、网络、提醒、备份或 UI。现有四种 repeat_rule 行为不变。
本文不是完整的 recurrence-rules.json 同步协议；完整文档结构、冲突合并和迁移仍是下一步门槛。

## 日历输入

| 字段 | 约束 |
| --- | --- |
| anchor_date | 首个计划实例日期；1900-01-01 至 9999-12-31，严格 YYYY-MM-DD |
| frequency | daily / weekly / monthly |
| interval | 整数 1..99 |
| weekdays | weekly 必填，ISO 周一=1、周日=7，升序不重复；其他频率为空数组 |
| month_day | monthly 为整数 1..31；0 代表当月最后一天；其他频率为 null |
| end_type | never / date / count |
| end_date | date 时必填有效日期且不早于 anchor_date；其他情况为 null |
| max_occurrences | count 时整数 1..100000，含首个实例；其他情况为 null |
| local_time_minutes | null 代表全天；否则为墙上时刻 0..1439 |

所有不适用的字段显式置空，避免切换模式后残留旧值。日历模型接收已经解码的类型；
未来网络/备份入口还必须做版本、字段类型、体积、条数及未知字段校验，不能用类型断言替代。

首个日期必须符合规则：weekly 的星期应被选中；monthly 应符合指定日号或月末截断结果。
anchor_date、周期参数和时间一旦产生后续实例，不能直接覆盖后复用旧标识；
“修改未来系列”需要在后续 Repository 设计中明确新规则 UUID 和旧规则墓碑。

## 日期计算

- daily：从当前计划日期增加 interval 个日历日，不依赖完成时间或设备今天。
- weekly：首个日期所在的星期一为第零周，每 interval 周激活一次；激活周内按所选星期依次生成。
- monthly：月份按 interval 推进，每次重新用 month_day 计算日期。不把二月截断后的日号带到三月。
- 结束日期包含当天；结束次数包含最初实例。达到限制或超出 9999 年返回 null。
- 当前实例日期必须是本规则的有效计划日期，且未超出结束条件；错误输入返回 INVALID_RECURRENCE。
- 下一实例序号由 anchor_date 和日历规则推导，不信任可变 generated_count。
- 每次完成最多生成一项，不按当前时间跳过计划日期，也不批量回填。长期未完成时，下一项可能已逾期。

## 时区边界

这一层输出计划日期和墙上时刻，不输出 due_at / reminder_at，不能把运算中使用的 UTC 当成业务时区。
三种主机 TZ 下相同结果仅证明日历计算不依赖设备时区，不证明夏令时提醒已通过。

完整规则协议还需在接入前确定持久化 timezone_id 及跨时区变更策略，并实现双端时区转换适配器。
夏令时不存在/重复时刻的 epoch 决策必须使用共享 fixtures 单独验收，不能套用设备“当前 UTC offset”。
在该门槛完成前，不开放定时自定义重复入口，也不宣称跨时区功能完成。

## 稳定实例标识

occurrence index 从 1 开始。规则 UUID 为小写 RFC4122 UUID（版本 1..5）。

```text
eggdone/recurrence/v1/{rule_uuid}/{YYYY-MM-DD}T{date|HH:mm}/{index}
```

全天使用字面量 date，例如 2026-09-06Tdate；定时使用 24 小时制 HH:mm。
生成前重新校验日期与序号，拒绝任意传入的伪造 index。

Todo UUID = UUID v5(DNS namespace, occurrence key 的 ASCII/UTF-8 字节)。
namespace 固定为 6ba7b810-9dad-11d1-80b4-00c04fd430c8。
桌面端使用 uuid crate；鸿蒙端使用 CryptoArchitectureKit SHA1 计算 UUID v5。
SHA1 仅为 UUID v5 算法组成，不用于签名、密码、附件完整性或任何安全校验；失败应向上传播，不能退回随机 UUID。

相同标识仅是并发去重的必要条件，后续仍需唯一索引、事务、合并和冲突重试。
本轮测试不等同于“两端离线完成经 S3 合并只生成一项”的集成验收。

## 后续协议门槛

- 自定义 Todo 保持旧 repeat_rule 为 null，不新增 custom 枚举。
- 独立规则 Object Key 按 Todo Object Key 同目录推导为 recurrence-rules.json；路径碰撞需拒绝。
- 冻结完整规则文档、当前实例关联、删除/跳过/结束系列语义，以及 ETag、dirty 和冲突决胜。
- 冻结跨时区和夏令时转换后，增加两端 migration、Repository、备份和独立同步域。
- 两端完成同一协议且通过重复生成/旧版本兼容集成测试后，再开放手机、平板和桌面 UI。

## 验证入口

共用数据：docs/fixtures/recurrence-v1.json（两仓库内容必须完全一致）。
日期期望值显式固定；UUID 值由独立 node:crypto 参考计算产生。

桌面端：

```powershell
cd src-tauri
cargo test recurrence:: --lib
```

鸿蒙端：

```powershell
node scripts/test-recurrence.cjs --desktop=D:/Develop/EggDone
```

主机测试运行真实 ArkTS 引擎转译代码，CryptoArchitectureKit 使用 node:crypto 替身；
原生 API 行为仍需后续设备验证。LocalTest 覆盖月末恢复、结束次数、间隔周和非法当前日期。
