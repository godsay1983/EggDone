# EggDone 桌面端国际化实现方案

## 1. 文档目标

本文档定义 EggDone 桌面端简体中文和英语界面的产品边界、语言状态、Svelte 与 Rust 本地化架构、系统界面适配和验证要求。开发顺序见 [INTERNATIONALIZATION_ROADMAP.md](INTERNATIONALIZATION_ROADMAP.md)。鸿蒙端对应方案见 `D:\Develop\EggDoneHarmony\docs\HARMONY_INTERNATIONALIZATION_IMPLEMENTATION_PLAN.md`。

两端共享术语、语义键和用户可见行为，但分别使用平台原生实现。国际化只改变界面和系统文案，不改变 Todo、便签、附件和 S3 同步协议。

## 2. 产品目标与边界

### 2.1 第一阶段目标

- 支持“跟随系统”“简体中文”“English”三种语言模式。
- 默认跟随系统；系统为中文时显示简体中文，其他语言暂时回退英语。
- 用户切换语言后，主窗口和专注窗口立即刷新。
- 托盘菜单、系统提醒、窗口标题、无障碍文案和错误提示与当前语言一致。
- 日期、时间、相对时间、数量和文件大小按当前区域格式显示。
- 中英文快捷新增语法在两种界面语言下都可使用。
- 语言偏好只保存在当前设备，不通过 S3 同步。

### 2.2 不翻译的内容

- 用户输入的任务标题、任务备注、便签正文和便签标题。
- 用户创建的分组名称、附件名称和对象存储配置。
- 从另一端同步过来的用户数据。
- S3 Object Key、JSON 字段、数据库字段和内部枚举值。

### 2.3 暂不包含

- 简体中文和英语以外的第三种语言。
- 在线翻译、自动翻译用户内容和语言包热下载。
- 按账户同步语言偏好。
- 根据语言修改既有同步协议或数据库业务数据。

## 3. 核心设计决策

### 3.1 使用轻量、强类型的前端本地化层

当前桌面端只有两种语言，不引入大型国际化框架。新增小型 TypeScript 本地化模块，提供：

- 强类型语义键。
- 参数插值和复数处理。
- 当前语言与解析后语言的 Svelte store。
- 标准日期、时间、相对时间和数字格式化器。
- 开发环境缺失键告警和中文硬编码检查。

建议目录：

```text
src/lib/i18n/
├─ index.ts
├─ types.ts
├─ languageStore.ts
├─ formatters.ts
└─ locales/
   ├─ zh-CN.ts
   └─ en-US.ts
```

### 3.2 共享语义，不共享运行时实现

桌面端和鸿蒙端使用相同的语义键命名和术语表，例如：

```text
common.cancel
common.delete
nav.all
nav.today
todo.dueAt
note.attachments.count
focus.phase.work
sync.status.synced
error.sync.network
```

桌面端使用 TypeScript/Rust 字典，鸿蒙端使用资源文件。两端不共享生成代码，也不为了共用语言包引入新的构建工具。

### 3.3 语言偏好是本机设置

语言模式使用稳定值：

```ts
type LanguageMode = "system" | "zh-CN" | "en-US";
```

偏好保存在本机 `localStorage`，建议 Key 为 `eggdone-language`。不得加入 `todos.json`、`notes.json`、同步设置或完整业务数据合并流程。完整备份可以记录此值为可选设备设置，但导入时必须让用户决定是否覆盖本机语言。

### 3.4 内部值保持稳定

界面只翻译显示文本，不翻译内部值：

| 领域 | 稳定内部值示例 | 中文显示 | 英文显示 |
| --- | --- | --- | --- |
| 重复 | `daily` | 每天 | Daily |
| 视图 | `matrix` | 四象限 | Matrix |
| 主题 | `system` | 跟随系统 | System |
| 同步 | `synced` | 已同步 | Synced |
| 专注阶段 | `focus` | 专注 | Focus |

这样不会影响数据库、测试 fixture、跨端同步和冲突决胜。

## 4. 语言解析与生命周期

### 4.1 启动流程

