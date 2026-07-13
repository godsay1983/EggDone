# EggDone 桌面端便签 Roadmap

配套设计见 [NOTES_IMPLEMENTATION_PLAN.md](NOTES_IMPLEMENTATION_PLAN.md)。阶段按依赖顺序执行，未完成协议和持久化前不进入 UI 联调。

## D0：跨端协议冻结

- [x] 与鸿蒙端确认 Note 字段、字符上限和颜色枚举。
- [x] 固定 `notes.json` Object Key 推导规则。
- [x] 固定冲突决胜顺序和删除墓碑语义。
- [x] 在两端保存相同的 JSON fixture 和预期合并结果。
- [x] 明确旧客户端只同步 Todo、不会触碰 Note 对象。

交付物：跨端协议 fixture、决胜测试样例、实现方案定稿。

## D1：数据库与领域模型

- [x] `db.rs` 增加 migration 14 和 `notes` 表。
- [x] 验证全新数据库建库。
- [x] 验证 migration 13 升级且原 Todo 数据不变。
- [x] 新增 Note Rust 类型和参数校验。
- [x] 实现列表、创建、更新、置顶、换色、软删除和撤销。
- [x] 增加 Repository / command 单元测试。

完成条件：不依赖 UI 即可通过 Rust 测试完成 Note CRUD。

## D2：前端 API 与状态层

- [ ] 新增 `noteApi.ts` 和严格类型。
- [ ] 新增 `noteStore.ts`，封装加载、保存、删除、撤销和错误状态。
- [ ] 实现 600 毫秒自动保存防抖和离开编辑器时强制刷新。
- [ ] 本地保存后只标记 Note dirty，不直接并发启动多个同步任务。
- [ ] 增加 Store 单元测试。

完成条件：使用测试 UI 或命令可以稳定创建、编辑并重新加载便签。

## D3：桌面 UI

- [ ] 视图切换器增加 `便签`。
- [ ] 便签模式复用快速新增区域，不新增常驻控件排数。
- [ ] 实现单列 NoteCard 列表和空状态。
- [ ] 实现当前面板内的 NoteEditor。
- [ ] 实现搜索、置顶、换色、删除和撤销。
- [ ] 增加 `Ctrl+Shift+N`、`Ctrl+F`、`Ctrl+S`。
- [ ] 完成亮色、暗色和长文本截断样式。

完成条件：Windows 托盘面板中所有便签操作可用，切换回 Todo 后原状态不丢失。

## D4：S3 / MinIO 同步

- [ ] 新增 NoteSyncDocument 构建、校验和合并。
- [ ] 实现 `notes.json` 下载、ETag、`If-Match` 上传和冲突重试。
- [ ] 远端 404 时按空文档处理并创建对象。
- [ ] Todo 和 Note 共用同步互斥锁，分别维护 dirty 和 ETag。
- [ ] 前台恢复、60 秒检查、手动同步和下拉刷新覆盖 Note。
- [ ] 同步状态详情区分 Todo 和 Note 结果。
- [ ] 增加跨端 fixture 测试。

完成条件：桌面创建的便签可被鸿蒙端读取，冲突重试后两端结果一致。

## D5：备份与兼容

- [ ] 全量 JSON 导出增加 `notes`。
- [ ] 导入旧格式时将 `notes` 视为空数组。
- [ ] 导入新格式时校验并合并便签。
- [ ] 验证旧客户端继续同步 `todos.json` 不会删除便签。
- [ ] README 增加便签和同步对象说明。

完成条件：新旧备份都能导入，旧客户端与新客户端可在同一 S3 目录共存。

## D6：回归与发布

- [ ] 运行 `pnpm check`。
- [ ] 运行 `pnpm build`。
- [ ] 运行 `cargo fmt -- --check`。
- [ ] 运行 `cargo check` 和相关 Rust 测试。
- [ ] Windows 手动验证托盘显隐、失焦隐藏和便签自动保存。
- [ ] 验证亮色、暗色和不同面板宽度。
- [ ] 执行桌面创建、鸿蒙编辑、桌面删除的完整同步链路。
- [ ] 更新版本号、关于页和发布说明。

完成条件：所有自动检查通过，跨端增删改同步和离线恢复通过真实 S3 / MinIO 验证。

## 推荐提交边界

1. `feat(notes): add desktop note persistence`
2. `feat(notes): add desktop note interface`
3. `feat(sync): sync notes through separate s3 object`
4. `test(notes): cover migration and cross-device merge`
5. `docs(notes): document desktop notes and release behavior`
