# EggDone 桌面端下一阶段实现方案

配套执行清单见 [NEXT_STAGE_ROADMAP.md](NEXT_STAGE_ROADMAP.md)。鸿蒙端对应方案见 `D:\Develop\EggDoneHarmony\docs\HARMONY_NEXT_STAGE_IMPLEMENTATION_PLAN.md`。

## 1. 文档目标

EggDone 桌面端已经具备 Todo、分组、日期提醒、重复任务、四象限、日历、专注、便签附件、备份、S3 / MinIO 同步和中英文界面。下一阶段不再横向堆叠大型模块，而是优先解决以下问题：

1. 让用户在重启应用后仍能判断本地修改是否已经同步。
2. 在不增加常驻控件排数的前提下，提高任务查找和处理效率。
3. 缩短提醒到任务完成的操作链路。
4. 通过桌面快捷入口快速收集任务或便签草稿。
5. 在保证旧客户端安全的前提下扩展重复任务。
6. 为检查清单、专注历史、任务与便签关联、加密同步保留清晰的后续边界。

## 2. 产品原则

- 保持轻量、离线优先、托盘常驻和快速收集定位。
- Todo、便签和附件继续是独立数据域；不引入账户、团队和服务端依赖。
- 双端共享业务语义，不机械复制平台 UI。
- 所有同步字段必须有 UUID、更新时间、设备标识和删除墓碑语义。
- 本地同步运行状态不上传到 S3，不在设备之间互相覆盖。
- 新入口优先放入现有“更多”、设置详情或上下文菜单，不扩大顶部常驻工具栏。
- 自定义重复属于协议升级，未完成跨端兼容设计前不得先开放单端 UI。

## 3. 阶段范围

| 阶段 | 功能 | 是否修改业务同步协议 | 双端要求 |
| --- | --- | --- | --- |
| NS0 | 状态与文档收口 | 否 | 各端清理过期状态并建立发布基线 |
| NS1 | 同步状态持久化与诊断中心 | 否 | 状态语义一致，本地存储实现不同 |
| NS2 | 智能列表 | 否 | 日期边界和组合筛选语义一致 |
| NS3 | 通知直接完成 | 否 | 复用同一完成、重复生成和同步逻辑 |
| NS4 | 快速收集 | 否 | 捕获草稿语义一致，平台入口不同 |
| NS5 | 自定义重复 | 是，新增独立规则对象 | 协议、生成算法和确定性标识完全一致 |
| NS6 | 后续能力决策 | 待定 | 先验证价值和数据边界 |
| NS7 | 双端回归与发布 | 否 | 同一轮真实对象存储和升级验证 |

## 4. NS0：状态与文档收口

### 4.1 工作内容

- 以当前 `main`、版本 `1.0.7` 和最新 handoff 为基线。
- 将已经实现但仍留有未完成框的 Roadmap 分为“代码完成”和“人工验收待完成”。
- 保留以下发布门槛：Windows 高 DPI、多显示器、代码签名、自动更新、macOS 和 Linux 回归。
- 对便签附件、国际化和 Linux 托盘分别记录自动验证与人工验证，不用构建成功代替 UI 验收。

### 4.2 完成标准

- README、Roadmap、版本和实际功能一致。
- 不再把旧阶段的人工检查误认为未实现功能。
- 后续提交可以明确归入 NS1 至 NS7。

## 5. NS1：同步状态持久化与诊断中心

### 5.1 用户体验

主面板继续显示紧凑同步胶囊：

- `未启用`
- `有修改未同步`
- `正在同步`
- `已同步 HH:mm`
- `离线，等待重试`
- `发生冲突`
- `同步失败`

点击胶囊打开同步详情，不增加新的顶部按钮。详情展示：

- 上次尝试和上次成功时间。
- Todo、便签、附件三个数据域的结果。
- 本地待同步数据域和附件数量。
- 简短可读错误、重试按钮和“复制诊断信息”。
- Endpoint、Bucket、Object Key 可以显示；Access Key、Secret Key、签名头和文件内容禁止进入诊断文本。

### 5.2 本地数据模型

新增本机专属的 `sync_runtime_state`，不进入 JSON 导出、完整备份或 S3：

```text
schema_version          INTEGER
last_attempt_at         INTEGER NULL
last_success_at         INTEGER NULL
dirty_since             INTEGER NULL
dirty_domains           TEXT       # todos,notes,attachments 的稳定 JSON 数组
last_result             TEXT       # never,success,offline,conflict,failed,interrupted
last_error_code         TEXT NULL
last_error_message      TEXT NULL   # 清洗后的用户可读文本
pending_attachment_count INTEGER
updated_at              INTEGER
```

