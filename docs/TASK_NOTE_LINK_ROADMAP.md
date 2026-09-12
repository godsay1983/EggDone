# 任务与便签关联 Roadmap

更新：2026-09-12。
状态：L4d1已提交（桌面d153b95、鸿蒙b549e42）。L4d2第一批原生关联、附件与桌面缩放验证通过，见[原生运行证据](TASK_NOTE_LINK_NATIVE_RUNTIME.md)。继续L4d2其余设备及异常矩阵；L3d3真实用户环境和既有E6人工验收继续保留。不升版、不推送。
方案：[TASK_NOTE_LINK_IMPLEMENTATION_PLAN.md](TASK_NOTE_LINK_IMPLEMENTATION_PLAN.md)

每一项双端一起完成、验证后分别用中文提交。先做协议和原子内核，最后开放入口；不得把未通过的原生/跨端测试勾选为完成。

## L1 协议与迁移
- [x] 冻结 links v1、命名空间、Object Key、限额、冲突、墓碑、悬挂链接和旧客户端行为，见 [协议](TASK_NOTE_LINK_PROTOCOL.md)。
- [x] 共享 51 组有效/无效、身份、并发/删除和 Key fixtures，Rust/ArkTS 共同通过。
- [x] SQLite 17->18、RDB 18->19 迁移和 Repository；不改 Todo.note 与 Note 主记录。宿主验证新库、升级、失败回滚/重试、幂等和既有数据保留。
- [ ] 用户实际旧库升级与双端操作验收：关联入口未开放，后续阶段继续验证。

### L1 验证记录

- 桌面 cargo test --lib：231 通过、2 个真实 S3 测试忽略；新增 4 个聚合测试覆盖协议、限额/合并、Repository、文件数据库迁移/重开。cargo fmt/check、前端 check/build 通过。
- 鸿蒙宿主关联 51 组及生产仓库/迁移测试通过；既有重复协议、日期、提醒完成回归通过。宿主 SQLite/crypto 适配不代替原生 RDB。
- 鸿蒙 Debug 与 ohosTest 构建通过；Mate 80 Pro Max 模拟器（127.0.0.1:5557）及 MatePad Pro 13 模拟器（127.0.0.1:5555）的 TaskNoteLinkNative 各 2 项全部通过、0 失败/忽略。验证原生 SHA1 UUID、RDB 重开、墓碑幂等、迁移/写入失败回滚及重试；仅操作独立临时库，不等于真实用户旧库升级或真实跨端同步验收。
- 设备均覆盖安装主包与测试包，没有卸载、清除数据或主动启动主界面。证据目录：C:/Users/CAOZHI~1/AppData/Local/Temp/eggdone-link-l1-c3c7b755d579476ba60a3e76425d3406。主包 SHA256：8CC9638C6A8EB211A4AA1489398759F0E04E59B2ABB78053DC537EE0076D4681；测试包：EE2D233D30491DD817C0BF3A25B1DFCD763DDE79E42022EBF20C7546732D8092。日志与构建产物不提交。
- L1 没有新增界面、任务/便签原子命令、网络请求或备份格式。旧客户端跨设备协议保持兼容，但升级后的本地库不支持旧二进制降级打开。

## L2 原子操作
- [x] Todo+link 确认创建、关联已有任务、解绑、本机删除实体后的链接墓碑协调。尚未开放 UI，远端删除协调留待 L3。
- [x] 幂等回执、过时请求保护、限额、单调时钟、写点故障回滚及重试。
- [x] 停止/删除重复规则不传播关联到未来，归档/完成保留链接；跳过当前任务只墓碑当前关联。
- [x] 桌面 236 项单测通过（2项真实S3忽略）；鸿蒙 12 组宿主操作测试、Debug/ohosTest构建、手机/平板模拟器各3项原生测试通过。
- [ ] 物理设备/真实跨端同步及未来 UI 的用户验收，不以本轮自动化代替。

范围、回执语义、测试命令和证据见 [TASK_NOTE_LINK_ATOMIC_OPERATIONS.md](TASK_NOTE_LINK_ATOMIC_OPERATIONS.md)。L2 已提交，L3 分阶段执行如下。