1. 读取 `eggdone-language`，无值时使用 `system`。
2. `system` 模式读取 `navigator.languages` 和 `navigator.language`。
3. 语言标签以 `zh` 开头时解析为 `zh-CN`，其他语言解析为 `en-US`。
4. 更新 `<html lang>`、Svelte store 和格式化器。
5. 将解析后语言发送给 Rust 运行时。
6. 初始化主窗口、专注窗口、托盘和系统提醒。

### 4.2 运行时切换

- 设置页使用三段式选择控件展示三种语言模式。
- 选择后立即保存并刷新 Svelte 组件，不要求重启应用。
- 主窗口向专注窗口广播语言变化，避免两个 Webview 文案不一致。
- 重建托盘菜单并更新托盘提示、窗口标题和后续通知文案。
- `system` 模式监听浏览器 `languagechange` 事件；系统语言变化后重新解析。

### 4.3 前端状态模型

```ts
interface LanguageState {
  mode: LanguageMode;
  resolvedLocale: "zh-CN" | "en-US";
}
```

组件只能通过 `t()` 和格式化器获取用户可见文案。不得在组件中根据语言写散落的三元表达式。

## 5. TypeScript 本地化层

### 5.1 字典与类型

- `zh-CN.ts` 作为中文事实来源，`en-US.ts` 必须具有完全相同的键。
- 构建测试比较两份字典的键集合。
- `t(key, params)` 对缺失参数、未知键和未替换占位符给出开发期错误。
- 生产环境缺失键回退英语，再回退语义键，不能显示空字符串。

示例：

```ts
t("todo.remainingCount", { count: 3 });
t("note.attachments.count", { imageCount: 2, fileCount: 1 });
```

英文复数不能通过简单拼接 `s` 处理，应使用 `Intl.PluralRules` 或显式复数分支。

### 5.2 格式化器

统一提供：

- `formatDate`：日历、到期日、便签更新时间。
- `formatTime`：提醒和专注结束时间。
- `formatDateTime`：详情页和错误记录。
- `formatRelativeTime`：刚刚、分钟前、小时前。
- `formatNumber`：任务数、进度和容量。
- `formatFileSize`：附件大小和缓存容量。

组件不得硬编码 `zh-CN`，也不得手工拼接“年/月/日”“分钟前”“张图片”等单位。

## 6. Rust 本地化层

### 6.1 运行时语言状态

新增窄职责模块：

```text
src-tauri/src/i18n.rs
```

职责：

- 保存当前解析后语言。
- 提供 Rust 侧语义键和参数格式化。
- 为托盘菜单、窗口标题、通知和提醒提供文本。
- 在语言变化后触发托盘菜单重建或文本更新。

前端通过窄 command 设置解析后语言。Rust 不重复判断系统语言，避免 Webview 与原生界面解析结果不同。

### 6.2 错误协议

当前 Rust 存在大量中文 `Err(String)`。迁移时采用稳定错误码和参数：

```json
{
  "code": "SYNC_NETWORK_ERROR",
  "params": { "detail": "..." }
}
```

前端根据错误码翻译标题和建议；底层 `detail` 只用于诊断，不直接作为整屏红字展示。第一阶段保留旧字符串回退，按同步、数据交换、附件、提醒和专注逐步迁移，不能一次性重写全部 command。

### 6.3 原生界面范围

必须覆盖：

- 托盘菜单、托盘 tooltip 和窗口标题。
- Todo 到期提醒和专注结束提醒。
- 通知动作、错误提示和确认文案。
- 平台特定菜单或权限提示。

## 7. 页面与功能覆盖

### 7.1 主界面

- 顶部标题、同步状态、视图导航和快捷新增。
- 全部、今天、四象限、日历、搜索和分组视图。
- Todo 卡片、编辑器、批量操作、日期时间和重复规则。
- 空状态、加载状态、失败状态和确认对话框。

### 7.2 便签与附件

- 便签列表、编辑器、颜色、置顶、删除和空状态。
- 图片、文件、附件管理、同步状态、打开、保存和重试。
- 文件类型缩写保留行业通用写法，例如 PDF、MD、ZIP。

### 7.3 设置、同步和数据管理

- 语言、主题、启动、快捷键、专注时长和提醒设置。
- S3 配置、连接测试、同步状态和冲突错误。
- 导入、导出、备份、恢复和破坏性操作确认。