约束：

- 不持久化 `syncing`。启动时发现上次尝试没有完成，归一化为 `interrupted`，并保留 dirty 状态。
- Todo、便签或附件本地写入成功后，必须标记对应 domain dirty。
- 同步只清除实际成功的数据域。Todo 成功但附件失败时，不能显示全局“已同步”。
- 每个 dirty domain 使用仅本机保存的递增版本号；同步只清除开始时读取且上传后未变化的版本，上传期间产生的新修改继续保留为 dirty。
- 成功时间表示远端上传或确认合并成功，不表示仅完成 ETag 检查。
- 错误码稳定、可本地化；底层异常只进入清洗后的诊断详情。

### 5.3 桌面端架构

- `src-tauri/src/db.rs`：增加本地状态 migration。
- `src-tauri/src/commands.rs` 及各数据写入入口：业务事务成功时标记 dirty。
- `src-tauri/src/s3_sync.rs` / 同步 command：记录尝试、成功、分域结果和错误。
- `src/lib/api/syncApi.ts`：增加严格类型的状态查询和诊断 DTO。
- `src/lib/sync/autoSync.ts`：从持久化状态初始化 Svelte store；只负责运行编排，不再是状态唯一来源。
- `src/lib/components/SyncSettings.svelte`：显示详情与重试。
- `src/lib/components/TodoPanel.svelte`：保持紧凑胶囊入口。

为避免遗漏 dirty 标记，优先在 Rust 业务写入层完成，而不是要求每个 Svelte 点击事件自行维护。

### 5.4 恢复策略

- 应用启动：读取状态；有 dirty 时触发一次有限重试同步。
- 应用崩溃或强制退出：下次启动保留未同步提示。
- 网络恢复：沿用前台检查，不做永久后台轮询。
- 用户清除凭据：停止同步，但保留 dirty 状态，避免误报数据已上传。

### 5.5 测试

- 全新数据库和旧数据库 migration。
- dirty 标记在重启后保留。
- 三个数据域部分成功、部分失败。
- `syncing` 中退出后恢复为 interrupted。
- 凭据错误、离线、ETag 冲突、附件失败和重试成功。
- 诊断文本不含 Access Key、Secret Key、Authorization 和签名信息。

### 5.6 2026-09-05 实施记录

- 双端冻结 `SyncRuntimeSnapshot` 字段、六种持久化结果和三个 dirty domain；该快照明确不进入 S3、JSON 导出或完整备份。
- SQLite schema 升至 16，数据库触发器覆盖 Todo、分组、便签和附件写入，避免 UI、托盘、提醒等入口遗漏 dirty 标记。
- 同步 command 记录尝试、分域完成、最终成功或清洗后的失败；每域版本号保护上传期间的新修改不被误清。
- 设置页增加同步诊断、分域状态、待传附件数和安全复制；主面板胶囊从持久化快照恢复，并显示最近同步时间。
- 自动验证覆盖迁移、分域 dirty、并发写入、部分失败和敏感信息清洗；真实 S3 / MinIO、强制退出恢复和窄窗口视觉仍按 Roadmap 人工验收。

## 6. NS2：智能列表

### 6.1 共享视图语义

智能列表只计算现有 Todo，不新增同步字段：

| ID | 名称 | 规则 |
| --- | --- | --- |
| `overdue` | 逾期 | 未完成且到期时间早于本地今天起点 |
| `next7` | 未来 7 天 | 未完成且到期日在今天至未来第 6 天，不包含逾期 |
| `no_date` | 无日期 | 未完成且 `due_date`、`due_at` 都为空 |
| `important` | 重要 | 未完成且 `priority = 1` |
| `recently_completed` | 最近完成 | 最近 7 个本地日内完成且未归档、未删除 |

现有“今天”语义保持不变：今天到期加逾期未完成。

### 6.2 组合规则

- 智能列表遵守当前分组筛选和搜索词。
- `recently_completed` 不受“隐藏已完成”影响；其他智能列表默认只显示未完成。
- 智能列表不改变 `sort_order`，禁止在计算子集里执行拖动排序。
- 日期计算统一使用本地日历边界，覆盖夏令时和跨午夜刷新。
- 上次选择的智能列表只存在本机偏好，不参与 S3 同步。

### 6.3 桌面 UI

- 保留主视图现有紧凑布局。
- 在“更多”菜单增加“智能列表”，展开后显示列表与计数。
- 当前智能列表使用一个可关闭的筛选胶囊表示，不把 5 个入口平铺到顶部。
- 全局快捷键只打开现有面板，不为每个智能列表新增快捷键。

