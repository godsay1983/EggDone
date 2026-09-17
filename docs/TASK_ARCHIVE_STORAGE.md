# 双端归档内核与验证

日期：2026-09-17。阶段 LC1a；本阶段纳入本地提交，没有产品入口，未推送或发布。

以上为 LC1a 历史边界。后续 LC1b-1 已接单条管理入口，当前状态见[归档界面与验证](TASK_ARCHIVE_UI.md)。

## 实现范围

- 按 archived_at 降序、UUID 升序查询，字面搜索标题/备注；keyset 分页，查询最多 200 个 UTF-16 单元，单页 1..100。
- 取消归档保留完成状态、完成时间和重复历史；重新打开清除完成、提醒及重复绑定，变为单次任务。两者保留清单勾选及有效关联，失效分组置空并返回 GROUP_RESET。
- 旧式重复日期原来存为本地时间戳，重新打开时转换为同一天的全天日期，避免被解释成定时任务；真正的定时任务仍保留时间。
- 从归档删除只进入回收站并 tombstone 关联，不删除关联便签，不推进重复后继。三种操作遇到活动规则的当前实例均拒绝。
- 查询/预览/写入采用 IMMEDIATE 事务；写前复核任务、分组、重复规则、清单、关联及 scope，不仅依赖 updated_at。
- 任务、关系、dirty revision、操作回执、批量进度在各批事务内共同提交；写入、回执或 revision 上界失败整批回滚。

## 本机元数据

复用 app_metadata，不新建 schema、不改变同步 JSON 或备份 v5：

| Key | 内容 |
| --- | --- |
| archive.scope.v1 | 本库随机标识，与现有 sync.target.epoch.v1 共同生成快照 scope |
| archive.op.v1:<operation UUID> | 请求哈希、结果版本、动作和警告，不含任务正文 |
| archive.batch.v1:<operation UUID> | 固定 UUID/快照哈希及逐项结果，不持久复制正文 |

初次有效预览可能初始化 scope，仅写本机元数据，不改任务。备份导入使 scope 和旧回执/批量进度失效；导入失败全部回滚。同步配置 pending 状态拒绝操作，新空间 epoch 拒绝旧预览。

同 ID、同参数重试返回 already_applied，不重放较早动作；同 ID 改参数拒绝。批量首版只支持取消归档和删除，准备后目标不再随查询变化，最多 10000 项、每批最多 50 项；冲突/缺失项分别记录，数据库异常回滚当前批，保留已提交的早前批。重新启动仍可按同 ID 继续。

## 接线边界

桌面实现：src-tauri/src/archive.rs、archive_batch.rs；鸿蒙实现：data/repositories/ArchiveRepository.ets 和 models/Archive.ets。

当前是可测试的存储层，不是可由前端直接传时间/设备身份的最终命令。LC1b 必须：
- 由权威命令/Store 读取当前时钟和本机设备 ID，UI 只提交动作、操作 ID、expected。
- 将结果回执和重新读取的当前实体状态区分，旧操作重试不伪报当前内容仍等于旧结果。
- 提交后刷新任务、搜索、提醒注册、卡片和同步状态；刷新失败说明“已保存、刷新失败”，不重放写入。
- 批量的准备/确认/停止及恢复入口使用固定 job；启动不擅自自动继续破坏性操作。
- 列表在跨页变化时按 UUID 去重并可刷新；存储游标携带原查询词校验，不是冻结整个数据库。
- 接归档管理和统一搜索恢复入口，补空/加载/错误/冲突状态及手机/平板布局。

终态删除凭据、计划/等待生命周期边界属于 LC2/LC3/LC4，未提前实现；不能将本轮软删除当作安全清空。

## 自动化证据

- 双端 docs/fixtures/archive-storage-v1.json 字节一致，23 个共享数据库场景：三种动作、失效分组、同时间戳正文/设备/清单/关联变更、状态变化、活动规则、时钟/dirty 溢出、配置切换、任务/关联/回执失败、JSON 键重排。
- 补充分页、字面搜索与参数绑定、同 ID 重试、正文不入回执、固定批量、冲突跳过、当前批回滚、重开数据库恢复进度。
- 重建前一 schema（桌面 v20、鸿蒙 v21，移除模板迁移新增对象），运行生产 migration 后验证归档；不是用户真实旧库验收。
- 真实生产导入函数验证 v1 导入失效旧归档记录及中途失败回滚；本轮未操作用户数据库。
- node scripts/test-archive-storage.cjs --desktop=D:/Develop/EggDone：鸿蒙生产任务 JSON 导出 → Rust 合并、归档重新打开 → 鸿蒙合并通过；无网络/S3，清单和关联独立同步仍沿用原协议。
- 桌面 cargo test --lib：341 通过、16 条条件忽略；归档交换测试由上述 Node 脚本显式执行，另行通过。
- 桌面 pnpm check（0 错误/0 警告）、pnpm build、cargo fmt -- --check、cargo check 通过；保留既有 TraySnapshot.locale 未使用警告。
- 鸿蒙 test-trash.cjs：11 项共享场景及附加检查；test-recurrence-backup.cjs：22 项；test-task-checklist-backup.cjs：31 项；test-task-template-backup.cjs：4 项，均通过。
- 鸿蒙 Debug 与 ohosTest 构建通过。新增 ArchiveNative.test.ets 使用独立测试库，已加入可选 archive 测试筛选，仅编译未设备执行；ArkTS 编译仍有系统能力及抛异常提示，不声称零警告。

## 未验收

尚无 LC1b 界面及实际用户操作；未执行原生 RDB 测试、系统提醒/卡片刷新、真实 S3、离线双端汇合、手机/平板/桌面视觉验收。上述范围保留到 LC1b/LC1c，不重复用户已确认的旧功能基础验收。
