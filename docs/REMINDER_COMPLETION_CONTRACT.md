# 通知完成任务契约

## 平台边界

| 平台/通知类型 | 本轮实现 | 降级与限制 |
| --- | --- | --- |
| Windows 原生 Toast | 新增完成按钮，保留稍后10分钟和今天晚些时候；正文仍打开任务 | 依赖当前托盘进程的激活回调；不承诺退出全部进程后的冷启动动作 |
| macOS / Linux | 保留现有普通通知 | 本轮不添加未经验证的原生动作按钮，回应用完成任务 |
| 鸿蒙普通通知 | 两个按钮为稍后10分钟、完成；完成按钮通过 START_ABILITY 进入应用并执行动作 | 不是静默后台完成；锁屏可能需要解锁，需真机验证 |
| 鸿蒙代理提醒 | 保留系统稍后、关闭及正文打开任务详情 | 当前 SDK ActionButton 只有 CLOSE / SNOOZE，不能把关闭按钮伪装为完成；进入详情后使用完成操作 |

不改变已通过审核的专注实况窗。完成任务不等于结束专注，两条业务链保持独立。

## 动作与幂等

- 使用 Todo UUID 和提醒时间（毫秒）锁定本次提醒；不以本机自增 ID 作为跨入口定位参数。
- 鸿蒙动作名为 `eggdone.action.COMPLETE_TASK`，携带 `taskUuid`、`reminderAt` 和 `reminderActionId`。ID 为 `reminder.complete.v1:<uuid>:<reminderAt>`，缺失、格式错误、时间不匹配时不执行。
- Windows Toast 的完成按钮参数为 `complete:<uuid>:<reminderAt>`，激活回调只接受本条通知的完整参数；正文不触发完成。
- 两端完成均为“设置为已完成”，绝不 toggle。已完成、删除、归档、不存在、提醒已改期均不再改变任务。
- 已处理记录使用已有本机 `app_metadata`，键为 `reminder.complete.v1:<uuid>:<reminderAt>`。与任务完成、重复任务下一实例生成处于同一事务，失败一起回滚。
- 收到已完成任务的动作也记录该动作，防止随后手动撤销完成时旧动作再次生效。新提醒应使用新的提醒时间。
- 记录只用于本机去重，不增加 S3 对象或同步字段，不改变 schema 16；数据库完整副本会自然保留这些记录。
- 鸿蒙页面、批量完成、桌面卡片和通知复用 Repository 完成入口，读取当前数据库记录而非使用旧页面对象生成重复任务；同进程完成操作串行化。
- Windows 面板与通知复用事务内完成用例。完成后更新托盘角标和发出列表刷新事件，不主动展示面板。
- 鸿蒙完成后取消对应提醒、刷新桌面卡片及页面；取消旧提醒不会取消同任务后来重新安排的代理提醒。
- 两端数据写入仍由既有触发器标记同步 dirty，网络失败不撤销本机完成结果。

## 自动验证

2026-09-05：
- 桌面 `pnpm release:check` 通过：82 项 Vitest、119 项 Rust 测试、类型检查、构建和国际化检查。新增4项通知完成数据库回归。
- 鸿蒙 `node scripts/test-reminder-completion.cjs` 的7项回归通过：运行实际 migration 和 Repository，使用宿主 SQLite 适配 RDB；覆盖重复/并发动作、旧提醒、已删除/归档、手动撤销后的旧动作、事务回滚和 migration 重跑。
- 鸿蒙75项 Local Test、Debug构建、静态检查及国际化检查通过；既有异常处理/废弃接口等警告仍存在。
- 上述结果不是 Windows 原生通知或鸿蒙系统服务验收。

## 人工验收

1. Windows 托盘常驻、面板隐藏：普通任务提醒点击完成，任务完成、角标减少、面板不意外弹出；点击正文只打开任务。
2. 鸿蒙普通通知：分别在前台、后台、杀进程后、锁屏中点击完成，进入应用后任务完成；重复 Want 不再改变任务。
3. 两端重复任务只生成一个下一实例。连续点击、重新进入、完成后手动撤销再点击旧通知都不会重复生成。
4. 通知出现后改提醒时间、删除或归档任务，再点旧通知，不影响任务及新的提醒。
5. 鸿蒙代理提醒保持稍后/关闭的原语义，正文进入正确任务详情后可完成；不得声称它支持直接完成按钮。
6. 完成后在页面、鸿蒙桌面卡片、同步详情中检查一致性，再验证真实 S3 / MinIO 双向同步。
7. 中英文、亮暗主题检查系统按钮标题；使用实际发布 Profile 测试升级保留数据。
8. 模拟器/宿主测试不能替代通知授权、代理提醒能力和系统冷启动验收。

## 依据

- [华为：为通知添加行为意图](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/notification-with-wantagent)
- [华为：后台代理提醒](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-reminderagentmanager)
- [微软：应用通知内容](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-content)
- 同时核对当前 DevEco SDK `@ohos.reminderAgentManager.d.ts` 和 `notification/notificationRequest.d.ts`：手机普通通知最多2个动作，代理提醒按钮不提供自定义 WantAgent。