### 6.4 实现位置

- `src/lib/stores/todoStore.ts`：增加纯函数筛选和计数。
- `src/lib/types.ts`：增加本地 `SmartViewId`，不得写入 Todo DTO。
- `src/lib/components/TodoPanel.svelte`：入口、当前状态和空状态。
- `src/lib/i18n/locales/`：中英文文案和复数规则。

### 6.5 测试

- 纯日期、具体时刻、逾期、午夜和夏令时边界。
- 分组、搜索、智能列表组合。
- 最近完成的 7 个本地日边界。
- 500 条任务切换无明显阻塞。
- 英文窄面板不截断。

## 7. NS3：通知直接完成

### 7.1 行为

- Windows 支持的通知增加“完成”操作，保留“稍后 10 分钟”和“今天晚些时候”。
- macOS / Linux 平台不支持动作时继续使用普通通知降级，不伪造按钮。
- 点击完成后不强制打开主面板；系统限制必须激活应用时，允许静默处理后保持面板隐藏。

### 7.2 业务规则

- 通过 Todo UUID 查询当前记录，不使用通知中的旧标题覆盖数据库。
- Todo 已完成、删除或归档时按幂等成功处理。
- 普通任务调用现有完成逻辑。
- 重复任务必须调用现有“完成并生成下一实例”逻辑，不能直接改 `completed` 字段。
- 成功后更新托盘角标、取消旧提醒、持久化 sync dirty，并按现有防抖触发同步。
- 同一个通知动作重复到达不得重复生成下一实例。

### 7.3 实现位置

- `src-tauri/src/reminders.rs`：通知动作定义和参数。
- `src-tauri/src/commands.rs`：抽取可被 UI 和通知共用的完成用例。
- `src-tauri/src/lib.rs`：动作分发与应用状态刷新。
- 前端只接收必要的刷新事件，不复制完成规则。

### 7.4 测试

- 普通、重复、已完成、已删除和不存在 UUID。
- 动作重复投递。
- 通知触发时面板隐藏。
- 完成后角标、提醒和 dirty 状态一致。

## 8. NS4：快速收集

### 8.1 共享草稿语义

快速收集只生成待确认草稿，不自动写入数据库：

```text
CaptureDraft
  target                 # todo 或 note
  title
  body
  source_url
  source_app
```

- 用户确认后才创建 Todo 或 Note。
- 外部文字遵守现有标题和正文长度限制，超限时截断并提示。
- URL 作为普通文本保存，不自动抓取网页、不执行脚本。
- 草稿不参与 S3；确认后的实体继续使用现有 dirty 和同步流程。

### 8.2 桌面入口

- 保留现有全局快捷键打开面板并聚焦新增输入框的行为。
- 增加可配置的“快速新建便签”全局快捷键，与 Todo 快捷入口使用同一冲突检测机制。
- 增加 `eggdone://capture` 或等价命令行参数，供浏览器、脚本和系统分享入口传入文字或 URL。
- 单实例收到第二次启动参数时转发给当前进程，并打开已有 Todo/Note 编辑器，不新建第二个窗口状态源。
- 剪贴板内容只能由用户明确点击“粘贴”后读取，不在快捷键触发时静默采集。

### 8.3 架构

- `src-tauri/src/lib.rs`：单实例参数和协议入口分发。
- 新增 Rust 捕获参数解析模块：只接受白名单 scheme、类型和长度。
- `src/lib/types.ts`：增加本地 `CaptureDraft`，不得进入同步 DTO。
- `TodoPanel.svelte`：接收已校验草稿并复用 TodoInput 或 NoteEditor。
- 全局快捷键设置继续通过现有设置页和系统注册逻辑管理。

### 8.4 测试

- 中文、英文、URL、空文本和超长文本。
- 单实例第二次启动、应用隐藏和编辑器已打开状态。
- 恶意 scheme、路径、控制字符和重复参数。
- 取消不创建空记录；确认后 dirty 和自动同步正常。

## 9. NS5：自定义重复

### 9.1 兼容性原则

现有 `repeat_rule` 只允许 `daily`、`weekly`、`monthly`、`weekdays`。直接写入 `custom` 会使旧客户端校验失败或错误生成任务，因此不得扩展原字段枚举。

自定义重复使用独立对象：

```text
recurrence-rules.json
```

旧客户端不会读取或覆盖该对象；现有四种重复继续使用原字段，保持完全兼容。

### 9.2 规则模型

