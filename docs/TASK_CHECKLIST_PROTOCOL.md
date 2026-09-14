# 检查清单协议 v1 与备份 v4

日期：2026-09-14；阶段：P1a。
这是两端下一步存储/同步实现的冻结契约和可执行参考，不是已经接入产品的网络服务。
共享样例：fixtures/task-checklist-v1.json；参考校验器：scripts/task-checklist-reference.cjs；测试入口：scripts/test-task-checklist-contract.cjs。
P1a 冻结时不改 SQLite 19 / RDB 20。2026-09-14 的 P1b-1 已实现 SQLite 20 / RDB 21 迁移、生产解析器及存储事务，见[存储记录](TASK_CHECKLIST_STORAGE.md)；P1b-2 已实现内部完整编辑/规则分叉事务，见[编辑事务记录](TASK_CHECKLIST_EDITOR.md)。网络和 UI 尚未接入，生产导出仍为 v3。
任务模板的 wire schema 留到 P2，不在本阶段提前塞进检查清单对象。

## 1. 对象与字段

两个独立对象与领域状态：
- task-checklist-items.json：{ format_version: 1, items: [...] }。
- task-checklist-definitions.json：{ format_version: 1, definitions: [...] }。
- Key 从 Todo Key 所在目录派生；与 Todo、Note、附件、规则、关联及另一个新对象冲突时拒绝保存配置。
- 不覆盖原有 Todo / recurrence-rules JSON，不把子项伪装成任务。

实例子项全部字段必填，nullable 字段也不能省略：
| 字段 | 语义 |
| --- | --- |
| uuid | 普通项为新 UUID；继承项为下文确定性 UUID v5 |
| todo_uuid | 不可变父任务 UUID，不使用本机整数 id |
| source_rule_uuid / source_entry_uuid | 普通项均为 null；继承项均为规范 UUID，不允许只填其一 |
| content | 1～200 UTF-16 单元，trim 后等于原文，单行，无控制符/双向控制符/不成对代理项 |
| sort_order | 0～9007199254740991 的安全整数，排序同值时按 uuid 升序 |
| completed | 严格 boolean |
| created_at / updated_at | 安全整数毫秒，0 <= created_at <= updated_at <= 9007199254740991 |
| updated_by | 1～128 位 ASCII，字符限 A-Z a-z 0-9 . _ : - |
| deleted_at | null 或 created_at <= deleted_at <= updated_at |

系列定义全部字段：
- rule_uuid、first_todo_uuid：规范 UUID；rule_uuid 同时是定义身份。
- schedule：既有 RecurrenceSchedule 完整快照；与同 rule_uuid 的规则日程相同，不包含进度/当前任务/修改时钟。
- timezone_id：沿用既有规则语义；全天为 null，定时须为合法时区 id，真实可解析性由现有时区服务复核。
- applies_from_index：固定 2。当前/首个任务保存自己的清单，定义只用于后续实例，不反向覆盖当前任务。
- entries：[{ uuid, content, sort_order }]。entry UUID 在定义内唯一，正文/排序与实例项同约束，不含 completed。
- created_at、updated_at、updated_by、deleted_at：同实例项时钟/设备约束。
- 定义的 rule_uuid、first_todo_uuid、schedule、timezone_id、applies_from_index、entries、created_at 不可变。
- entries 按 uuid 规范化再比较；同一 rule_uuid 出现不同不可变载荷，整笔拒绝 DEFINITION_IDENTITY_CONFLICT，不能随机挑一套再生成子项。

共同限制：规范小写 RFC4122 UUID（v1～v5），未知字段/版本、重复记录身份、缺失字段、非法类型整笔拒绝。
所有数字接受 JSON 等值整数写法（如 1e0），拒绝小数和非安全整数。
实例对象最多 10000 条（含墓碑）；定义最多 2000 条、每定义远端最多 1000 项且总项数最多 20000。
两个对象各最多 4 MiB UTF-8；单文档与合并后的并集均检查，超限明确失败，不截断。
本地新建仍最多 20 项；远端合法超限项不被当作坏数据删除。超限时允许删减、修改、勾选，不允许新增。
传输序列化采用规范字段和 UUID 排序。参考脚本比较语义内容，不以 JSON 属性到达顺序决定冲突。

