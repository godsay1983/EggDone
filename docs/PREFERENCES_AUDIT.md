# E6a 双端偏好与系统能力状态审计

2026-09-12后续：D01/H04已在E6b2实现，见[持久化契约与验收](PREFERENCE_PERSISTENCE.md)；H03剩余权限摘要及D03/D04已在E6b3实现，见[系统能力状态](SYSTEM_CAPABILITY_STATUS.md)。自动化及有限运行证据与用户验收分别记录，未确认项目保持待验收。以下表格保留审计基线。

最新修复进度：E6b1已修复H01/H02/D02，并分离H03中的保存/应用失败重试；详情及验收见[偏好失败保护](PREFERENCE_FAILURE_FIX.md)。原5项OPEN探针现已改成强制回归并通过。下文为审计时的基线，不代表这些问题仍全部未修复；D01/H04及完整能力状态项仍待后续交付。

日期：2026-09-11。基线：桌面 a29b269，鸿蒙 a7c8341。

## 结论与本轮边界

E5c 已由用户确认验收并提交。E6a 按 roadmap 第一项完成代码审计、存储清单、能力状态契约及可复现检查。本轮只增加测试、诊断脚本和文档，**没有修改业务代码或安装新包**，以下待修复问题不视为已解决。不推送、升版或发布。

优先修复：鸿蒙设置写入失败后静默显示新值；桌面通用偏好仍依赖 WebView 存储且缺少统一异常保护。窗口/缩放、快捷键已有原生持久化，不重复实现。

## 证据等级

- 代码确认：实际读取调用点、保存调用点、失败处理及界面绑定已核对。
- 主机测试：调用生产逻辑，系统/RDB边界替身隔离；不读写真实用户数据库、凭据或系统设置。
- 故障探针：刻意注入写入失败，报告 OPEN 缺口；进程退出成功仅说明探针执行成功，不代表产品正确。
- 未完成：本轮未重新进行真机/原生桌面完整退出重启、系统权限关闭、真实快捷键冲突验收。之前E5c验收不能扩大为E6验收。

## 桌面清单

| 偏好或状态 | 当前持久化与恢复 | 审计判断 |
| --- | --- | --- |
| 窗口宽高、缩放 | SQLite app_metadata/main_window_preferences_v1；启动原生读取，旧localStorage只在原生值缺失时使用；尺寸/缩放写入串行，关闭/退出刷新尺寸 | 已具备；显示器边界可能限制实际大小，不应误判为保存丢失。原生读取失败不覆盖默认值 |
| 主窗口快捷键 | panel_shortcut_preferences_v1；组合与enabled一起保存；原生值缺失时迁移旧值 | 已具备；启动冲突保留启用意愿，错误单独显示 |
| 快速便签快捷键 | note_shortcut_preferences_v1；与主窗口独立 | 已具备；写入失败尝试恢复原注册，实际恢复仍可能被OS拒绝 |
| 开机启动 | 系统autostart插件查询/设置；更新后再次读取OS结果 | 当前没有独立“用户期望”持久化；以OS真实结果为准是合理设计，不在重启时强制重新开启。读取失败需要呈现未知而非简单关闭 |
| 主题 | localStorage/eggdone-theme，首次无选择时参考系统主题；主窗口、启动HTML、专注窗口分别读取 | 普通相同WebView数据目录的重启应保留；迁移/更换profile/清除WebView数据时无法依靠原生库恢复 |
| 界面语言 | localStorage/eggdone-language；system/zh-CN/en-US；storage及languagechange通知；当前语言另传Rust运行态 | 同上；后续迁移须同时照顾多个窗口及系统语言跟随，不能只改设置下拉框 |
| 显示已完成 | localStorage/eggdone-show-completed | 应长期保留；需统一读写异常处理 |
| 默认任务视图、最后任务视图 | eggdone-default-list-view / eggdone-list-view；默认remember，支持all/today/quadrants/calendar | 已有用户选项；便签不写入启动任务视图。后续保持旧语义 |
| 所选分组 | eggdone-selected-group | 应长期保留；恢复时不存在/已删除分组应回退全部，不删除任务 |
| 智能筛选 | eggdone-smart-view（TodoPanel定义）；启动存在智能筛选时任务视图为all | 应长期保留；优先级需保持当前语义，不与下一轮固定入口混同 |
| 专注/休息时长 | eggdone-focus-duration-minutes / eggdone-break-duration-minutes；限定15/25/45与5/10/15 | 应长期保留；迁移默认参数不能改变正在运行的专注状态机 |
| 最近专注目标 | eggdone-focus-target-uuid / eggdone-focus-target-title | 任务上下文，不等于偏好或运行会话；首轮通用偏好迁移不包含它，删除目标后如何清理另行检查 |
| 同步启用、地址及选项 | SQLite sync_settings；AK/SK在系统keyring，凭据存在另有状态 | 已区分开关与是否配置；本轮不搬迁凭据，不改目标切换/同步协议 |
| 搜索词、列表滚动、选中便签、展开菜单 | E5c会话内上下文 | 不要求重启恢复；不混入长期偏好 |
| 减少动画 | 读取系统prefers-reduced-motion | 系统状态，不是新增应用偏好 |

