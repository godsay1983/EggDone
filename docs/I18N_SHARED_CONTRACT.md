# EggDone 双端国际化共享契约 v1

## 1. 适用范围

本契约同时适用于 EggDone 桌面端和 HarmonyOS 端。两端可以使用不同的运行时实现，但必须保持语言模式、术语、语义键、格式化边界和快捷新增语义一致。

契约版本为 `1`。修改本文件时，必须在两个仓库同步更新，并比较文件哈希和快捷新增 fixture。

## 2. 语言模式

| 稳定值 | 中文显示 | English | 行为 |
| --- | --- | --- | --- |
| `system` | 跟随系统 | System | 根据当前系统语言解析 |
| `zh-CN` | 简体中文 | Simplified Chinese | 固定简体中文 |
| `en-US` | English | English | 固定英语 |

默认值为 `system`。系统语言标签以 `zh` 开头时解析为 `zh-CN`，其他暂不支持的语言回退 `en-US`。

语言偏好是本机设置，不写入 `todos.json`、`notes.json`、`note-attachments.json`、S3 同步设置或业务冲突合并字段。

## 3. 品牌规则

| 场景 | 简体中文 | English |
| --- | --- | --- |
| 应用名称 | 蛋定 Todo | EggDone |
| 短名称 | 蛋定 | EggDone |
| 应用说明 | 轻量 Todo 与专注工具 | Lightweight tasks and focus |

品牌属于应用界面资源。用户自己输入的“蛋定 Todo”或“EggDone”按普通用户内容处理，不自动替换。

## 4. 核心术语

| 语义 | 简体中文 | English | 备注 |
| --- | --- | --- | --- |
| Todo | 任务 | Task | 品牌名中的 Todo 不翻译 |
| Notes | 便签 | Notes | 不使用 Memo |
| Group | 分组 | Group | 用户创建的分组名不翻译 |
| All | 全部 | All | 主视图 |
| Today | 今天 | Today | 主视图或日期 |
| Matrix | 四象限 | Matrix | 完整无障碍名称为 Eisenhower Matrix |
| Calendar | 日历 | Calendar | 不改变原有周起始行为 |
| Search | 搜索 | Search | 搜索用户原文 |
| Focus | 专注 | Focus | 阶段和入口统一用词 |
| Break | 休息 | Break | 专注阶段 |
| Paused | 已暂停 | Paused | 状态，不使用 Stopped |
| Completed | 已完成 | Completed | Todo 状态 |
| Due | 到期 | Due | 日期或时间 |
| Reminder | 提醒 | Reminder | 系统通知提醒 |
| Repeat | 重复 | Repeat | 重复规则 |
| Important | 重要 | Important | `priority = 1` |
| Archive | 归档 | Archive | 软归档 |
| Archive Completed | 归档完成 | Archive Completed | 批量操作 |
| Clear Completed | 清除已完成 | Clear Completed | 破坏性操作 |
| Pin | 置顶 | Pin | Todo 或便签 |
| Unpin | 取消置顶 | Unpin | Todo 或便签 |
| Sync | 同步 | Sync | S3 / MinIO |
| Synced | 已同步 | Synced | 完成状态 |
| Not Synced | 未同步 | Not Synced | 有本地变更 |
| Syncing | 同步中 | Syncing | 进行中 |
| Attachment | 附件 | Attachment | 图片和普通文件的统称 |
| Image | 图片 | Image | 便签附件 |
| File | 文件 | File | 普通附件 |
| Backup | 备份 | Backup | `.eggdone-backup` |
| Restore | 恢复 | Restore | 从备份恢复 |
| Live View | 实况窗 | Live View | HarmonyOS 系统能力 |
| Lock Screen Card | 锁屏卡片 | Lock Screen Card | HarmonyOS 系统形态 |
| Home Screen Card | 桌面卡片 | Home Screen Card | HarmonyOS Widget |

## 5. 语义键清单

语义键使用小写英文和点分命名。HarmonyOS 资源名把点转换为下划线。以下为 v1 必备键；实现阶段允许在对应命名空间追加更细键，但不得改变既有键语义。

### 5.1 应用与通用操作

```text
app.name
app.shortName
app.tagline
common.add
common.back
common.cancel
common.close
common.confirm
common.delete
common.done
common.edit
common.loading
common.more
common.next
common.noData
common.open
common.previous
common.retry
common.save
common.share
common.skip
common.unknownError
```

### 5.2 导航与任务

```text
nav.all
nav.calendar
nav.focus
nav.groups
nav.matrix
nav.notes
nav.search
nav.settings
nav.today
todo.add
todo.archive
todo.archiveCompleted
todo.clearCompleted
todo.completed
todo.createPlaceholder
todo.details
todo.due
todo.dueAt
todo.group
todo.important
todo.noReminder
todo.overdue
todo.pin
todo.reminder
todo.remainingCount
todo.repeat
todo.unpin
```

### 5.3 日历与四象限

```text
calendar.addToDate
calendar.emptyDate
calendar.nextMonth
calendar.previousMonth
calendar.selectedDate
calendar.today
matrix.fullName
matrix.q1.subtitle
matrix.q1.title
matrix.q2.subtitle
matrix.q2.title
matrix.q3.subtitle
matrix.q3.title
matrix.q4.subtitle
matrix.q4.title
```

### 5.4 便签与附件

```text
note.add
note.attachments
note.color
note.contentPlaceholder
note.empty
note.filesCount
note.imagesCount
note.pin
note.titlePlaceholder
note.unpin
attachment.addFile
attachment.addImage
attachment.delete
attachment.download
attachment.failed
attachment.manage
attachment.needsDownload
attachment.open
attachment.retry
attachment.save
attachment.share
attachment.synced
```