### 7.4 专注

- 主窗口专注页和独立专注窗口。
- 专注、暂停、休息、完成四类状态。
- 目标任务、倒计时、控制按钮和系统提醒。

## 8. 快捷新增与搜索

快捷新增语法不能与界面语言绑定。无论当前界面语言是什么，都应识别：

- 中文：`今天`、`明天`、`提醒`、`每天`、`重要`。
- 英文：`today`、`tomorrow`、`remind`、`daily`、`important`。

规则：

- 保持现有中文解析优先级和日期语义不变。
- 英文关键字大小写不敏感，并明确单词边界。
- 无法识别时保留原始文本，不吞掉用户内容。
- 中英文关键字冲突时使用最长、最具体规则。
- 搜索默认对英语大小写不敏感；不对用户内容做机器翻译。

## 9. UI 与无障碍要求

- 英文通常比中文更长，按钮和胶囊必须允许合理扩展或换行。
- 不通过缩小字号解决英文溢出。
- 窄窗口优先使用图标加 tooltip，或把次要操作收入菜单。
- `四象限` 的窄控件英文显示 `Matrix`，完整无障碍名称使用 `Eisenhower Matrix`。
- 所有 `aria-label`、`title`、对话框标题和错误提示必须进入语言包。
- 中英文都要满足亮色、暗色主题的文字对比度要求。
- 品牌中文显示“蛋定 Todo”，英文显示“EggDone”。用户数据中的品牌文本不自动替换。

## 10. 跨端与同步兼容

- 语言偏好不进入 S3，同一账户的桌面端和鸿蒙端可以使用不同语言。
- `todos.json`、`notes.json`、`note-attachments.json` 和 `.eggdone-backup` 的业务字段保持不变。
- `daily`、`weekly`、`matrix`、`system` 等内部枚举保持英文稳定值。
- 同步错误使用稳定错误码；错误码本身不写入用户数据。
- 快捷新增解析结果必须与鸿蒙端共享 fixture，保证中英文输入得到相同字段。

## 11. 测试策略

### 11.1 自动测试

- 中英字典键集合完全一致。
- 插值参数、复数、缺失键和回退行为。
- 系统语言解析和 `languagechange`。
- 日期、相对时间、数字和文件大小格式。
- 中英文快捷新增 fixture。
- Rust 语义键、错误码映射和托盘文案。
- 业务测试不得依赖翻译后的显示文本判断内部状态。

### 11.2 手动回归矩阵

| 环境 | 中文 | English | 跟随系统 |
| --- | --- | --- | --- |
| Windows 主窗口 | 必测 | 必测 | 必测 |
| 独立专注窗口 | 必测 | 必测 | 必测 |
| 托盘和系统通知 | 必测 | 必测 | 必测 |
| 亮色/暗色 | 必测 | 必测 | 必测 |
| 窄窗口/默认窗口 | 必测 | 必测 | 必测 |

重点验证切换语言时正在编辑的 Todo/便签不丢失、专注倒计时不中断、同步任务不重复启动。

## 12. 主要改动位置

```text
src/lib/i18n/
src/lib/components/TodoPanel.svelte
src/lib/components/TodoItem.svelte
src/lib/components/NoteList.svelte
src/lib/components/NoteCard.svelte
src/lib/components/NoteEditor.svelte
src/lib/components/SettingsPanel.svelte
src/lib/components/SyncSettings.svelte
src/lib/components/DataManager.svelte
src/lib/components/FocusWindow.svelte
src/lib/utils/quickAdd.ts
src-tauri/src/i18n.rs
src-tauri/src/tray.rs
src-tauri/src/reminders.rs
src-tauri/src/commands.rs
```

## 13. 完成标准

- 用户可以稳定选择跟随系统、简体中文或英语。
- 主窗口、专注窗口、托盘、通知和错误提示无中英混杂。
- 英文界面在默认和窄窗口中无裁切、重叠或不可操作控件。
- 用户数据和同步协议完全兼容旧版本。
- 双端快捷新增 fixture 通过。
- 全部前端、Rust 自动测试和 Windows 手动回归通过。