```text
RecurrenceRuleRecord
  uuid
  current_todo_uuid
  frequency             # daily,weekly,monthly
  interval              # 1..99
  weekdays              # 1..7 的稳定数组，weekly 使用
  month_day             # 1..31 或 last，monthly 使用
  end_type              # never,date,count
  end_date
  max_occurrences
  generated_count
  timezone_id
  local_time_minutes
  updated_at
  updated_by
  deleted_at
```

规则对象是关联关系的权威来源。自定义重复产生的 Todo 对旧客户端表现为普通任务，旧客户端完成该任务后，新客户端同步时仍能根据 `current_todo_uuid` 推导下一实例。

### 9.3 确定性生成

- 每个实例生成稳定 `occurrence_key = rule_uuid + scheduled_local_datetime + occurrence_index`。
- Todo UUID 由双方实现一致的 UUID v5 算法从 occurrence key 派生。
- 两台设备同时发现当前实例完成时会生成相同 UUID，不产生重复任务。
- 每次只生成下一实例，不回填大量错过的历史实例。
- 月度 29、30、31 日使用“当月最后有效日”策略，并在 UI 中明确说明。
- 时区变化后保留本地墙上时刻；同步记录创建时的 `timezone_id` 用于审计和测试。

### 9.4 同步顺序

1. 下载并合并 Todo。
2. 下载并合并 recurrence rules。
3. 对已完成或已删除的 `current_todo_uuid` 执行确定性补算。
4. 上传 Todo。
5. 使用 ETag 上传 recurrence rules。
6. 任一步失败均保留对应 dirty domain，不能误报全部完成。

Object Key 从 Todo Object Key 同目录推导为 `recurrence-rules.json`，设置页只读展示。

### 9.5 桌面 UI

- 保留现有四个快捷选项。
- 增加“自定义…”打开单独弹层，不在任务行内展开全部控件。
- 支持每 N 天、每 N 周指定星期、每 N 月指定日期，以及永不结束、指定日期结束、执行 N 次。
- 列表 chip 使用可读摘要，例如“每 2 周 · 周一/周四 · 至 12月31日”。

### 9.6 测试

- 双端共用 JSON fixtures、UUID v5 fixtures 和下一实例 fixtures。
- 月末、闰年、夏令时、跨时区、结束日期和次数边界。
- 两端离线同时完成后的去重。
- 旧客户端只同步 Todo 时不删除规则对象。
- 规则删除、单次跳过和整个系列结束。

## 10. NS6：后续能力决策

以下能力只进入设计和验证，不在 NS1 至 NS4 中顺带编码：

### 10.1 轻量检查清单

- 每个 Todo 最多 20 项，单层、可排序、可勾选。
- 需要独立 item UUID 和墓碑；不把数组塞进 Todo 整条记录，避免并发覆盖。
- 先验证用户是否需要“完成所有子项才允许完成主任务”。默认不强制。

### 10.2 专注历史

- 首版只保存完成的专注次数和分钟数，提供今日和最近 7 天摘要。
- 默认本地保存，不同步、不做积分和排行榜。
- 若后续需要跨设备统计，再设计独立对象，不能写入 Todo。

### 10.3 任务与便签关联

- 使用独立 link 记录连接 Todo UUID 与 Note UUID。
- 删除任一实体时只删除链接，不级联删除另一实体。
- 先实现“从便签创建任务”和“从任务打开便签”两个核心入口。

### 10.4 加密同步与备份

- 必须先确定密钥来源、跨设备导入、忘记密码和密钥轮换策略。
- 加密应覆盖 JSON、附件原图、预览和 `.eggdone-backup`。
- 不得将解密密码写入 S3、SQLite 普通字段或日志。

## 11. NS7：回归与发布

- 双端使用同一真实 S3 / MinIO 目录验证 Todo、便签、附件和新增规则对象。
- 验证旧版本升级、旧同步对象、旧备份和删除墓碑。
- Windows 完成高 DPI、多显示器、通知动作、代码签名和自动更新验证。
- Linux 验证 ksni 托盘、凭据库、通知降级和 AppImage/deb。
- macOS 验证菜单栏、窗口定位、快捷键和通知降级。
- 运行 `pnpm release:check`，并把人工结果写回 Roadmap。

## 12. 明确不做

- 团队协作、账号系统、任务指派和评论。
- 多级项目、依赖、甘特图和资源排程。
- AI 自动拆分或自动安排任务。
- 为获取后台同步而增加常驻服务或高频轮询。
- 在没有跨端协议和迁移测试时直接修改线上同步格式。
