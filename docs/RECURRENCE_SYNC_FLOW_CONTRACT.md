# 自定义重复双文档同步编排契约

适用阶段：DNS5-D2C2A / HNS5-D2C2A。前置是 D1 条件传输、D2B 关联准备、D2C1 一致快照。本阶段提供独立编排内核，不启用主同步入口。

## 输入与职责

Rust RecurrenceSyncPort / ArkTS RecurrenceSyncPort<T> 把顺序控制与平台 I/O 分开。Remote 是本轮下载结果的强类型容器，生产适配时必须同时携带 Todo 与规则的条件上传令牌；不得用缓存 ETag 替代。

| 方法 | 适配层必须保证 |
| --- | --- |
| targetIsCurrent / target_is_current | 校验本次目标、启用状态、凭据和配置世代仍有效；不输出凭据或签名 URL |
| download | 重新下载 Todo 与规则，保留本次传输实例所属的令牌；只有真实 404 可视为缺失 |
| prepare | 合并下载结果，调用 D2C1 原子快照；不得绕过关联校验或直接修改生成规则 |
| uploadTodos / upload_todos | 条件上传快照对应的 Todo；true 为成功，false 为 409/412 冲突，其他失败抛错 |
| acknowledgeTodos / acknowledge_todos | 成功上传后按捕获版本确认 Todo，不清除规则、便签或附件 dirty |
| uploadRules / upload_rules | 通过 D1 条件上传同一快照的规则，返回本次 PUT 的 ETag，禁止用后续 HEAD 代替 |
| acknowledgeRules / acknowledge_rules | 仅按规则版本 CAS 确认；版本改变则返回 false |
| snapshotIsCurrent / snapshot_is_current | 收尾检查两个领域版本是否仍匹配，不进行补算或写入 |

所有方法必须等待其 I/O 完成后才返回。数据库事务只能覆盖短的本地操作，不能跨网络 await。适配层必须为整个同步会话提供唯一运行约束；ArkTS 内核额外拒绝同一个 Flow 实例的重入，Rust 通过独占可变借用限制同一个 port，并非跨实例锁。

## 固定流程

每次最多两轮，不能把多种冲突分别各重试两次。

1. 校验配置世代，下载两份远端文档，再次校验。
2. 合并并准备一致快照，再次校验。
3. 快照因缺失/冲突/归档阻塞时，可完整重试一次；持续阻塞返回 RECURRENCE_LINKS_BLOCKED。
4. 先条件上传 Todo。成功后校验目标，再按 Todo 快照版本确认。
5. 再次校验目标，条件上传规则；成功后校验目标，再按规则快照版本和本次 PUT ETag 确认。
6. 收尾检查两个版本与目标，返回尝试次数、分域确认结果、快照版本和待同步标志。

任一个 PUT 返回冲突时，下一轮从下载两份文档开始，重新合并、补算、生成快照、先上传 Todo，再上传规则。第二轮仍冲突返回 RECURRENCE_SYNC_CONFLICT，不仅重试规则 PUT，不无条件覆盖。

网络、凭据、文档解析、补算上限、数据库写入和确认异常直接向上层返回，不伪装为 404，不在本层无限重试。

## 结果与部分成功

- Todo 上传成功、规则上传失败：允许已确认的 Todo 保留确认结果，规则仍待同步；不做虚假的远端回滚。
- 确认版本不匹配：返回 false，保留新修改 dirty。规则仍可上传对应已上传 Todo 的旧快照，不能换用一份新的未配对规则。
- 即使两个 ACK 曾成功，只要收尾发现新修改，localChangesPending / local_changes_pending 仍为 true。
- 本内核不记录全局同步成功、不刷新提醒或 UI；便签、附件结果也不在本结果内。
- returned revisions 与 ACK 布尔值是当次证据，不是未来一直有效的“全部已同步”状态。

## 配置变更防线

内核在下载、准备、写入和确认之间调用配置校验。测试能证明：校验返回 false 后不再执行后续步骤，返回 RECURRENCE_CONFIG_CHANGED。

但校验回调本身不等于已经完成生产配置保护。D2C2B 必须实现：
- 与设置保存和凭据变更共用的世代/运行防护，处理切换后又切回及关闭再开启。
- 数据库确认与目标校验之间的并发窗口保护；不得先异步校验，然后无条件清除 dirty。
- 无法撤回已经发出的请求；配置在请求飞行期间改变时，不用该旧响应清除新目标的待同步状态。
- 旧客户端、空规则库和远端 404 的启用策略；不得让没有规则功能的用户因为新对象权限而同步失败。

这些生产适配尚未完成，所以本阶段不调用 SyncService.syncNow / commands::sync_now_inner 中的新流程。

## 验证证据

共用 fixtures/recurrence-sync-flow-v1.json 的 20 组场景覆盖：
- 成功、Todo 冲突、规则冲突、混合冲突、重试耗尽。
- 依赖晚到、持续缺失、下载/准备/写入/确认失败。
- 下载、准备、Todo PUT/ACK、规则 PUT/ACK 时切换目标。
- Rust 与 ArkTS 测试使用真实快照和数据库确认，传输端为脚本化对端；检查规则引用的实例存在于该轮 Todo 上传数据。
- 额外覆盖三个上传/确认期间编辑场景、重试合并较新的远端任务，以及 ArkTS 同会话并发与失败释放。
- 鸿蒙六项 LocalTest 验证顺序、完整重试、上限、目标切换、pending 和重入。

~~~powershell
# 桌面根目录
pnpm release:check

# 鸿蒙根目录
node scripts/test-recurrence-sync-flow.cjs --desktop=D:/Develop/EggDone
node scripts/test-recurrence-snapshot.cjs --desktop=D:/Develop/EggDone
~~~

传输是替身，不是实际 S3 请求；宿主 SQLite 不是原生 RDB 并发验收。配置回调在测试中可控，不代表系统设置/安全凭据库适配已验证。

## 后续接入

D2C2B：完成真实配置世代、S3/RDB/SQLite 适配、错误映射、提醒和 UI 刷新，再接两端主同步入口并完成旧同步回归。随后继续普通规则操作、备份恢复和真实双端验收；这些门槛通过前不开放自定义重复 UI。