### 5.5 设置与同步

```text
settings.language
settings.languageEnglish
settings.languageRestartBody
settings.languageRestartTitle
settings.languageSimplifiedChinese
settings.languageSystem
settings.theme
settings.themeDark
settings.themeLight
settings.themeSystem
settings.title
sync.failed
sync.manual
sync.notSynced
sync.settings
sync.synced
sync.syncing
sync.testConnection
```

### 5.6 专注与系统形态

```text
focus.addFiveMinutes
focus.break
focus.completed
focus.currentTarget
focus.end
focus.pause
focus.paused
focus.resume
focus.skip
focus.start
focus.title
liveView.break
liveView.completed
liveView.focus
liveView.paused
notification.focusFinished
notification.reminderTitle
widget.empty
widget.incompleteCount
widget.todayCount
```

### 5.7 错误

```text
error.attachmentDownloadFailed
error.databaseInitializationFailed
error.liveViewUpdateFailed
error.reminderPermissionRequired
error.syncCredentialsInvalid
error.syncNetwork
error.syncTemporary
```

## 6. 参数与复数

共享契约使用命名参数描述语义，例如 `{count}`、`{time}`、`{name}`。桌面端可以直接使用命名参数；HarmonyOS 资源实现可以转换为平台支持的位置参数，但参数含义和顺序必须写入测试。

至少覆盖：

| 键 | 参数 | 中文示例 | English 示例 |
| --- | --- | --- | --- |
| `todo.remainingCount` | `count` | 还有 3 件事 | 3 tasks left |
| `note.imagesCount` | `count` | 3 张图片 | 3 images |
| `note.filesCount` | `count` | 1 个文件 | 1 file |
| `focus.currentTarget` | `title` | 正在专注：写报告 | Focusing on: Write report |
| `todo.dueAt` | `time` | 到期 18:00 | Due 6:00 PM |

英语必须区分单复数。中文使用同一模板即可，但仍由统一格式化器处理。

## 7. 日期、时间、数字和文件大小

- 时间戳、日期字段和业务时区语义保持不变，语言切换只改变显示。
- 日期、月份、星期和相对时间使用平台 `Intl` 或 HarmonyOS i18n API。
- 时间显示尊重平台可用的 12/24 小时设置；测试比较语义，不依赖固定标点。
- 数字使用当前语言的分组符和小数符。
- 文件大小使用 `B`、`KiB`、`MiB`、`GiB`，数值按当前语言格式化。
- 国际化第一阶段不改变两端现有日历周起始、任务排序和“今天”边界。
- 不手工拼接“年、月、日、分钟前、件事、张图片、个文件”等单位。

## 8. 永不翻译的值

- 用户输入的任务、便签、分组和附件名称。
- S3 / MinIO、ETag、UUID、JSON、PDF、ZIP 等技术名称。
- Object Key 和本地文件路径。
- `daily`、`weekly`、`monthly` 等重复枚举。
- `all`、`today`、`matrix`、`calendar`、`notes` 等视图枚举。
- `system`、`light`、`dark` 等主题枚举。
- `focus`、`break`、`paused`、`completed` 等状态枚举。
- `synced`、`pending_upload`、`remote_only` 等同步状态。
- JSON、数据库和备份格式中的字段名。

## 9. 双语快捷新增语法

完整测试数据见 [quick-add-i18n-v1.json](fixtures/quick-add-i18n-v1.json)。v1 规则：

- 中文和英文语法在两种界面语言下始终同时启用。
- `今天/today`、`明天/tomorrow` 和星期词表示到期日期。
- `提醒/remind` 后的时间表示提醒时间；必须已有到期日期。
- `每天/daily` 设置 `repeat_rule = daily`；必须已有到期日期。
- `!` 或 `！` 作为标题开头的重要标记。
- `#分组名` 按用户现有分组精确匹配，分组名不翻译。
- 英文关键字大小写不敏感并检查单词边界。
- 解析后没有实际标题时保留原始输入，并且不应用解析结果。
- 未知词保持在标题中，不能静默删除。

## 10. 质量检查清单

### 10.1 缺失键

- 中文和英文字典/资源键集合必须一致。
- 占位符集合必须一致。
- 开发构建发现缺失键时失败；生产回退不能显示空白。

### 10.2 硬编码扫描

以下用户可见位置不得直接写中文或英文句子：

- 页面、组件、按钮、菜单和对话框。
- `aria-label`、`title`、`accessibilityText`。
- 托盘、通知、桌面卡片、实况窗和锁屏。
- 面向用户的错误和恢复建议。

允许白名单：测试输入、同步 fixture、用户示例数据、开发日志和本契约术语表。

### 10.3 布局回归

- 使用英语和 130% 长度伪文本检查按钮、胶囊、标题和空状态。
- 不通过缩小字号解决英文溢出。
- 关键操作不得截断为省略号；次要图标必须有完整提示。
- 同时检查亮色、暗色和系统跟随主题的文字对比度。
- 桌面检查默认/窄窗口；鸿蒙检查手机、MatePad Mini 和大平板横竖屏。

## 11. 变更流程

1. 先更新本契约和 fixture。
2. 在两个仓库复制相同内容并比较 SHA-256。
3. 再更新平台字典或资源文件。
4. 运行键集合、占位符和快捷新增测试。
5. 完成对应平台的布局和系统界面回归。

