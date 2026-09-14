# 检查清单存储基础（P1b-1）

日期：2026-09-14。两端同步实施，尚无用户界面入口。
本页记录生产代码，不替代 [协议](TASK_CHECKLIST_PROTOCOL.md) 或真机验收。

## 已实现

- 桌面 SQLite 19 → 20，鸿蒙 RDB 20 → 21；增量创建子项表、系列定义表、分领域同步状态表、本机操作回执表。
- 生产 Rust / ArkTS 校验器执行同一组 84 个共享样例，覆盖有效/无效文档、合并冲突和 UUID v5。日程校验使用现有真实重复引擎。
- 两个领域同事务合并，规范化 JSON 相同则不增加 revision；保留墓碑和父任务尚未到达的子项。
- 一致性读取在读事务内取得状态和记录，检查索引与 JSON 身份对应关系；损坏数据拒绝继续写入，不当作空列表。
- 父任务标题、备注和本次实例清单原子保存；失败同时回滚任务、子项、标脏与操作回执。
- 保存同时比较父任务 updated_at 与该任务完整清单快照，阻止远端仅修改子项时被旧草稿覆盖。
- 操作 UUID 对应固定负载，重试返回既有结果，不重复修改；同 UUID 换负载拒绝。回执只留本机，不进入备份。
- 删除子项写终态墓碑，不允许复用 UUID。父任务删除仅隐藏子项，恢复后显示未删除子项；归档任务只读。
- 远端超过本地 20 项上限仍完整保留，允许编辑/勾选/删除；超限新增拒绝。UTF-16 长度、时钟上限、重复身份均检查。
- 磁盘重新打开后保留清单和操作回执。没有改动现有日期、提醒、系统提醒注册、重复生成或活动实况窗。

## 调用边界

桌面：
- `src-tauri/src/task_checklist_protocol.rs`：严格生产解析、编码、合并和身份计算。
- `src-tauri/src/task_checklist_store.rs`：snapshot、merge、save、visible_items。
- `save_in_transaction` / `merge_in_transaction` 可加入调用方拥有的事务；出错必须回滚，不能捕获错误后继续提交。

鸿蒙：
- `models/TaskChecklist.ets` / `services/tasks/TaskChecklistProtocol.ets`：类型和生产协议。
- `data/repositories/TaskChecklistRepository.ets`：snapshot、merge、save、visibleItems。
- `saveInTransaction` / `mergeInTransaction` 使用官方 Transaction，进入异步读取前复制输入；事务竞争错误返回调用方，不无限重试。

save 的负载包含 operation_uuid、todo_uuid、expected_updated_at、expected_items、title、note、items。
expected_items 是指定任务的完整子项快照，含墓碑；items 是这次希望保留的活动子项。
省略原活动子项表示明确删除；没有调用保存就不写数据。
保存返回 updated_at；重试可能返回旧回执时间，产品接入时必须重新读取当前内容，不能把旧草稿覆盖回界面。

目前 save 编辑已存在父任务的标题/备注，不处理日期、分组、提醒或规则变更。
新建流程可以在同一外层事务先插入父任务再调用 save_in_transaction/saveInTransaction；已测试外层回滚零残留，但尚未接到产品新建入口。
没有新增 Tauri command 或鸿蒙 store/UI 调用，不允许在 P1c 同步完成前直接暴露成“已完成的清单功能”。

## 尚未完成

2026-09-14：P1b-2 的完整编辑、规则分叉、定义/清单原子保存和失败回滚已实现，详见 [编辑事务记录](TASK_CHECKLIST_EDITOR.md)。
该能力由独立完整编辑 API 提供，不改变上文基础 save 的标题/备注边界；尚未接入 UI、store 或系统提醒注册。

P1c：实际 S3 传输、分领域 ACK/配置世代接线、重复继承/迟到补齐、备份 v4。
当前生产导出仍为 v3，不能声称它已包含清单；新字段尚未开放给用户。
P1d/P1e：编辑/详情/计数 UI、手机平板布局、配套安装包及人工验收。

## 验证记录

- 桌面 `cargo test --locked --lib --quiet`：275 passed，11 ignored（显式隔离 S3 场景）；含 9 组新增清单测试。
- 桌面 `cargo check --locked`、`cargo fmt -- --check` 通过；既有 TraySnapshot.locale 未使用警告仍在。
- 桌面 `pnpm check`：0 errors / 0 warnings；`pnpm build` 成功；Vitest 36 文件、324 项通过。
- 鸿蒙 `node scripts/test-task-checklist-storage.cjs`：84 个共享生产协议样例，实际 ArkTS repository + 宿主 SQLite 事务、回执、超限、旧库迁移/回滚、删除恢复、磁盘重开与损坏索引检查通过。
- 鸿蒙重复协议 47 场景、任务便签关联 51 场景、关联操作 12 组、便签历史 15 场景回归通过。
- 两端便签历史升级测试的模拟旧库版本清理改成包含更高版本，防止新迁移加入后夹具误报；没有改变历史恢复的生产逻辑。
- 两端 P0 18 组草稿样例与 P1a 124 场景参考契约回归通过。84 是生产解析/合并子集；生成、备份和 S3 key 的其余参考样例还不代表生产实现。
- 鸿蒙新增模型、协议、repository 的 MCP 检查无错误；存在异常传播提示。Debug 构建成功，保留原有弃用 API/异常处理等警告。
- 未安装设备、未访问用户旧库或真实 S3，宿主 SQLite 结果不等于原生 RDB/手机平板验收。
- P1a 已提交：桌面 `09682d0`、鸿蒙 `f4e9c70`。P1b-1 随后已按用户授权提交：桌面 `8027e0e`、鸿蒙 `eead870`；未推送、升版、合并或发布。以上测试数量为 P1b-1 历史记录，最新数量见 P1b-2 记录。
