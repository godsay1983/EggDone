# 工作回顾与日程创建待办验收记录

- 日期：2026-10-02。
- WR-1：双端实现完成；本轮自动化与手机模拟器冒烟通过，用户/真机联合验收待 RC-1。
- CT-1：双端实现及自动化/原生数据库测试通过；用户已确认测试通过并授权提交，未正式发布。
- WR-1 已提交：桌面 `b512301`，鸿蒙 `6dcee30`；CT-1 本次按用户指令提交，具体提交号以 Git 记录为准。
- 版本仍为桌面 1.5.0、鸿蒙 1.6.0 / 1000036；schema 27、备份 v9、进展与日历共享 v1 不变。
- 未推送、发布或安装真机。模拟器覆盖安装保留数据；专项数据库测试使用新建隔离库并在结束后删除该测试库。

## 可见功能

两端从“更多 → 工作回顾”进入，默认本周，支持今天/本周/上周/自定义日期、全部/未分组/具体当前分组、标题或进展正文关键词。显示涉及任务数与进展数，30 条稳定分页，支持展开正文、打开已有进展面板并返回，以及复制完整匹配集合。

按记录创建日期归属自然日，编辑不移动日期。已完成和归档任务可见；归档进展面板沿用只读保护。回收站任务、记录墓碑、永久删除终态及缺失父任务排除，已删除或缺失分组归为未分组。关键词是字面子串：ASCII 大小写不敏感，其他字符精确匹配，不把百分号、下划线或反斜杠当通配符。

复制使用新的完整只读快照，不只复制已加载页。包含标题、日期范围、分组和计数，按日期/任务/记录整理，保留正文换行。100000 UTF-16 code unit 上限包括标题与缩进，超限明确失败，不静默截断。复制前校验数据及同步目标，筛选/目标变化丢弃尚未交付的旧内容；操作系统已接收的剪贴板写入无法撤回，变化后不再报告旧操作成功。

## 实现边界

- 桌面查询和计数在 `spawn_blocking` 中执行，使用参数绑定、只读事务、查询绑定游标与快照 token；不逐任务查询。
- 鸿蒙复用原生 RDB 的 DEFERRED 事务，所有 ResultSet 在成功/失败路径关闭，事务不跨剪贴板操作。原生探针与测试确认 `lower()` 仅折叠 ASCII。
- token 结合进展 revision/generation、同步目标 epoch 与实际匹配父任务值，涵盖正文更新、改名、移组、归档、删除及目标变化；不靠同毫秒更新时间推断一致性。
- 两端都复用现有进展、任务、分组和同步数据，没有新增同步域、权限、数据库索引或迁移。
- 两端使用共同 `docs/fixtures/work-review-v1.json`，字节哈希相同；复制摘要的中英文期望、关键词和 DST 边界共享。
- 桌面使用既有浏览器剪贴板入口；鸿蒙使用系统 pasteboard。错误仅固定类别，不记录进展正文或凭据。

## 桌面验证

工作目录 `D:\Develop\EggDone`：

| 命令 | 本轮结果 |
| --- | --- |
| `pnpm test` | 71 文件、595 测试通过；WR 专项 24 项 |
| `pnpm check` | 0 errors / 0 warnings |
| `pnpm i18n:check` | 通过 |
| `pnpm views:check` | Shanghai 30、UTC 29、New York 30 个 fixture 通过 |
| `pnpm capture:check` | 12 个 fixture 通过 |
| `pnpm build` | 通过；保留既有大 chunk 提示 |
| `cargo fmt -- --check` | 通过 |
| `cargo check --offline` | 通过；既有未使用代码警告未顺手清理 |
| `cargo test --offline --lib -- --nocapture` | 550 通过、0 失败、44 个既有忽略 |
| `cargo test --offline --lib work_review::tests -- --nocapture` | 最终查询版本 9 项通过 |
| `node scripts/test-work-review-ui.mjs` | 24 布局、12 行为、2 DST 场景通过；浏览器错误为空 |
| `pnpm tauri build --debug --no-bundle` | 通过，生成包含 WR-1 的 Windows 测试程序 |

UI harness 挂载生产 Svelte 组件，使用隔离 IPC/合成数据/模拟剪贴板，不读用户数据库。覆盖中英文、深浅主题、320/480/1100 宽度和 100%/150% 缩放，分页、全量复制、展开、归档只读面板、返回与滚动保持、变更重载、剪贴板拒绝、筛选/目标竞态、空态和无效范围。

截图目录：`%TEMP%\eggdone-review-ui-1790914550121`。本轮已查看中文深色常规尺寸与英文浅色窄屏大字截图。原生 Windows WebView/真实剪贴板/托盘到回顾的用户操作仍需用户验收，不能以 IPC 模拟视为证明。