证据入口（桌面工程内路径）：src/lib/components/TodoPanel.svelte、src/lib/api/desktopSettings.ts、src/lib/api/windowPreferencesApi.ts、src/lib/stores/windowPreferences.ts、src/lib/i18n/index.ts、src/lib/utils/focusSettings.ts。

## 鸿蒙清单

| 偏好或状态 | 当前持久化与恢复 | 审计判断 |
| --- | --- | --- |
| 主题、语言 | RDB app_metadata/theme_mode、language_mode；AppSettingsRepository读取；系统应用主题/语言另行应用 | 持久化存在；主题保存失败吞错，界面与库可能不同；语言已有失败回滚及提示，仍需避免并发切换和延迟加载覆盖 |
| 提醒声音、响铃长度 | reminder_sound_enabled、reminder_ring_duration；保存后尝试刷新已注册提醒 | 持久化存在；写失败与提醒刷新失败混在一个catch里静默忽略，用户无法区分“未保存”和“已保存但未应用” |
| 专注/休息默认时长 | focus_duration_minutes、focus_break_duration_minutes | 持久化存在；写失败后内存保留新值。修复只影响默认偏好，不改已审核实况窗及活动计时 |
| 智能筛选 | smart_view；读时修订号保护、写入串行化 | 已具备。保存失败显示错误，但不回滚当前浏览筛选；应标明重启保留未成功 |
| 显示已完成、分组选择 | Index @State showCompleted=true、groupFilter=all；点击后只改状态 | 仅会话保留，与桌面重启语义不一致；建议在E6b加入本机持久化并验证旧库默认 |
| 默认/最后任务视图 | Index viewMode=all；智能筛选恢复可覆盖 | 尚无桌面的默认视图选项；先冻结同一语义，再接入设置。E5c“返回任务”不等于重启记忆 |
| 搜索、列表滚动、便签草稿 | 当前会话状态 | 本阶段不做强杀或重启草稿恢复 |
| 同步启用与地址 | RDB sync_settings；AssetStore保存AK/SK，配置开关与credentialsSaved分开 | 已具备；以两者共同决定自动同步可用。不在本轮改变安全存储、网络或目标隔离 |
| 通知授权 | ReminderService调用系统查询/申请，调度/意图返回能区分permission_required等状态 | 不是reminderSoundEnabled；设置页目前主要显示声音偏好，没有完整的系统授权状态摘要 |
| 评分冷却、活动专注恢复、桌面卡片快照 | 各自Preferences仓库/服务 | 内部持久化，不是用户通用设置，不合并到通用偏好 |
| 设备字号、方向、断点、能力支持 | 系统配置/AppStorage投影 | 实时系统状态，不写入长期偏好覆盖系统 |

证据入口：鸿蒙工程 EggDone/entry/src/main/ets/data/repositories/AppSettingsRepository.ets、pages/Index.ets（loadAppSettings、setThemeMode、setReminderSoundEnabled、setFocusDurationMinutes、saveSmartViewPreference）、services/reminder/ReminderService.ets、store/SyncSettingsStore.ets。

## 待修复清单

