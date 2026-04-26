# MyNotes TODO

> 基于当前文档状态整理的执行清单。
> 说明：
> - `[x]` 已完成
> - `[ ]` 待完成 / 候选
> - 本文件偏“执行视图”；设计理由以 `design_V2.md` 为准，交付细节以 `delivery_log.md` 为准。

## 已完成

- [x] Phase 2 全部完成
- [x] P3-A1 App Config + 快捷键自定义
- [x] P3-A2 must-fix sweep
- [x] P3-A3 Graph hardening
- [x] P3-A4 Rename hardening
- [x] P3-A5 命令反馈 notice stack
- [x] P3-A6 Sidebar 拖拽导入
- [x] P3-A7 打印 HTML 主题化
- [x] P3-D1 related-notes AI 辅助面板
- [x] P3-D2a.1 Embedding 索引底座
- [x] P3-D2a.2 Provider 接入 + keychain + 测试连接
- [x] P3-D2a.3a 手动 embed 管道
- [x] P3-D2a.3b watcher 增量 embed
- [x] P3-D2a.4 整库初始化 preview/run
- [x] P3-D2a.5 related-notes 升级 embedding cosine
- [x] P3-D2a.6 AI 失败降级 UX
- [x] P3-D2b.1 Chat 会话数据层
- [x] P3-D2b.2 Provider chat 接口
- [x] P3-D2b.3 右栏 Tab + ChatPanel v1
- [x] P3-D2b.4 流式响应 + cancel
- [x] P3-D2b.5 RAG 上下文注入 + citations + wiki-link 跳转
- [x] P3-D2b.6 独立聊天窗口
- [x] P3-D3.1 `ai_complete` 单次补全 IPC + cancel
- [x] P3-D3.2 `DiffPreviewModal` 共享 UI + 行级 diff
- [x] P3-D3.3 `Summarize current note` 三档写回
- [x] P3-D3.4 `Suggest tags for current note`
- [x] P3-D3.5 `Draft MOC from tag (AI)`
- [x] P3-D4.1 AI 写回流 failure / cancel / retry UX hardening
- [x] P3-D5.1~D5.7 Agentic Chat 全线（tool calling 协议层 / 5 件 read-only / proposal 卡片 / writeback / destructive 二次确认 / 权限矩阵 / audit log）
- [x] **Phase 4 Stage 0 基线**（`pnpm check` 0 err / 0 warn）
- [x] **Phase 4 Stage 1 ChatPanel mock 抽 dev-only fixture**（`PUBLIC_E2E` build-time 常量 + `?e2eMock=1` URL flag 双 gate；新增 `mockBootstrap.ts` 浏览器模式假 vault + `__TAURI_INTERNALS__` invoke 桩）
- [x] **Phase 4 Stage 2 E2E 真断言 + writeback fixme**（`tests/e2e/agent-chat.spec.ts` 七条 case 全部转正，`editor-host` / `active-file-path` 新增稳定 testid）
- [x] **Phase 4 Stage 3 `delete_note → trash`**（`Cargo.toml` 加 `trash = "5"`，`commands/file.rs::file_delete` + 工具描述 + ProposalCard label + 完成文案全部对齐回收站语义）
- [x] **Phase 4 Stage 4 一致性 hardening**（`proposalResolutionStore.ts` localStorage 镜像 + ChatPanel 写入 / 加载 / 删除三处接线）
- [x] **Phase 4 Stage 5 CI 固化**（`.github/workflows/ci.yml` frontend + rust 五道门）
- [x] **Phase 4 Stage 6 文档对齐**（README / plan_P3 / delivery_log / TODO 同步）
- [x] 文档同步到当前状态
- [x] `README.md`
- [x] `plan_P3.md`
- [x] `delivery_log.md`
- [x] `design_V2.md §6.35 / changelog 2.34`

## 当前待完成

- [ ] 补 D4.1 慢路径手测
- [ ] 准备一篇足够长的测试笔记
- [ ] 验证 summarize 的 `canceling -> partial result -> retry`
- [ ] 验证 suggest-tags 的 `canceling -> partial result -> retry`
- [ ] 验证 draft-MOC 的 `canceling -> partial result -> retry`
- [ ] 验证 cancel-before-first-token 文案与保留弹窗行为
- [ ] 验证 cancel IPC 自身失败时 modal 保持打开并展示错误
- [ ] CI 首跑（pushed workflow 后看 frontend / rust 两个 job 是否绿；按报错调一两轮）
- [x] 把 ChatPanel 内嵌 mock script 拆到 `src/lib/e2e/mockChatScripts.ts`（ChatPanel 2330 → 2234 行；mock 通过 `MockChatHandles` 适配器读写 panel 状态；`runMockSend` 一行调用）
- [ ] proposal 镜像跨设备同步（如需要）：`.mynotes/ai/chats/<session>.resolutions.jsonl` + 一个轻 IPC

## P3-D4 后续候选

- [ ] `Rebuild MOC from tag (AI)`，基于 `moc_source_tag`
- [ ] `DiffPreviewModal` section-level 部分接受
- [ ] AI draft 结果缓存
- [ ] suggest-tags 置信度排序
- [ ] suggest-tags 黑名单 UI
- [ ] summarize tone switch
- [ ] 在 draft-MOC prompt 中拼入 `frontmatter.summary` 提升分组质量

## Phase 4 / 长期候选

- [ ] ChatPanel failure / cancel UX 与写回流进一步统一
- [ ] provider `retry_after_secs` 倒计时 / backoff UI
- [ ] AI 结果缓存或后台任务中心
- [ ] 更完整的 AI 批处理能力
- [ ] Mobile Quick Capture + Browse
- [ ] Web 只读分发（可选，低优先级）