## L3 同步与备份
- [x] L3a 本地内核：原子捕获任务/便签/links及revision；快照一致性与ACK保护；配置切换失效；缺失保留、实体墓碑协调、恢复不重连。
- [x] L3a 自动化：桌面新增5项聚合测试、全量241项通过（2项真实S3忽略）；鸿蒙15组新增宿主测试、39组既有会话回归、Debug/ohosTest构建、手机和平板模拟器各4项原生测试通过。
- [x] L3b 网络接入：独立GET/HEAD/条件PUT与dirty/ACK/ETag；实体先于链接；全局会话锁、配置世代隔离、最多两轮外层冲突重试；手动/自动同步与状态摘要。内层冲突仍沿用原有有界预算。
- [x] L3b 协议异常自动化：缺失保留、实体墓碑、404/403/412/传输错误，不把权限/临时错误当空文档；真实旧客户端混用及S3仍待L3d验收。
- [x] L3c 数据备份v3、关联预览、原子恢复与旧备份不复活解绑；双端自动化和手机/平板模拟器各5项原生RDB测试通过，文件选择器/真实数据恢复仍待验收。
- [ ] L3d 整体：真实S3离线双端创建/解绑、冲突、失败恢复和真实用户旧库升级验收。
- [x] L3d1 测试接入：隔离S3服务、桌面/鸿蒙原生签名传输、链接仓库合并与墓碑交换；阶段证据和设备结果见[TASK_NOTE_LINK_S3_INTEGRATION.md](TASK_NOTE_LINK_S3_INTEGRATION.md)。不是整应用同步通过。
- [x] L3d2 完整SyncService会话自动化：实体优先上传、dirty/ACK、离线双端变更、真实S3冲突及恢复；调用生产业务服务，宿主平台适配边界见[完整内核验证](TASK_NOTE_LINK_FULL_SESSION_TESTS.md)。原生验收属于L3d3。
- [x] L3d2a 桌面完整同步内核：5项故障/顺序测试及真实S3双桌面实例/文件交换/错误凭据恢复通过，见[完整内核验证](TASK_NOTE_LINK_FULL_SESSION_TESTS.md)。不等于鸿蒙跨端或Tauri外层命令验收。
- [x] L3d2b 鸿蒙真实SyncService与便签/附件元数据服务、跨端四阶段完整会话交换、真实412及失败恢复，宿主平台适配执行。不是原生文件传输或物理设备验收。
- [ ] L3d3 物理设备、用户旧库升级、原生文件恢复和用户云端环境验收。

L3a范围见[TASK_NOTE_LINK_SYNC_SNAPSHOT.md](TASK_NOTE_LINK_SYNC_SNAPSHOT.md)，L3b生产接入与自动化证据见[TASK_NOTE_LINK_SYNC_SESSION.md](TASK_NOTE_LINK_SYNC_SESSION.md)。链接网络与备份代码均已接入；L3c契约与证据见[TASK_NOTE_LINK_BACKUP_CONTRACT.md](TASK_NOTE_LINK_BACKUP_CONTRACT.md)。真实跨端/设备恢复尚待验收，L3未整体完成。

## L4 双端 UI
- [x] L4a 数据接入：按任务/便签读取关联、端点状态与墓碑预期版本；桌面Tauri命令/API、鸿蒙Service复用L2原子操作。共享8组状态、宿主/原生验证见[接口与验证](TASK_NOTE_LINK_UI_DATA_ACCESS.md)。
- [x] L4b 开发及自动化：从已保存便签创建关联任务、便签关联列表；状态编排、保存保护与同步/提醒/卡片刷新接入。有限模拟器检查与待验收边界见[入口与验证](TASK_NOTE_LINK_NOTE_ENTRY.md)。
- [x] L4c 开发：任务打开便签、关联已有内容、解绑和上下文返回；完整验收独立记入L4d。
- [x] L4c1 开发与自动化：双向选择已有内容、确认解绑、预期版本及来源保存保护；有限模拟器入口/取消检查，不等于原生写入验收，见[管理入口与验证](TASK_NOTE_LINK_MANAGEMENT.md)。
- [x] L4c2 开发与自动化：双向打开、逐层返回、草稿保存失败保护和上下文保留；有限手机/平板模拟器往返检查，见[导航与验证](TASK_NOTE_LINK_NAVIGATION.md)。
- [ ] L4d 完整UI异常、语言/主题/窗口与设备矩阵验收。
- [x] L4d1 双端异常保护及桌面自动化：保存失败阻止删除、图片迟到结果失效、嵌套返回优先级；24组导航、48组管理、24组创建/DST及有限原生菜单返回检查。
- [ ] L4d2 完整原生设备矩阵、软键盘/大字体/分屏/真实附件及原生异常恢复，详见[尚待验证项](TASK_NOTE_LINK_UI_REGRESSION.md)。
- L4d2第一批已验证：桌面8组真实WebView2/SQLite关联、附件与100%/125%/150%缩放；鸿蒙手机Markdown导入、关联创建返回、平板元数据同步与原文件下载。完整矩阵仍未完成，见[范围与复现](TASK_NOTE_LINK_NATIVE_RUNTIME.md)。
- [ ] 便签创建关联任务、任务打开便签、便签查看关联任务。
- [ ] 加载/空/错误/删除/归档状态、未保存草稿保护、上下文返回。
- [ ] 中英文、亮暗、窄桌面、手机平板/大字体/键盘，附件布局不退化。

## L5 发布验收
- [ ] 桌面 release:check、鸿蒙 LocalTest/ohosTest/构建与共享 fixtures。
- [ ] 原生设备、跨端真实同步和升级后数据保留人工验收。
- [ ] 发布说明、版本、handoff，双端兼容版本就绪后才发布。