## 鸿蒙验证

工作目录 `D:\Develop\EggDoneHarmony`：

| 命令/用例 | 本轮结果 |
| --- | --- |
| `node scripts/test-work-review-rules.cjs` | 共享中英摘要、日期、DST 23/25 小时、字面关键词、UTF-16 上限和同毫秒排序通过 |
| `node scripts/test-work-review-repository.cjs` | 6 项真实 SQLite 宿主仓库测试通过 |
| `node scripts/test-work-review-store.cjs` | 12 项 store 测试通过 |
| `node scripts/test-work-review-ui.cjs` | UI 接线、30 个回顾资源键及占位符三语言对齐、主题 Select、原面板复用和复制状态绑定通过 |
| 既有进展专项回归 | 协议 116 向量、存储 26 项、store/界面/备份回归通过；见下列命令，不作为真实网络验收 |
| `test-task-progress-cross-client.cjs --rust-test-binary=...` | Harmony → Rust → Harmony 隔离 JSON 交换通过；没有请求 S3 |
| `devecocli build --product default --modules entry@default --build-mode debug` | 最终主 HAP 构建通过、无 ArkTS 错误；仍有既有弃用/异常处理提示 |
| `devecocli build --product default --modules entry@ohosTest --build-mode debug` | 测试 HAP 构建通过 |
| `WorkReviewNative` | 手机模拟器 2 项通过，Hypium 2/2、Failure 0、Error 0，经报告校验器确认 |
| `scripts/check-i18n-resources.ps1` | 未通过：既有 zh_CN 缺少 workflow/archive 键；与 HEAD 比较缺项完全相同，本轮未增加缺项 |

全局国际化检查的历史缺项不作为通过项，也没有放宽检查器。新增 `review_*` 文案单独严格比较全部键名与占位符，三语言均通过；全局资源补齐留待发布前处理。

既有回归命令为 `node scripts/test-task-progress-{protocol,storage,store,ui,backup}.cjs`（花括号表示五个独立脚本，不是 PowerShell 可直接展开的命令）。跨端脚本使用本轮编译的 `D:\Develop\EggDone\src-tauri\target\debug\deps\eggdone_lib-5b77e58dfd73106c.exe`；文件名是本机测试产物，重新编译后应选择实际生成的测试二进制。

原生测试设备：Mate 80 Pro Max 模拟器、HarmonyOS 7.0.0 (26.0.0)，`127.0.0.1:5555`。使用 `install -r` 顺序安装主/测试 HAP，未使用会卸载应用的 Hvigor `onDeviceTest`。

原生覆盖 ASCII/Unicode/百分号等字面搜索、30/6 分页、36 记录/2 任务快照、归档纳入、回收站/孤立记录排除、改名后 token/游标失效。全部使用独立测试数据库，不读取系统日历或请求同步。

复现入口（构建、保留安装和严格报告校验）：

```powershell
./scripts/run-device-tests.ps1 -Device '127.0.0.1:5555' -Suite WorkReview
```

本轮报告：`%TEMP%\eggdone-work-review-native-20261002.log`。主模块构建日志：`%TEMP%\eggdone-work-review-main-build-20261002.log`。

手机正常应用冒烟：从更多进入、看到本周记录和正确计数，切换今天得到空态/零计数，切回本周，从记录打开已有进展面板并返回，筛选和列表保持。发现并修复“数据已载入但复制按钮仍禁用”的 ArkUI 状态绑定问题；最终有记录时按钮可点击且为主题主色，空结果时禁用。未点击新增/编辑/删除/共享或主动同步，也未写入真实剪贴板；主应用自身的前台自动同步行为没有关闭。

截图：`%TEMP%\eggdone-work-review-phone-ready-20261002.png`、`eggdone-work-review-phone-empty-20261002.png`、`eggdone-work-review-phone-return-20261002.png`。截图仅留本机，不将已有任务内容作为仓库 fixture。验证结束后关闭本轮启动的模拟器，恢复原先停止状态。

## 查询测量

以下为隔离宿主机 Debug/SQLite 测量，不是手机或网络速度承诺，无新增索引；首次/缓存与并行构建会影响数值。

| 数据量 | 桌面首屏 / 翻页 / 搜索 / 完整快照 | 鸿蒙宿主首屏 / 翻页 / 搜索 / 超限复制拒绝 |
| --- | --- | --- |
| 1000 | 2 / 2 / 6 / 7 ms | 7.6 / 8.1 / 9.2 / 4.9 ms |
| 10000 | 16 / 17 / 66 / 67 ms | 77.6 / 62.8 / 78.1 / 272.4 ms |
| 20000 | 39 / 35 / 167 / 152 ms | 70.4 / 68.3 / 171.5 / 205.6 ms |