| 编号 | 优先级 | 已确认问题或风险 | 修复边界/成功标准 |
| --- | --- | --- | --- |
| H01 | 高 | 5个设置setter写失败被忽略，界面仍显示新值；生产方法故障探针已复现 | 显示明确保存失败与重试；写成功前不宣称持久化完成。串行保存或修订号防旧结果覆盖新选择 |
| H02 | 高 | loadAppSettings任一读取失败便将所有设置静默回退默认；没有区分加载失败和“无旧值” | 保留已知状态并显示加载失败；读失败不写默认覆盖；提供重新读取入口；加载晚到不能覆盖用户新选择 |
| H03 | 中 | 声音偏好写入和已有提醒刷新失败混为一谈；声音开关不是授权状态 | 分开“保存成功/应用失败”；权限查询失败呈现未知，禁止反推关闭偏好 |
| H04 | 中 | 鸿蒙显示已完成/分组/默认视图重启语义与桌面不同 | RDB复用metadata，旧库缺失默认兼容；分组失效回退，不改同步协议 |
| D01 | 中 | 桌面通用偏好依赖WebView存储，尚未享有窗口/快捷键原生持久化 | 复用app_metadata新增版本化非敏感偏好值，迁移前原生读成功且为空；旧存储保留为迁移来源 |
| D02 | 高 | TodoPanel初始化多次直接getItem发生在todos.load之前；存储异常可中断后续初始化；若干写调用缺少错误反馈 | 安全读取单项失败不能阻断任务加载；保存失败不误报成功；浏览器回退和原生读失败分开 |
| D03 | 中 | 开机启动读取失败返回false+错误；设置面板无明确unknown模型；实际状态通常只在启动/修改时查询 | 设置打开时刷新能力状态；错误不等于关闭；不自动重新启用用户在OS中关闭的自启动 |
| D04 | 中 | 快捷键初始原生读失败会让整体初始化reject，不能据此将默认UI解释为真实注册结果 | 加载/未知/已启用但冲突分开；保留原意愿及原有回滚策略，按钮显示当前操作失败 |

H01是故障注入下已复现的逻辑问题；D01是存储边界风险，不能表述为“每次正常重启都会丢失”。本轮不借审计改动实况窗、小艺或同步实现。

## 冻结的实现契约

1. 长期偏好：本机主题、语言、显示已完成、默认视图、最后任务视图、分组、智能筛选、专注默认时长。只控制本机体验，不参加S3合并。
2. 窗口/缩放及快捷键保留既有原生key，不合并重写；凭据永远不进入普通偏好JSON或日志。
3. 桌面通用偏好使用独立版本化metadata值；只在成功读到“无值”时迁移WebView旧值。读取异常或未知更高版本应阻止覆盖，不当作新用户。
4. 鸿蒙复用已有metadata键；新增筛选键必须定义缺失、非法值与已删除分组回退规则。先声明保存成功，再更新持久化状态；预览态可提前应用，但必须可见地区别于已保存态。
5. 用户意愿、持久化结果、系统应用结果分别表达。系统结果按能力使用unknown/available/blocked/error等明确状态；不把授权或注册状态长期保存成永远有效。
6. 初始读取与用户写入必须有就绪/修订保护；快速切换必须顺序可控；失败不能丢后续最新选择。错误文案两端中英文配套。
7. 返回/退出不承担首次保存：用户修改时持久化；隐藏窗口不等于进程退出。已有窗口关闭尺寸刷新维持不变。
8. 默认视图与智能筛选保留当前优先级；不将便签、搜索词、滚动书签扩展成跨重启恢复。
9. 正在专注的计时、系统提醒、实况窗的执行流程不随默认时长设置迁移而改变。

## 本轮自动检查

桌面：pnpm check、pnpm build、cargo fmt -- --check、cargo check通过，Cargo仍有既有TraySnapshot.locale未使用警告；新增3项自启动查询失败/系统回读/操作失败测试，全部23文件246项Vitest通过。主机替身不等于真实OS状态测试。

鸿蒙：test-app-preferences.cjs通过：默认不写库、7项设置新实例读回、智能筛选清除、非法值回退、读失败不写默认、写失败旧值保留、参数绑定。使用RDB替身，未验证真实迁移或进程重启。
audit-preference-failures.cjs报告5项OPEN（H01），未将这些错误行为算成通过的产品回归。本轮业务源码未变化，无新包需要安装。

运行：
- 桌面：pnpm test；必要时pnpm exec vitest run src/lib/api/desktopSettings.test.ts。
- 鸿蒙：node scripts/test-app-preferences.cjs；node scripts/audit-preference-failures.cjs。
- 鸿蒙脚本使用项目既有TypeScript路径，也支持TYPESCRIPT_PATH覆盖。

## 后续交付顺序与验收

- E6a：本审计及契约、测试证据，已完成。用户无需为这一轮重新安装。
- E6b：双端持久化与失败反馈修复，按H01/H02/D02优先；再补D01/H04迁移与能力状态刷新。不得把审计完成等同修复完成。
- E6c：固定1-2个常用筛选与低频入口布局。固定入口和当前选中筛选分别存储，不新增一排首页按钮。
- E6d：双端重启/更新覆盖、权限关闭、快捷键冲突、手机/平板/桌面形态验收及收口。

E6b验收：设置后真正退出进程再启动；覆盖安装保留值（卸载/清除数据不在保留承诺中）；旧值迁移后不再依赖WebView；注入读写失败不误显示成功；快切最终选择一致；OS拒绝/权限关闭后应用提示准确；专注运行中修改默认时长不重置活动计时。完整特殊矩阵逐项记录，不用“测试通过”反推所有系统能力。
