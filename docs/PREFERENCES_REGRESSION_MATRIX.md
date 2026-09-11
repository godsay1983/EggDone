# E6d 偏好与系统能力验证矩阵

更新：2026-09-12。E6c已本地提交：桌面 `ee67a27`，鸿蒙 `a303b98`。E6d仅增加回归测试与验证记录，现按用户要求本地提交后继续E7/L1。E6b1～E6b3、E6c与E6d完整原生验收仍待确认，随开发继续保留，不改为通过；不推送或升版。下方“本轮未提交”为提交前的验证记录。

## 本轮自动化结果

| 范围 | 结果 | 证据边界 |
| --- | --- | --- |
| 桌面全量前端单测 | 26文件、266项通过 | 纯逻辑及替身接口，非真实操作系统 |
| 桌面原生偏好 | 7项通过 | 使用rusqlite；包含窗口、两组快捷键、通用偏好的文件数据库关闭重开 |
| 桌面系统状态界面 | 8组通过 | 中英文、亮暗、380/820px；只读刷新、未知状态、注册重试；系统API为替身 |
| 桌面窗口设置 | 16组通过 | 语言、主题、宽度、缩放、DPI、回滚及监听释放；原生窗口为替身 |
| 桌面偏好故障界面 | 6组通过 | 主页面、专注和设置的读写失败，不吞错误或覆盖旧值 |
| 桌面固定筛选 | 12组通过 | 中英文、亮暗、380/640/1100px、125%缩放、上限与失败重试 |
| 鸿蒙磁盘重启 | 3个独立Node进程通过 | 执行生产AppSettingsRepository的SQL，底层是Node SQLite适配器，不等于原生RDB |
| 鸿蒙既有回归 | 7个脚本通过 | 固定筛选、偏好操作、设置仓库、通知能力、任务视图、上下文、便签导航 |

桌面新增测试使用隔离临时文件数据库，写入全部10个通用偏好，验证再次迁移不覆盖、设备标识保留、只读失败后旧值保留、取消固定不清除当前筛选。既有窗口/快捷键磁盘测试也重新执行。

鸿蒙新增脚本在三个独立进程中完成写入、重启重读、失败重试后的再次重读；覆盖主题、语言、提醒声音/时长、专注/休息时长、任务视图、当前筛选及固定列表。执行仓库真实参数化SQL并检查结果集关闭；损坏/未来版本数据不允许被默认值覆盖。测试文件限于临时目录，不连接用户数据库。

首次新增测试运行暴露测试自身的Rust类型推断、Node适配器缺失ArkData常量绑定，已修正并复跑通过；没有修改生产逻辑。Rust保留既有 `TraySnapshot.locale` 未使用警告，`cargo fmt -- --check` 通过。

本轮未修改ArkTS生产源码、资源或包配置，因此沿用E6c已构建并覆盖安装的Debug包，不把此前构建写成本轮重新构建。

## 模拟器验证

本轮连接的是手机模拟器 `127.0.0.1:5557`（1320×2848）与平板模拟器 `127.0.0.1:5555`（2880×1920），不是真机。

手机已完成：在智能列表固定“无日期”，确认首页入口出现；使用 `aa force-stop com.eggdone.todo` 停止进程，再用 `aa start` 启动，入口仍保留。未卸载、清库或新建业务数据。本项验证应用进程重启，不代表设备重启、升级、通知送达或所有偏好完整验收。

平板已完成同样的“固定无日期 → 强制结束进程 → 启动 → 首页入口保留”验证。两台模拟器测试前均为0个固定项，结束后均已取消临时固定，控件树确认恢复0/2，当前任务筛选未被固定操作修改，任务仍为3项。手机弹层已关闭、平板智能列表已收起。本轮未更改通知权限、提醒声音、快捷键、开机启动或同步配置。

中文暗色控件树用于确认操作与入口，不等于完整视觉矩阵。桌面本轮浏览器截图保存在临时目录：系统状态 `eggdone-capability-SKwnHy`，固定筛选 `eggdone-pins-viVVXm`。这些截图及系统API替身不能作为原生Tauri或真机通知验收证明。

## 可复跑命令

桌面仓库根目录：

```powershell
pnpm exec vitest run --maxWorkers=2
cargo test --manifest-path src-tauri/Cargo.toml preferences::tests --lib
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
$env:PLAYWRIGHT_PATH='C:/Users/caozhipeng/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
$env:BROWSER_CHANNEL='msedge'
node scripts/check-system-capabilities.mjs
node scripts/test-window-ui.mjs
node scripts/check-preference-failures.mjs
node scripts/check-pinned-smart-views.mjs
```

Playwright路径为本机依赖路径，换机器时重新确认；脚本使用隔离浏览器和临时服务，不替换用户运行的桌面程序。

鸿蒙仓库根目录，磁盘脚本需要支持 `node:sqlite` 的Node（本机24.20.0）及DevEco TypeScript；后者可由 `TYPESCRIPT_PATH` 指定：

```powershell
node scripts/test-preference-disk-restart.cjs
node scripts/test-pinned-smart-views.cjs
node scripts/test-preference-operations.cjs
node scripts/test-app-preferences.cjs
node scripts/test-notification-capability.cjs
node scripts/test-task-view-preferences.cjs
node scripts/test-navigation-context.cjs
node scripts/test-note-task-navigation.cjs
```

## 尚待人工/原生验收

| 编号 | 操作 | 通过标准 |
| --- | --- | --- |
| D1 | 桌面设置语言、主题、默认视图、分组、专注时长、窗口大小与缩放，固定两个筛选；从托盘退出后重开 | 已确认偏好保留，默认视图与智能筛选优先级符合原方案；关闭窗口隐藏不等于退出进程 |
| D2 | 分别验证两组快捷键，使用其他程序占用后启动，再释放并点击重试注册 | 保存的启用意愿保留，注册失败明确反馈；重试后快捷键实际有效 |
| D3 | 系统中更改开机启动，返回设置刷新；测试结束恢复原设置 | 显示系统实际状态，不因只读刷新强制重注册或重新启用 |
| H1 | 真机修改语言、主题、默认视图、分组、提醒声音、专注时长与固定筛选，然后退出重开 | 全部已确认偏好保留；取消固定不清除当前筛选，不影响另一设备的固定列表 |
| H2 | 真机允许/禁止通知，返回应用刷新，再创建测试提醒 | 权限摘要与系统一致；声音偏好独立；分别验收通知触发、声音、锁屏和点击 |
| X1 | 桌面原生窄/宽窗口与150%缩放，鸿蒙小平板、分屏、横竖屏及大字体，中英文亮暗 | 文字完整、按钮可操作、无重叠；不以浏览器替身或标准模拟器替代整个矩阵 |

不要为了验证清空用户数据。权限/启动项/快捷键修改前记录原值，结束后恢复；测试提醒确认后删除。新库、旧库升级、Linux桌面环境与设备重启的完整组合未在本轮执行。

## 阶段结论

E6d自动化补强与本轮手机/平板模拟器固定入口重启检查已完成，尚不能勾选“完整系统/设备验收通过”。本轮测试与文档增量未提交。E6b/E6c待确认项保持原状态，不将提交视为验收。下一步确认上述原生矩阵；E7任务与便签关联仍未开始。用户若决定暂缓人工验收并继续开发，应单独记录延期范围。
