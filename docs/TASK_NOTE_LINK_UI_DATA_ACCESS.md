# 关联界面数据接入（L4a）

更新：2026-09-12。此阶段只完成数据入口，用户还看不到关联按钮。L4b开始接入界面和状态编排，不把本阶段等同于整项关联功能完成。

## 接口契约

| 操作 | 桌面 | 鸿蒙 | 约束 |
| --- | --- | --- | --- |
| 按端点读取 | list_task_note_links / taskNoteLinkApi.list | TaskNoteLinkService.list | scope为todo或note，UUID校验后绑定参数；一次JOIN读取，不逐条查询正文或附件 |
| 读取当前关系 | get_task_note_link / pair | pair | 包含解绑墓碑，用作显式重新关联的expected版本；不存在返回null |
| 创建任务及关联 | create_linked_todo / create | create | 复用L2原子创建和幂等回执；稳定草稿UUID，提交失败不得重复生成UUID |
| 关联/解绑 | change_task_note_link / change | change | 复用L2预期版本检查；不以旧页面状态覆盖后来的解绑 |

读取结果使用共同的snake_case DTO：link、todo_title、note_title、todo_state、note_state、is_repeating。只返回活动关系，按created_at、uuid排序，不按标题和数据库返回顺序排序。

- 任务状态优先级：missing > deleted > archived > completed > active。便签为missing、deleted、active。
- 已删除对象标题隐藏；尚未下载到本机的端点保留关系并标missing，不擅自删除或推定不存在于云端。
- 重复规则/系列任务标is_repeating，后续UI说明“仅本次”；不会把关系复制到未来实例。
- 数据库错误、非法UUID、索引与record_json不一致、损坏记录均抛错。不能伪装为无关联或操作成功。
- 查询不协调墓碑、不更新ACK、dirty或实体内容。底层同步和删除协调仍由L3/L2负责。

## 实现边界

桌面新增独立Rust command模块并注册4个命令，前端API只做传输。写入时从本机数据库取得device_id与时钟，释放数据库锁后才通知主窗口刷新；创建后刷新托盘计数。界面调用方后续仍需刷新store并安排自动同步。

鸿蒙新增ViewRepository与Service，组件不直接读库。Service在读取本机device_id前复制传入草稿/expected，避免异步等待期间被调用者修改。L4b的store层负责提交后的任务/便签重载、提醒和卡片刷新以及自动同步，不在只读Repository中加入这些副作用。

不修改数据版本、同步格式、备份格式、附件文件、实况窗或小艺实现。不新增外部权限，不读取用户真实云端凭据。

## 自动化验证

- 两端共享fixtures/task-note-link-views-v1.json的8种状态组合；覆盖正常、完成、归档、删除、缺失与重复任务。共享文件内容一致。
- 桌面新增3项Rust聚合测试：只读状态/作用域、墓碑/损坏记录/非法输入、确定性排序。全量cargo test --lib为256通过、7个需要显式隔离S3环境的用例忽略。
- 桌面新增4项API测试，验证命令名、参数、expected与异常传播；前端全量273通过。pnpm check/build、cargo fmt/check通过。原有TraySnapshot.locale未读取警告保留。
- 鸿蒙12组宿主测试运行生产ViewRepository和Service/原子操作：共享状态、墓碑、排序、查询失败、ResultSet关闭、本机身份、异步草稿隔离、过时操作冲突。既有12组原子操作和9组完整关联会话回归通过。
- 鸿蒙3个新增生产文件MCP无诊断。ohosTest保留异常传递给测试框架的lint警告，未用空catch隐藏失败。Debug主包、ohosTest构建通过。
- 手机Mate 80 Pro Max模拟器（127.0.0.1:5557）和平板MatePad Pro 13模拟器（127.0.0.1:5555）TaskNoteLinkNative各6项通过，0失败/错误/忽略。新增原生读取用例验证缺失、完成、归档、删除、标题隐藏、重复提示、墓碑和损坏记录；已有迁移/原子操作/备份/快照测试同时执行。

首次桌面接口异常测试暴露测试钩子误返回mock函数，修正为无返回值后通过。一次前端全量测试在构建负载下发生既有快捷键测试5秒超时和后续状态污染；构建结束后原命令重跑273项全通过，未扩大修改生产设置模块或放宽超时。

## 复现与证据

桌面：pnpm test、pnpm check、pnpm build；在src-tauri内执行cargo test --lib、cargo fmt -- --check、cargo check。

鸿蒙：node scripts/test-task-note-link-views.cjs、node scripts/test-task-note-link-operations.cjs、node scripts/test-task-note-link-session.cjs。原生使用scripts/run-device-tests.ps1，参数-Suite TaskNoteLinks和明确的-Device设备序列号，脚本断言6个用例全部完成。

原生证据（仅本机临时文件，不提交日志或HAP）：

- 手机：C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-device-tests-6f31f829eeac451aa55d5757e3ac28e3
- 平板：C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-device-tests-dec0d83a4e6248819988feaa7e3acc13
- 各目录含build日志、install.log、hap-sha256.txt、hypium.log。采用主包再测试包的install -r；没有卸载或清空用户库。用例只读写自身独立临时RDB库，完成后删除该测试库。

## 仍待完成

L4b-L4d的UI、草稿保存保护、上下文返回、可访问性和多尺寸验收还未实现或执行。当前只连接模拟器，L3d3的物理设备、真实用户旧库/云端/文件选择器恢复仍未验收；不据此阻塞可独立推进的数据接入，也不提前勾选原生人工验收。