桌面测量为单任务短正文；鸿蒙宿主为 100 个父任务、每条约 990 字符，20k 不匹配关键词查询约 194.2 ms。不同样本不能用于两端速度对比。鸿蒙大样本超过 100000 字符，复制按设计明确拒绝；不是成功复制被截断。短样本完整摘要与多页复制另由共同 fixture、store 和桌面 UI 用例验证。真机最大合法同步文档、内存峰值与大字体布局仍待 RC-1。

## 测试程序与后续

- Windows Debug：`D:\Develop\EggDone\src-tauri\target\debug\eggdone.exe`。
- 鸿蒙最终 Debug：`D:\Develop\EggDoneHarmony\EggDone\entry\build\default\outputs\default\entry-default-signed.hap`。
- 构建产物、日志、数据库及截图不提交 Git；本轮未生成 Linux 包或正式发布包。
- 待验收：鸿蒙手机深浅/中英/大字完整矩阵、平板、系统剪贴板与时区切换，桌面原生 WebView，真实双向同步后回顾刷新、离线/错误凭据/冲突，以及既有新旧库/备份跨端恢复。
- WR-1 以上为上一轮验证证据；CT-1 已在本轮接通双端界面、保存重试、立即刷新和既有同步，具体证据及未验收范围见下文。

## CT-1 本轮增量（2026-10-02）

WR-1 已按用户指令提交：桌面 `b512301`、鸿蒙 `6dcee30`。随后完成 CT-1 双端实现与以下自动化验证，用户已报告测试通过，本次提交该增量。用户未逐项列出设备和网络测试范围，未覆盖的专项验证仍保留；不能将下列宿主交换当成实际 S3 同步验收。

### 功能和实现

- 两端只在实际可展示日程的展开详情中提供“创建待办”；鸿蒙操作按钮在整行 Button 外。空态、权限失效、来源撤回及目标切换的隐藏缓存不能打开新草稿。
- 表单只有标题、备注和分组，默认未分组。摘要是打开时的纯文本副本，含原始完整起止范围、非空地点及可用日历名称，不保存内部日历/日程 ID 或来源设备标识。取消不写入，源日程变化不替换已编辑草稿。
- 定时摘要使用当前设备时区，鸿蒙直接读取 `i18n.getTimeZone()`，不依赖旧 Intl 默认时区缓存；全天使用原始自然日期和结束日期前一天。共同 fixture 覆盖跨日、全天多日、重复实例、未知来源时区、无标题、长标题和长 emoji。
- 标题沿用 100 UTF-16 上限，备注 1000 UTF-16 上限，控制字符/无效文本明确拒绝，超限要求编辑，不静默截断。新入口更严格的校验不改变其他创建 API。
- 首次保存原子写入普通任务元数据；同草稿 UUID 只生成一次。返回丢失先解析已提交结果，返回当前任务但不覆盖后续修改。删除、归档和永久删除 UUID 明确不可用，不复活。重新打开是新草稿，可创建第二个独立跟进。
- 落库确认后立即更新任务列表/计数，走原自动同步和卡片管线；刷新失败不当作保存失败，不产生第二条任务。“查看任务”复用既有定位/详情路径，不暗改日历日期筛选。
- 默认没有截止日期、提醒、重复、今日计划、进展或检查清单，不运行自然语言新增解析。鸿蒙从本机日程创建不要求开启日历共享；任务继续独立遵守普通任务同步设置。
- 版本/schema/备份/wire/权限均未改动。未推送、发布、安装真机或操作真实云端测试数据。

### 本轮验证