## 2. 冲突与删除

- 同 uuid 的 todo_uuid、source_rule_uuid、source_entry_uuid、created_at 必须一致，否则 ITEM_IDENTITY_CONFLICT。
- 实例项按以下全序取最大：isDeleted、updated_at、updated_by、deleted_atOrMinusOne、completed、sort_order、content 的 UTF-8 字节序。
- 子项删除优先于任何活动副本，即使活动副本的墙钟更大；防止离线设备和旧备份复活被明确删除的子项。
- 已删除子项不复用 uuid；用户重新添加同文案分配新 uuid。这个终止删除策略不同于任务/关联的显式恢复，不改那些已有契约。
- 同一子项的并发“改文字/勾选/排序”采用整行决胜，可能覆盖较低版本的一方；首版不声称逐字段无损合并。
- 不同子项分别合并；勾选甲与勾选乙都应保留。
- 定义先比不可变身份，再按 isDeleted、updated_at、updated_by、deleted_atOrMinusOne 决胜。
- 父任务缺失是同步待补齐，不是删除；保留子项但不显示可操作入口。
- 父任务显式删除时隐藏所有子项，不额外把子项全写墓碑；恢复父任务后可显示尚未显式删除的子项。
- 父任务完成/归档不删除清单，归档显示只读。
- 子项和定义墓碑不按天数自动清理。停止重复规则不清除定义；历史已存在任务仍能补齐迟到的清单资料。

## 3. “以后”的定义与规则分叉

已核对现有 recurrence_editor.rs：保存新规则/修改以后会分配新 rule_uuid，并以当前任务为 first_todo_uuid。
检查清单沿用此机制，不增加一个会不断改写历史含义的“最新模板”字段：
1. 首次为重复系列设置清单，或修改“以后”的清单，即使时间安排未变也创建新 rule_uuid。
2. 新规则与完整不可变定义在同一事务中保存；定义可为空，空定义表示后续不继承，不能用缺失定义表示清空。
3. 原规则停止，原定义保留；历史实例只与旧规则相关。
4. 当前任务的编辑草稿（包括其清单）同事务保存；新定义只从 index=2 开始生成。
5. 不得仅为改清单就调用会清空旧提醒的现有排期修改副作用：P1b 需独立编排，日程未变时保留当前日期、提醒及其系统注册状态。
6. 修改“仅本次”不分叉、不修改定义；生成后任务的勾选永远不会写回定义。
7. 不允许向既有 rule_uuid 后补一份可变定义。旧端规则首次启用“以后清单”先分叉，从而避免两端争用同一旧规则定义。
8. 分叉后的规则仍由已有重复冲突决胜机制处理；新定义不改变规则赢家。未赢得父任务关联的规则不得生成新任务。

普通 daily/weekly 等旧式重复也必须通过既有高级规则转换并原子建立定义后，才可承诺“以后”继承。
无法转换或规则上下文未同步齐时，允许只编辑本次，并明确原因，不静默承诺继承。

## 4. 确定性身份与补齐

父任务身份完全沿用现有 recurrenceOccurrenceKey / recurrenceTodoUuid：
eggdone/recurrence/v1/{rule_uuid}/{localDate}T{HH:mm 或 date}/{index}
首项沿用 first_todo_uuid，后续由现有 DNS namespace UUID v5 推导；不新写一套日历算法。

继承清单项：
UUIDv5(DNS_NAMESPACE, "eggdone:task-checklist-item:v1:" + todo_uuid + ":" + entry_uuid)
只接收规范小写输入；父任务 UUID 已编码规则/发生日/发生序号，entry UUID 区分步骤。
SHA1 只作为 UUID v5 的规定算法，不用于数据完整性或加密。

