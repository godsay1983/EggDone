# 双端任务模板协议与存储

日期：2026-09-15。P2b-1 内部基础，未发布。
已提交：桌面 866c93f、鸿蒙 d6a3368。下方为 P2b-1 阶段记录。
后续 P2b-2 已接[实际网络同步与备份 v5](TASK_TEMPLATE_SYNC_BACKUP.md)；模板界面仍在 P2c。

## 记录与范围

独立文档为 format_version=1、templates 数组。每条记录包含：
uuid、content、created_at、updated_at、updated_by、deleted_at。
content 只含 name、title、note、group_uuid、checklist（有序文字数组）。
不含原任务 ID、清单实例 ID、勾选、日期、提醒、重复规则、关联便签、附件或专注记录。
不依赖原任务存在；修改、完成、归档或删除原任务不影响模板快照。
相同文字的清单项允许重复，顺序属于模板整体内容。

name 60、title 100、note 1000、单项 200，均按 UTF-16 单元计算；标题/名称/单项须非空且无首尾空白。
时钟使用 0 至 9007199254740991 的安全整数；created_at <= updated_at，墓碑时间在二者之间。
UUID 使用小写 RFC4122 v1-v5；设备标识为 1-128 个 ASCII 字母数字或 ._:-。
未知版本、未知字段、缺失字段、非法控制符、非法分组 UUID、重复记录 UUID 均拒绝。
JSON 中可空字段必须显式为 null。协议不会截断或自动修复非法正文。

## 限额与冲突

本地创建最多 100 个有效模板、20 个清单项。已有超限模板可改名/编辑或减少项数，不能继续增加超限清单。
远端协议上限：1000 条记录（含墓碑）、每模板 1000 项、总计 20000 项、文档 UTF-8 4 MiB。
远端合并超过本地 100 个时仍完整保留，仅禁止本地新增；超过协议安全上限则拒绝整个输入，不部分写入。
分组只作为偏好，不设级联外键；远端缺失分组保留，后续应用模板时提示并重新选择。
本地新建/更换分组必须存在且未删除；编辑既有模板时允许保留已失效的原分组。

同 UUID 的 created_at 必须一致，否则报 TEMPLATE_IDENTITY_CONFLICT，事务回滚。
同一身份按以下全序择大，整体选择一份快照，不混合两份正文：
1. 已删除优先于有效记录，不允许迟到编辑复活墓碑。
2. updated_at。
3. updated_by 的 UTF-8 字节顺序。
4. deleted_at（null 排在时间之前）。
5. 固定键序 content JSON 的 UTF-8 字节顺序。

文档按 UUID 排序，清单文字顺序原样保留。两端共享 JSON 样例及精确编码 golden，验证交换/结合/幂等。
模板删除不能复用旧 UUID；未来重新创建或从其他内容建立模板须使用新身份。不做自动墓碑清理。

## 存储与事务

桌面 SQLite 20 -> 21，鸿蒙 RDB 21 -> 22；应用版本不变。
增量迁移同事务创建 task_templates、task_template_operations、task_template_sync_state 和版本记录。
已有任务、便签、清单表不重写。旧代码遇到更高 schema version 会拒绝打开，回退须使用升级前备份。
模板表只保存 UUID、有效状态索引和规范化记录 JSON；读取时校验索引与 JSON 一致。
独立 revision 由表触发器更新，synced_revision/etag/generation 为后续传输预留，不声称已经同步。

内部写入请求为 operation_uuid、uuid、expected（完整旧快照或 null）、content、deleted。
读取、预览不写库；保存验证完整 expected，不只比较时间戳，避免同一时钟下的远端编辑被覆盖。
删除只能针对未删除的当前快照，保留原内容并生成墓碑。
记录、revision、操作回执同事务；写入/回执/文档上限校验任何失败全部回滚。
重试使用同一操作 UUID、规范化负载及设备标识，时间参数不纳入负载：
- 相同请求且结果仍为当前版本：返回原结果，不重复写入或增加 revision。
- 相同操作 UUID、不同内容：TEMPLATE_OPERATION_REUSED。
- 回执存在但模板已经继续修改或删除：TEMPLATE_STALE_RECEIPT，必须重新读取，不能把旧结果当当前成功。
本地新版本时间为 max(now, previous.updated_at+1)，安全整数溢出拒绝，不损坏记录。

## 验证与边界

- Rust / ArkTS 同跑 31 组共享协议样例；包含 UTF-16 长度、补充平面字符与 UTF-8 冲突次序、删除优先及身份冲突。
- 7 个冲突版本的 343 组结合律组合；有效文档还对比相同 canonical JSON 字节。
- Rust 新增 4 组综合测试；完整 cargo test --locked --lib 为 318 passed / 13 ignored。
- 鸿蒙新增 4 组存储回归及文件数据库关闭重开，覆盖回执失败回滚、重复提交、原任务删除隔离、超限保留、失效分组、完整快照冲突、时钟溢出和迁移重试。
- 两端迁移 SQL 一致性通过；鸿蒙旧 recurrence protocol 47 组及既有 checklist editor gateway 13 组回归通过。
- 提醒完成回归 9 组通过；旧测试加载器原先缺少清单迁移依赖，本轮补齐清单/模板迁移后验证，未修改产品提醒逻辑。
- cargo check、桌面 pnpm check 通过；鸿蒙新增文件 MCP 无错误，Debug 构建通过。既有告警及少量事务异常传播提示仍保留。
- 桌面 pnpm build 成功（264/270 模块，13.25s）；最终鸿蒙 Debug 构建 7.934s，33 tasks / 19 executed / 14 up-to-date。
- 测试使用隔离 SQLite；不代表原生设备 RDB、真实 S3 或设备恢复已验收。没有代理安装或发布行为。

模板表目前只由内部 repository 和测试使用，未注册公开 IPC/产品入口。
现有 v4 备份尚未包含模板，不能用内部测试写入真实用户模板后交付。
P2b-2 必须完成独立对象键冲突检查、配置世代、revision/ACK、412 重试、状态汇总与备份新版本后，才允许 P2c 开放模板编辑。
旧格式导入需保留本机模板，新格式必须验证模板并与其他数据同事务恢复；恢复后失效旧操作回执并标脏。
