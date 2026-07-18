# EggDone 桌面端国际化 Roadmap

配套设计见 [INTERNATIONALIZATION_IMPLEMENTATION_PLAN.md](INTERNATIONALIZATION_IMPLEMENTATION_PLAN.md)。`DI0` 必须与鸿蒙端 `D:\Develop\EggDoneHarmony\docs\HARMONY_INTERNATIONALIZATION_ROADMAP.md` 的 `HI0` 同步完成。

本 Roadmap 只增加界面语言，不修改 Todo、便签、附件和 S3 同步协议。每次开发按阶段顺序推进，不跨阶段批量替换全部中文。

## DI0：跨端术语与边界冻结

- [x] 固定 `system`、`zh-CN`、`en-US` 三种语言模式。
- [x] 固定中文和英文品牌显示规则。
- [x] 建立双端共享语义键清单和中英术语表。
- [x] 固定日期、时间、数量、文件大小和复数显示规则。
- [x] 固定语言偏好只在本机保存、不进入 S3 的边界。
- [x] 固定不翻译用户内容、Object Key、JSON 字段和内部枚举的规则。
- [x] 建立中英文快捷新增跨端 fixture。
- [x] 建立缺失键、硬编码文案和布局回归检查清单。

交付物：[I18N_SHARED_CONTRACT.md](I18N_SHARED_CONTRACT.md) 和 [quick-add-i18n-v1.json](fixtures/quick-add-i18n-v1.json)。

完成条件：两端相同功能使用相同语义键和术语，不再边开发边决定英文名称。

## DI1：TypeScript 国际化基础层

- [x] 新增 `src/lib/i18n/`、强类型字典和 `t()`。
- [x] 建立 `zh-CN` 与 `en-US` 字典键一致性测试。
- [x] 实现插值、复数、缺失键和回退规则。
- [x] 实现日期、时间、相对时间、数字和文件大小格式化器。
- [x] 实现 `LanguageMode`、解析后语言和 Svelte store。
- [x] 使用 `eggdone-language` 保存本机偏好。
- [x] 监听系统 `languagechange` 并更新跟随系统模式。
- [x] 更新 `<html lang>`，为屏幕阅读器提供正确语言。

本阶段验证：`pnpm check`、58 项单元测试和 `pnpm build` 通过。设置页已提供跟随系统、简体中文和 English 三种模式，桌面端可即时切换并持久化。

完成条件：测试组件可以实时切换中英文，刷新后语言保持正确。

## DI2：主窗口核心链路

- [x] 本地化顶部标题、同步胶囊、视图导航和新增任务区。
- [x] 本地化全部、今天、四象限、日历、搜索和分组视图。
- [x] 本地化 Todo 卡片、更多菜单和任务详情。
- [x] 本地化到期、提醒、重复、重要和分组编辑控件。
- [x] 本地化批量操作、归档、清除完成和确认对话框。
- [x] 替换组件内硬编码 `zh-CN` 日期格式。
- [x] 修复英文按钮、标签、空状态在窄窗口中的溢出。
- [x] 保持当前 Todo 排序、筛选和编辑状态逻辑不变。

本阶段自动验证：强类型中英文字典保持一致，`pnpm check`、单元测试和生产构建通过。手机等宽窄窗口与深浅主题仍需按 DI6 清单做发布前人工回归。

完成条件：用户可只使用英文完成 Todo 的新增、编辑、完成、删除、筛选和日历操作。

## DI3：便签、附件、设置与数据管理

- [x] 本地化便签列表、编辑器、空状态和卡片操作。
- [x] 本地化图片、文件、附件管理、同步状态和错误提示。
- [x] 在设置页增加三段式语言选择控件。
- [x] 本地化主题、启动、快捷键和专注时长设置。
- [x] 本地化 S3 配置、测试连接、手动同步和冲突错误。
- [x] 本地化导入、导出、备份、恢复和危险操作确认。
- [x] 验证切换语言不会提交空草稿或丢失未保存输入。
- [x] 完成亮色、暗色和窄窗口的代码级视觉回归。