| 平台/命令 | 结果与边界 |
| --- | --- |
| 桌面 `pnpm test` | 74 文件、617 项通过 |
| 桌面 `pnpm check` | 0 errors / 0 warnings |
| 桌面 `pnpm i18n:check` | 1178 个键对齐 |
| 桌面 `pnpm views:check` / `pnpm capture:check` | 89 个视图 fixture / 12 个 capture fixture 通过 |
| 桌面 `cargo test --offline --lib --quiet` | 最终 558 项通过、45 项忽略；新增 CT 普通测试 8 项 |
| 桌面 `cargo fmt --all` | 已执行；最终格式检查与差异检查通过 |
| 桌面 `node scripts/test-calendar-todo-ui.mjs` | 生产组件隔离 IPC：24 组布局，取消、选择分组、重复点击、返回丢失、重试、撤回、导航通过，浏览器错误为空 |
| 桌面 `pnpm tauri build --debug --no-bundle` | 成功生成包含 WR-1/CT-1 的 Windows Debug；既有大 chunk 和 7 个 Rust 未使用代码警告保留 |
| 鸿蒙 `test-calendar-todo-draft.cjs` | 14 个共享中英用例及当前时区/DST、隐私、全天日期、控制字符边界通过 |
| 鸿蒙 `test-calendar-todo-repository.cjs` | 6 组隔离真实 SQLite 宿主仓库用例通过 |
| 鸿蒙 `test-calendar-todo-store.cjs` | 6 组草稿冻结、重试、连点、终态与保存后失败用例通过 |
| 鸿蒙 `test-calendar-todo-ui.cjs` | 3 组生产 handler、状态绑定、入口、键盘恢复、正常同步接线和三语言资源检查通过；不是原生视觉验收 |
| 鸿蒙 `test-calendar-todo-cross-client.cjs --rust-test-binary=...` | 实际 Harmony 仓库创建 → 生产任务文档 → Rust 合并/创建 → Harmony 合并，通过；标题、备注、分组和普通默认值相同。明确执行原先忽略的 Rust 桥接测试，不请求 S3 |
| 鸿蒙既有日历/备份回归 | `test-system-calendar.cjs`、`test-calendar-share-protocol.cjs`（12 组）、`test-task-progress-backup.cjs`（8 组）通过；进展 UI/工作回顾 UI 也通过 |
| 鸿蒙主/测试 Debug 构建 | 两次 `devecocli build` 均通过，最终主包 13:36:47，测试包 13:38:19，无后续生产源码变更 |
| 鸿蒙模拟器 `CalendarTodoNative` | 4/4，Failure 0 / Error 0，严格报告校验通过 |
| 两端 `git diff --check` | 通过；仅 Git 既有 LF/CRLF 提示 |

共同 fixture：两端 `docs/fixtures/calendar-todo-v1.json` 的 SHA256 均为 `209a493b2827bd994e1508ad94b0233f5255471b4636271120ab697a031c8696`。国际化新增键单独对齐通过；WR-1 已记录的鸿蒙全局旧资源缺项仍未作为通过项或在本轮放宽。

桌面截图目录：`%TEMP%\eggdone-calendar-todo-ui-1790919187421`；已查看中文深色和英文浅色表单，标题、输入文字、备注与底部按钮可读。覆盖中英、深浅、320/480/1100 宽和 100%/150% 缩放，属于浏览器生产组件验证，Windows 原生 WebView/托盘入口待用户验收。

鸿蒙原生报告：`%TEMP%\eggdone-calendar-todo-native-20261002.log`；构建日志：`%TEMP%\eggdone-ct-main-final.log` 和 `eggdone-ct-test-final.log`。使用 Mate 80 Pro Max / HarmonyOS 7.0.0 (26.0.0) / `127.0.0.1:5555`，顺序 `install -r` 保留安装主/测试包，未卸载或清数据。原生用例只创建并删除隔离测试数据库，不读写系统日历或真实任务。

手机正常应用检查：进入日历、切换到 9 月 20 日、滚动查看当前共享日程空态，确认没有无日程创建按钮；返回全部。当前模拟器没有该日期的可见日程，因此没有宣称在实际 ArkUI 创建表单中完成输入/保存/取消验收，也没有为取得画面向用户日历或数据库注入事件。主应用自身前台自动同步保持原逻辑，没有主动点击同步或共享。截图 `%TEMP%\eggdone-calendar-todo-phone-empty-20261002.png` 仅留本机，结束后关闭本轮启动的模拟器。

### 后续边界

两项功能的代码与测试程序已具备。下一阶段 RC-1：用户检查双端实际日程创建/查看、工作回顾和完整复制；再用明确隔离同步目标验证真实双向网络、离线恢复和冲突，集中完成鸿蒙手机/平板/真机及键盘、大字体、主题、系统时区与剪贴板验收。未实测项不能记为完成；无需再拆一次后端或界面开发。

测试程序路径仍为上文 Windows Debug 与鸿蒙签名 Debug HAP，本轮均已更新。没有生成 Linux 发布包或自动升版。

## 用户确认与提交（2026-10-02）

用户反馈：“提交，测试通过了，下一步做什么”。记录为日程创建待办核心用户测试通过，提交双端功能、专项测试与文档。不将这条反馈扩大为全部真机、平板、大字体、离线/冲突、剪贴板或备份场景已逐项验收；前述未覆盖项和鸿蒙全局旧国际化缺项继续保留。两项业务功能开发已经完成，下一步建议发布前收尾，未新增功能、升版、推送或发布。