生成候选：content/sort_order 取定义，completed=false，created_at=definition.created_at，updated_at=definition.created_at，
updated_by="checklist-seed-v1"，deleted_at=null。设备无关，不取当前手机/电脑时钟。
后续用户写入时钟必须大于已有 updated_at；溢出拒绝，不回退随机值。

补齐前由生产层使用既有重复校验器确认：
- 任务确实存在且未删除；归档任务不后台改写，待用户取消归档后再补齐。
- rule_uuid、first_todo_uuid、日程、时区与定义匹配；发生日/index 对应父 UUID。
- index >= 2，父任务不是手工独立任务；缺少任一依赖时等待，不猜日期或生成父任务。
- 只按确定性 UUID 插入“尚不存在”的子项；已存在的勾选、改文案、墓碑都保留。
- 若待补齐 UUID 已存在但不可变身份不同，拒绝整次补齐，不把身份冲突当已完成。
- 已停止规则可以补齐既有历史父任务，但不得生成新父任务。
- 一个定义后续不再改变，故晚到定义不会用今天的模板覆盖昨天的任务；新定义属于新 rule_uuid。
参考脚本的 materialize 只接收“生产层已验证的上下文”fixture，不代替日历/时区/关联有效性验证。

## 5. 同步与持久化门槛

P1b-1 已增量迁移至桌面 20、鸿蒙 21；P1b-2 复用现有表，无新迁移。后续迁移先重读当前 schema，若已有其他迁移占号则顺延，不能覆盖。
独立实例表、定义表、分领域 revision/synced_revision/ETag/配置世代，不把 UI rowKey 写成 UUID。
父任务+清单保存、规则分叉+定义、墓碑、标脏与操作回执须同事务；写前比对期望版本，失败全部回滚。
仅正文/勾选真的变化时标脏；远端合并中的只读/隐藏状态不反写用户数据。

下载先合并任务和规则，再合并定义与实例项，最后在有完整依赖的父任务上补齐。
上传先父任务/规则，再定义，再实例项；各领域有独立 ACK，上传旧快照不得清除新修改。
换桶/路径/凭据世代隔离，不沿用原配置 ETag；404 允许无对象，403/网络错误不当空文档。
PUT 使用条件写；412 重拉重合并，有界重试。子项待传入首页同步汇总，不能任务文本成功就显示全部已同步。
接入前只运行 fixtures，不访问真实 S3。物理设备、用户旧库及远端混用留到 P1e。

## 6. 备份 v4

仅冻结将来的 data.json/普通 JSON 数据格式，生产导出目前仍为 v3。
v4 在有效 v3 全部字段基础上，必需增加：
- task_checklist_items：完整 items v1 文档（含墓碑）。
- task_checklist_definitions：完整 definitions v1 文档（含空定义和墓碑）。
空领域也写空文档；null、缺字段、错版本、身份冲突、未知顶层新增字段整笔拒绝。
ZIP manifest 保持 v1；原件、SHA256、文件预检与恢复回滚沿用原逻辑。

v1/v2/v3 必须继续按旧契约验证；不得携带 checklist 字段以伪装兼容。
旧版备份缺少新领域时保留当前清单/定义，不清空；v4 恢复用同一合并规则保护本地子项墓碑。
父任务仍删除时不展示恢复的子项；恢复原子性覆盖既有所有领域及新增两个领域。
不导出配置、ETag、ACK、revision、本机操作回执和凭据。可移植重复回执继续沿用旧契约。
恢复完成重置新增领域的同步回执并标脏，不能把备份当已上传证明。
预览单独显示子项总数/已删除数/定义数/缺失父任务数；不声称所有记录都是新建。
P2 模板另行升级备份版本，不在 v4 中放未定义字段。

## 7. 本阶段证明与下一步

参考校验器及 fixtures 用于验证模型约束、冲突交换/结合/幂等性、稳定身份、删除保护、依赖等待与备份版本门槛。
它们是独立 Node 参考实现，不是 Rust/ArkTS 生产解析器，也不证明真实数据库、S3、系统提醒或设备体验。
P1b/P1c 必须让生产解析器、事务和同步运行同一组 fixtures；不能只把参考脚本复制两份就称作双端实现完成。