本阶段自动验证：数据管理弹窗改为受视口约束的响应式宽高，英文长文案可换行；语言状态在当前组件树内原地更新，不重载便签编辑器。`pnpm check`、59 项单元测试和生产构建通过。完整中英文、亮暗主题和窄窗口截图矩阵继续在 DI6 发布验收执行。

完成条件：便签、附件和设置不存在影响操作的中文硬编码或英文裁切。

## DI4：Rust 原生界面与专注

- [x] 新增 `src-tauri/src/i18n.rs` 和运行时语言状态。
- [x] 增加前端设置解析后语言的窄 Tauri command。
- [x] 本地化托盘菜单、tooltip 和窗口标题。
- [x] 语言切换后安全重建或更新托盘菜单。
- [x] 本地化主窗口专注页和独立专注窗口。
- [x] 本地化专注、暂停、休息、完成状态和目标任务提示。
- [x] 本地化 Todo 提醒和专注结束系统通知。
- [x] 验证语言切换不重置倒计时、不关闭专注窗口。

本阶段自动验证：前端仅把解析后的 `zh-CN` / `en-US` 传入 Rust，运行时语言切换会更新窗口标题、重建托盘菜单并保留专注 tooltip 快照；主窗口与独立专注窗口共用同一翻译目录，用户任务标题原样插入。Rust 109 项测试、Svelte 静态检查、59 项前端测试和生产构建通过。托盘和 Windows 系统通知的最终人工回归继续在 DI6 执行。

完成条件：主窗口、专注窗口、托盘和系统通知使用同一种语言。

## DI5：错误码与双语快捷新增

- [x] 定义同步、数据交换、附件、提醒和专注的稳定错误码。
- [x] 前端按错误码翻译标题、说明和可执行建议。
- [x] 保留旧 `Err(String)` 的安全回退，避免一次性破坏 command 合约。
- [x] 避免把底层错误堆栈直接显示为整屏红字。
- [x] 扩展 `quickAdd.ts` 支持英文日期、提醒、重复和重要标记。
- [x] 保持中文快捷新增现有行为和优先级。
- [x] 运行与鸿蒙端共享的中英文解析 fixture。
- [x] 验证英文搜索大小写不敏感且不改写用户数据。

本阶段自动验证：Rust command 对同步、提醒和专注返回稳定错误码，前端 API 对数据交换和附件补齐同一错误信封，并按当前语言显示简短说明与建议；旧字符串错误只显示首行且限制长度。快捷新增已支持 `today/tomorrow/next weekday`、12/24 小时时间、英文提醒、重复和 `important`，原中文用例保持通过。共享 fixture 已同步到两端仓库，桌面端执行通过，鸿蒙端将在 HI6 接入同一 fixture。Svelte 静态检查、64 项前端测试、生产构建和 110 项 Rust 测试通过。

完成条件：同一中英文快捷输入在两端产生相同业务字段，主要错误可按语言显示。

## DI6：质量门禁与发布准备

- [ ] 增加字典缺失键和多余键自动检查。
- [ ] 增加用户可见中文硬编码扫描白名单。
- [ ] 使用超长伪本地化文本检查布局，不发布伪语言包。
- [ ] 完成 Windows 中文、英文、跟随系统回归。
- [ ] 完成亮色、暗色、默认窗口和窄窗口回归。
- [ ] 完成托盘、通知、专注窗口和系统语言切换回归。
- [ ] 验证旧数据库、旧同步数据和 `.eggdone-backup` 兼容。
- [ ] 更新 README、手动回归文档和发布说明。
- [ ] 更新版本号并检查关于页版本显示。

自动验证：

```bash
pnpm check
pnpm test
pnpm build
cd src-tauri
cargo fmt -- --check
cargo check
cargo test
```

完成条件：自动检查通过，Windows 双语手动回归无阻塞问题，可进入发布流程。

## 推荐提交边界

1. `docs(i18n): freeze cross-client terminology and rollout`
2. `feat(i18n): add desktop language foundation`
3. `feat(i18n): localize desktop todo workflows`
4. `feat(i18n): localize notes settings and data tools`
5. `feat(i18n): localize tray focus and notifications`
6. `feat(i18n): add bilingual quick add and error codes`
7. `test(i18n): cover desktop language regression`
