# MyNotes Phase 3 计划

> 本文件记录 Phase 3 的**推进计划**——包含已完成回顾、下一步要做什么、为什么这样排序。
>
> 和其它三份主文档的分工：
>
> - `design_V2.md`：记"**架构决策为什么这样做**"（§10 有路线图索引）。历史 changelog 见 `delivery_log.md` 尾部「版本变更总览（Changelog，历史索引）」。
> - `delivery_log.md`：记"**每一次交付实际做了什么 / 怎么验 / 留了什么坑**"，倒序流水账。
> - `plan_P3.md`（本文件）：记"**Phase 3 这一整段打算怎么走 + 下一步要开哪一刀**"，面向"下次新任务启动前先读这里"。
> - `README.md`：对外说明入口。
>
> 本文件**不复述**全局架构原则与 Phase 0/1/2 历史——只覆盖 Phase 3 范围内的决策与待办。如需上下文，从 `design_V2.md §1` / §10 起读；历史 changelog 见 `delivery_log.md` 尾部版本总览表。

---

## 0. 状态快照（2026-04-21）

- **已落地**：P3-A1 ~ P3-A7 共七刀 + 一次 P3-A2 补坑 sweep（Desktop Hardening 全部完成）；P3-D1 · AI 辅助面板（related-notes 面板）已落地；**P3-D2a 全线已收口** + **P3-D2b 全线（D2b.1 → D2b.6 六刀）已全部落地**；**P3-D3 全线（D3.1 → D3.5 五刀）已落地 · AI 辅助·写回管线贯通**；**P3-D4.1 已落地**；**P3-D5.1 · Tool Calling 协议层已落地**；**P3-D5.2 · 首批 5 件读取类 Tool 已落地**——`Tool` trait 签名吸收 `ToolContext`（pull-through vault_root / index / embeddings / embed_model / cancel），registry 不再空跑，`SearchByTagTool` / `SearchFulltextTool` / `ListTagsTool` / `ReadNoteTool` / `GetRelatedNotesTool` 通过 `tools/mod.rs::register_readonly_tools` 在 `lib.rs::setup()` 中预组装注册；`related_notes_core` 抽为 pub(crate) 供命令 + 工具共用；`fts_sanitize` 提升 pub(crate)；FTS5 contentless `f.path`=NULL 坑改为 rowid JOIN（snippet 空串兜底）；36 条新增 tool 测试 + 208 既有，共 244 全绿——后端 `ChatTurn` 加 `tool_calls` / `tool_call_id` 可选字段 + 新 `ChatRole::Tool`；`ChatRequest` 加 `tools: Vec<ToolDefinition>`；`chat_store.jsonl` schema bump 到 v=2 并向后兼容 v1（宽松加载 + 混行 .jsonl 可读）；新 `tool_registry.rs` 提供 `Tool` trait + `ToolRegistry` 空注册表（D5.2 起逐个真实工具注册）；`openai.rs` 加 `tools` / `tool_choice: "auto"` 请求字段 + `BTreeMap<u32, ToolCallAccumulator>` 重组 SSE 碎片（id 只来自首帧，后续 `push_str` 累加 arguments）；`ai_chat_stream_start` 重写为 `for iter in 0..MAX_TOOL_ITERATIONS=8` 多轮循环，顺序严格为「流式转发 delta → 遇 `finish_reason: "tool_calls"` → 持久化 Assistant-with-tool_calls 到 jsonl（先于执行，避免孤儿 Tool）→ 对每个 call 发 `tool_call_requested` / `registry.execute(..)` / `tool_call_result` / 持久化 Tool 消息 → 下一轮带入」；原子单元截断 `truncate_history_to_budget` 保障每个保留的 Tool 都能找到父 Assistant，否则整块 evict；前端 `ai.ts` 加两事件常量 + 两 payload 类型 + `ChatRole` / `ChatMessage.tool_calls` 扩展；`ChatPanel.svelte` 加 `inlineToolEvents` 占位渲染（`▸ tool request: name(…)` / `◂ tool result: …`）。`cargo test --lib` 208 个测试全绿（新增 ≥10 条覆盖 ChatTurn roundtrip / tool_call accumulator / v1+v2 mix load / empty registry / atomic-unit truncation / MockProvider per-iteration script）；`cargo build` 清编译；`pnpm run check` 0 error 0 warning；`pnpm run build` 成功。`clippy --all-targets -- -D warnings` 有 23 条 **预存** warning（scanner / indexer / embedding_store / graph / project / chunker / embed_service + rag.rs 24-36 doc 未触碰段 + ai.rs 1878 `ai_complete` 未触碰段），非 D5.1 引入。
- **下一主线**：**Phase 4 · 质量工程（发布前硬化）**。D5.1~D5.7 已全部完成并通过 `pnpm check` / `pnpm build` / `cargo test --lib` / `cargo build`，主线从"继续加 Agent 能力"切到"把 agent-chat 闭环打磨到可发布"：先文档状态对齐，再补 E2E 回归、稳定性 hardening 与 CI 固化。
- **Phase 4 首轮硬化（2026-04-26 落地）**：Stage 0 基线 / Stage 1 ChatPanel mock 抽 dev-only fixture（`PUBLIC_E2E` build-time 常量 + `?e2eMock=1` URL flag 双 gate） / Stage 2 E2E 真断言 + writeback fixme 转正 / Stage 3 `delete_note → trash::delete` 系统回收站 / Stage 4 proposal 卡片 resolution 镜像到 localStorage / Stage 5 `.github/workflows/ci.yml` frontend + rust 五道门固化 / Stage 6 文档对齐。详见 `delivery_log.md` 顶部 `2026-04-26 · Phase 4 首轮硬化` 一条。
- **D4.2+ 降为 backlog**：D4.1 的慢路径手测 + `rebuild from tag (AI)` / section-level accept / AI draft cache / suggest-tags 置信度排序 / 黑名单 UI / summarize tone switch / `frontmatter.summary` 拼进 draft-moc prompt——这些原 D4 polish 项退到 D5 推进期间或之后再看。
- **版本对齐**：`design_V2.md` 已更新到 `§6.35`（D4.1）；historic changelog 表已从 `design_V2.md §16` 整体迁移至 `delivery_log.md` 尾部「版本变更总览（Changelog，历史索引）」，最新一行为 `2.34`。`delivery_log.md` 顶部三段式记录为 `2026-04-21 · P3-D4.1 AI failure / cancel / retry UX hardening`。

---

## 1. Phase 3 目标与启动原则

### 目标

Phase 3 不再以"补齐 Phase 2 功能"为目标，而是：

1. **稳桌面内核**（P3-A，已完成）——让"功能齐备"真正进入"可长期使用"。
2. **接入 AI 能力**（P3-D）——复用已稳定的 SQLite + FTS5 索引层，把 RAG / related-notes / MOC draft 这类"对现有 vault 的智能增量"落地。
3. **扩移动端窄功能**（P3-C）——Quick Capture + 浏览，不追求完整编辑。
4. **（可选）Web 只读分发**（P3-B）——优先级最低，见 §3 原因。

### 启动原则

1. **先稳桌面，再开新端**。P3-A 已经把 desktop 推到可长期使用状态，后续不再以"补桌面缺口"为由阻塞 Phase 3。
2. **新能力/新端先做窄而深的最小版本**。AI 先做一条具体管道跑通；Mobile 只做 Quick Capture + Browse；Web 只做只读。
3. **AI 建在稳定索引层之上**。不引入新 schema migration；所有 AI 派生数据放 `.mynotes/ai/` 目录，可删可重建，不污染 vault 真相源。
4. **产品哲学铁律**：
   - Markdown 永远是 SSOT；AI 产物要么是派生索引，要么用户**显式确认**才写回 frontmatter / body。
   - 任何 AI 能力**可一键关闭**；关掉后整条代码路径不发任何请求、不读取任何 provider API key。
   - 每次网络调用可审计（日志 + 可选的成本计数）。

- 离线降级：AI 关闭或无网时，related-notes 等能力仍只走本地索引打分（links / tags / 共同被引 + 若已初始化则 embedding cosine），不发任何网络请求。

---

## 2. 工作线与推进顺序

**新顺序**（2026-04-21 从原 A→B→C→D 调整）：

```
A (done) → D → C → (B optional)
```

| 工作线                                | 排序 | 状态                                                                        | 理由                                                                                                                                                                     |
| ------------------------------------- | ---- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **A · Desktop Hardening / Config**    | 1    | ✅ 完成                                                                     | 自用日增量最直接。A1~A7 + A2 sweep 已收口。                                                                                                                              |
| **D · AI Module**                     | 2    | 🟡 主体完成（D1 ✅ / D2a 全 ✅ / D2b 全 ✅ / D3 全 ✅ / D4.1 ✅）→ D5 新主线 | §7 硬性判定已达成。新主线 **D5 Agentic Chat**（2026-04-21 锁定）：对话面升级为 tool calling 入口，AI 可自主 search / read / propose 写回；命令面板降级为"专家/调试快捷径"，不从 Settings 主推。见 §4.2 D5。 |
| **C · Mobile Quick Capture + Browse** | 3    | ⬜ 待启动                                                                   | 真实痛点（灵感在外面产生要能落下来），但 Tauri Mobile 生态不成熟，调试成本高。放 D 之后可复用 AI 的自动归类 / 摘要。                                                     |
| **B · Web 只读**                      | 4    | ⬜ 可选                                                                     | §1.3 明确"多人协作 / 全文移动端原生编辑"是非目标；Web 主要服务"分享给别人看"的分发语义，和本项目"单用户自用"定位冲突。除非未来出现"vault 对外发布主页"需求，否则可不做。 |

**偏离默认顺序的理由**：B 与 D 对调 + B 降级为可选。AI 能力复用现有 SQLite 派生索引、风险低、收益高；Web 只读一旦做了就要考虑跨设备同步、只读 viewer 打包、部署拓扑等一串问题，超出单用户 KB 的产品边界。

---

## 3. P3-A Desktop Hardening — 已完成清单

（存档用；详细交付记录见 `delivery_log.md` 各条目）

| 编号  | 名称                      | 核心产物                                                                                                                 |
| ----- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| P3-A1 | App Config + 快捷键自定义 | `app-config.json` 持久化主题 / autosave / shortcuts；Settings 里录快捷键 + 冲突检测；`installShortcuts` 改为 keymap 驱动 |
| P3-A1 | TagView 多标签筛选 / 排序 | 后端 `index_notes_by_tags(tags, match_all)`；前端主标签 + 附加 tag + 交/并集 + 四种排序                                  |
| P3-A3 | Graph hardening           | Keyboard roving focus / 屏阅镜像 / 空态提示 / 大图 force preset / `data-theme` 自动重绘 hook                             |
| P3-A4 | Rename hardening          | `rename_preview` 后端命令；文件 + 目录 rename 两阶段 modal（dry-run → 影响列表 → 二次确认）                              |
| P3-A5 | 命令反馈 notice stack     | 把 graph load / extract / export / rename / project 等反馈从 autosave banner 剥离到独立 notice stack                     |
| P3-A6 | Sidebar drop 导入         | 从 Finder 拖文件进侧栏；`file_import` 后端命令；三分支 drop target；`-N` 冲突策略                                        |
| P3-A7 | 打印 HTML 主题化          | `PrintTheme { Light, Dark, System }` 三分支；`@media print` 强制亮色；hex 调色板保证跨 PDF viewer 一致性                 |
| P3-A2 | Must-fix sweep（补坑）    | MOC 模板 sentinel 解耦 / MOC+Extract indexer race / 打印 wiki 链接变锚点 / Windows drive-letter embed                    |

**剩余摩擦（已收敛到 Phase 4 或长期候选池，不进入 P3-A8+）**：

- 前端 vitest harness（目前 `injectMocEntries` / `normalizeAbsPath` / `parseDroppedPaths` / `buildExtractedNote` 都是纯函数但无自动化测试）
- 大图 5k+ 节点的 benchmark 驱动调参
- 跨平台 Windows / Linux 真机冒烟 CI

---

## 4. P3-D · AI Module（下一阶段主线）

### 4.1 总体形态

在桌面端引入可开可关的 AI 辅助层。数据架构：

```
vault/
├─ 0-inbox/ 1-notes/ 2-moc/ attachments/    ← markdown SSOT，AI 绝不写入
└─ .mynotes/
   ├─ index.sqlite                          ← 已有：notes / links / tags / fts
   └─ ai/                                   ← 新增：AI 派生层
      ├─ embeddings.sqlite                  ← chunk embeddings（P3-D2a）
      ├─ chats/<session-id>.jsonl           ← 对话历史 + 元信息（P3-D2b）
      ├─ summaries.json                     ← 摘要缓存（P3-D3）
      └─ usage.log                          ← 每次调用审计（P3-D1+）
```

**不引入新 schema migration** 到 `index.sqlite`。AI 派生数据在 `.mynotes/ai/` 下独立文件，删除整个目录即可"出厂重置 AI"。

### 4.2 子任务拆分

#### P3-D1 · related-notes（本地启发式版，无需 API key）✅ 已完成

**目标**：打开一篇笔记时，右侧/底部面板显示"相关笔记 Top N"。第一版**不调用任何外部 AI**——完全基于已有索引的启发式打分。

**打分模型（v1，纯本地）**：

```
score(current, candidate) =
    2.0  * (共同 tag 数 / min(|tags|))          # tag 重叠
  + 1.5  * (1 if linked else 0)                # wiki 链接直连
  + 1.0  * (1 if cocited else 0)               # 被同一篇笔记引用
  + 0.5  * title_similarity(jaccard n-gram)    # 标题文本相似
  - 0.3  * days_since_updated / 30             # 陈旧衰减
```

全部走 SQLite 查询，不需要向量计算。TopN 取前 5–10 条。

**交付内容**：

- Rust 侧 `ai_related_notes(src_rel_path, limit) -> Vec<RelatedNote>` 命令（放 `src-tauri/src/commands/ai.rs` 新模块）
- 前端 `RelatedNotesPanel.svelte`（右栏新 section，命令面板 `> Show related notes` 入口）
- 单测：打分纯函数 + `ai_related_notes` 数据库查询（10+ 条）
- Settings 里"AI 辅助"区块出现（即使此刻只有"related-notes 面板：开 / 关"一个开关）

**非目标**：不调用外部 API；不做 embedding；不写任何 AI 产物到 markdown。

**为什么作为第一刀**：

- 零 API 依赖，零配置成本，首日就能用
- 验证面板 UX + 数据管道 + Settings 整合
- D2a.5 已按这个接口完成升级：把打分函数里的 `title_similarity` 替换成 `embedding_cosine`，命令签名保持不变

#### P3-D2a · Embedding 索引底座（RAG 基座）

**目标**：把全 vault 段落 embed 落到 `.mynotes/ai/embeddings.sqlite`，作为 D2b 对话面 RAG 检索的底座 + D1 打分升级。**无独立 UI 页**——触发入口放 Settings "AI 辅助 · 初始化索引" 按钮。

**关键决策**：

- **Provider 抽象**：`AiProvider` trait（`embed(inputs) -> Vec<Vec<f32>>` + D2b 再加 `chat(messages) -> Stream<Token>` 方法），**D2a.1 只实现 OpenAI-compatible 一个真 provider**（覆盖 OpenAI / Ollama / OpenRouter 同一 HTTP 协议）+ `MockProvider` 单测专用；Anthropic 作为后续可选
- **Chunking 策略**：段落切（双换行）+ 单段 > 800 token 按句子二次切；保留 `note_rel_path` + 绝对 byte `offset_start/end` 供 D2b 回引高亮
- **向量存储**：纯 SQLite BLOB（小端 f32 × dim）+ 内存 cosine 扫描；vault < 50 k chunks 下 < 50 ms；schema 稳定，未来升 ANN 只换 `search()`
- **增量 embed**：filewatcher + `note_mtime` 对比，debounce 30 s；只对变过的 chunk 重跑；删除的笔记对应 embedding 同步清理（由 `delete_by_note` 提供）
- **Dry-run 必走**：首次全 vault embed 显示"预计 X chunk / Y token / ≈$Z"，用户确认才跑
- **API key 存储**：macOS Keychain / Windows DPAPI / Linux secret-service（`keyring` crate 统一封装）；**不明文写 `app-config.json`**
- **D1 打分升级**：related-notes 的 `title_jaccard` 位替换成 `embedding_cosine`（对外接口不变，权重在 Settings 可调）

**D2a 切片路线图**（见 `design_V2.md §6.17.8` 表）：

| 切片   | 状态      | 产物                                                                                                                                                                                                                                                                                                                                           |
| ------ | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D2a.1  | ✅ 已完成 | Rust 库层：`services/ai/{provider,chunker,embedding_store}.rs` + 24 单测，**无 IPC、无 UI、无真实 HTTP**                                                                                                                                                                                                                                       |
| D2a.2  | ✅ 已完成 | `OpenAiProvider` HTTP impl（OpenAI / Ollama / OpenRouter 同协议）+ `keyring` crate keystore + 4 条 IPC（set/clear/has/test_connection）+ Settings 三栏表单 & 测试连接按钮；+22 单测（openai 12 / secrets 7 / 3 条 provider trait）                                                                                                             |
| D2a.3a | ✅ 已完成 | `AppState.embeddings` 生命周期挂载 + `embed_service::embed_note` 流水线（mtime 增量 / 64-batch / delete-then-upsert）+ 4 条 IPC（`ai_embed_note` / `_stats` / `_delete_note` / `_clear_all`）+ 命令面板 `> Embed current note` + Settings 「AI 索引」面板；+6 单测（empty / frontmatter / basic / up-to-date / shrink-cleaned / missing-file） |
| D2a.3b | ✅ 已完成 | watcher 挂 30 s debounce 增量 embed（create/modify → queue / delete → 同步删除；仅在 `ai_provider` + `ai_enabled` 都就绪时启用）                                                                                                                                                                                                               |
| D2a.4  | ✅ 已完成 | Settings "初始化索引"按钮 + dry-run modal（chunks / tokens / 成本估算）                                                                                                                                                                                                                                                                        |
| D2a.5  | ✅ 已完成 | D1 `ai_related_notes` 打分升级（`title_jaccard` → `embedding_cosine`，消费本地 `embeddings.sqlite`）                                                                                                                                                                                                                                           |
| D2a.6  | ✅ 已完成 | 失败降级（API timeout / 余额不足 / 网络中断 UX；原子替换 + structured failure + 整库 init 提前中止）                                                                                                                                                                                                                                           |

#### P3-D2b · 对话面（α detach 模式）

**子刀拆分（2026-04-21 落地，见 `design_V2.md §6.24`）**：

| 子刀   | 状态      | 范围                                                                                                                                                                                                                                                               |
| ------ | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| D2b.1  | ✅ 已完成 | 会话数据层：`services/ai/chat_store.rs` + 5 条 `ai_chat_session_*` IPC + 前端 `ipc/ai.ts` 5 wrapper；jsonl append-only + sync_data + 后端生成 session_id + 白名单校验；10 条 Rust 单测。**无 UI**。                                                                |
| D2b.2  | ✅ 已完成 | Provider 对接：`AiProvider::chat_stream` trait 方法（默认 `InvalidRequest`）+ `OpenAiProvider` SSE 实现（`bytes_stream` + spawn task + mpsc + `[DONE]` 哨兵）+ `MockProvider` chat script/error harness + `AiProviderConfig.chat_model` 字段 + `ai_provider_test_chat_connection` IPC + Settings 「测试聊天」按钮 & Chat model 输入；`build_configured_chat_provider` helper 预置（D2b.4 消费）。+14 Rust 单测（mock chat 4 + openai SSE 10）。**无 Panel UI 改动**。         |
| D2b.3  | ✅ 已完成 | 右栏 Tab 架构 + 非流式 `ChatPanel` v1：`Panel.svelte` Tab bar（笔记关系 / AI 对话，AI 关时自动回 Links）+ `ChatPanel.svelte`（会话下拉 + `+`/`×` + transcript 气泡 + 最小 markdown 渲染 + Enter/Shift+Enter/Cmd+Enter composer + 乐观 user 气泡 + 结构化失败 banner）+ 后端 `ai_chat_send → ChatSendResult` 非流式 IPC（先持久化 user turn → provider chat_stream + collect → 持久化 assistant turn；失败按 `user_message_persisted` 区分 pre-flight 与 provider 层失败）；`$effect` 用非响应式 `lastResolvedSessionId` 短路避免磁盘 reload 覆盖乐观状态。 |
| D2b.4  | ✅ 已完成 | 流式响应：`ai_chat_stream_start` / `ai_chat_stream_cancel` IPC + `AppState::chat_streams: Arc<Mutex<HashMap<stream_id, Arc<AtomicBool>>>>` cancel 注册表；`ai:chat-stream:{delta,done,error}` 三事件协议（每个事件带 `stream_id` 预留多流路由）；前端 `listen()` + 实时 append + 闪烁光标 + 中断按钮（取消时已累积内容保留到 assistant turn）；`truncate_history_to_budget` 按字符预算截断（4k tok × 3.5 char/tok ≈ 14k 字，永远保系统 prefix + 最新对），+4 条 history 截断单测。 |
| D2b.5  | ✅ 已完成 | RAG 上下文注入：`services::ai::rag`（`embed_query` + `search_and_format`，async/sync 拆分避免 `MutexGuard` 跨 `await`）+ `ai_chat_stream_start` pre-flight 拼接 system message & 返回 `citations`；`[[wiki-link]]` 渲染成 `<span data-wiki-target>` + transcript 事件委托 + `index_resolve_wiki_link` IPC（title / stem 两段 precedence）；新建会话 modal（标题输入 + 「关联当前笔记」checkbox + Esc/Enter 快捷键）替代 `window.prompt`。 |
| D2b.6  | ✅ 已完成 | 弹出独立窗口：新增 `src/routes/chat-standalone/+page.svelte`（独立窗 shell，`onMount` 握手 `EV_READY` 拉 `EV_FILE_PATH`，`onDestroy` emit `EV_CLOSED`）+ `Panel.svelte` 的 `⧉` 弹出按钮 + docked 占位符（`"AI 对话已在独立窗口" + 聚焦 / 取回到此处`）+ 跨窗事件协议 `chat-standalone:{ready,file-path,open-note,close,closed}`（file-path 主→独立、open-note 独立→主、close 主→独立、closed 独立→主）+ `!aiEnabled && standaloneOpen` 自动 `bringBack()` 关闭独立窗 + 600ms 兜底强关；`ChatPanel.svelte` 加 `variant: 'docked' \| 'standalone'` prop；capability `windows: ["main", "chat-standalone"]`。Tauri v2 默认 `fallback: index.html` 让 SPA 路由零改动工作。 |

**目标**：右栏常驻对话面板 + 可弹出独立窗口；两处共享同一份会话数据；多会话持久化到 `.mynotes/ai/chats/<session-id>.jsonl`。

**形态（2026-04-21 锁定）**：

- **α 方案 · detach**：主形态右栏 tab；tab 顶部 "⧉ 弹出" 按钮 → Tauri 新开 `chat-standalone` webview window 加载同一个 `/chat` route，右栏 tab 同时切为 placeholder（"已弹出到独立窗口，点击收回"）
- **放弃 β 双存共生**：用户一次只看一处；β 需要跨窗口 focus 仲裁 + 双向增量同步，ROI 太低

**关键决策**：

- **State 放后端**：会话数据只存 `.mynotes/ai/chats/<session-id>.jsonl`（append-only）；前端 store 订阅后端 emit 的 `ai:chat:message` event；两个 webview 自然看到同一数据源，无需前端同步逻辑
- **Svelte 组件复用**：`ChatPanel.svelte` 带 `variant: 'docked' | 'standalone'` prop；docked 模式紧凑布局，standalone 模式 full-height + 顶部 breadcrumb
- **会话粒度**：全局 sessions 列表（不按笔记强绑定）；新建会话可选 "关联当前笔记"，关联后 RAG 检索优先该笔记 chunks
- **上下文组成**：system prompt + RAG Top-K chunks + chat history（按 token 预算滚动窗口截断，默认保留最近 10 轮 + 必要 system 消息）
- **流式响应**：`AiProvider::chat` 返回 stream；前端逐 token 渲染；中断按钮可撤销当前生成
- **引用渲染**：RAG chunk 在回答里显示为 `[[note-title]]` 内嵌链接，点击跳转到原笔记并高亮 `offset_start..offset_end` 区间
- **写回铁律**：对话里"把这段存为笔记" / "写到 frontmatter.summary" **全部走 D3 的 diff preview modal**；对话面本身不开任何 bypass 通道
- **关 AI 开关后**：右栏 tab 自动隐藏；`chat-standalone` 窗口若存在自动关闭；独立窗口 route 在 AI 关闭状态下渲染 "AI 已禁用" 空页

#### P3-D3 · 可写回能力

**目标**：给用户**主动触发**的写回能力——所有写回都必须用户确认。

- `> Summarize current note`：生成 TL;DR，预览 modal 让用户选"写到 frontmatter.summary" / "写到笔记顶部" / "仅复制到剪贴板"
- `> Suggest tags for current note`：生成候选 tags，用户勾选后写入 frontmatter
- `> Draft MOC from tag` AI 增强：现有的 `buildMocFromTag` 在"已选笔记 → MOC body"之间补一个 AI-suggested 组织结构

**铁律**：没有任何 AI 调用会**自动**改 markdown。用户必须看到 diff 预览并点确认。

**子刀拆分**（2026-04-21 规划）：

| 子刀 | 状态 | 内容 |
| --- | --- | --- |
| **D3.1** | ✅ 已落地（2026-04-21） | `ai_complete` 单次补全 IPC + cancel + TS wrapper（`AppState.complete_requests` 独立 cancel 注册表；窄 failure struct；无 RAG / 无 chat 持久化） |
| **D3.2** | ✅ 已落地（2026-04-21） | `DiffPreviewModal.svelte` 共享 UI（loading / error / diff 三态，`.dpm-*` scoped，Cmd/Ctrl+Enter = accept）+ 行级 diff 库自写（`src/lib/ai/diffLines.ts`，LCS，~30 行，无依赖）；三条写回命令都将复用这一个 modal |
| **D3.3** | ✅ 已落地（2026-04-21） | `> Summarize → frontmatter.summary` / `→ insert TL;DR at top` / `→ copy to clipboard` 三档命令；prompt / body-mutation helper `src/lib/ai/summarizePrompt.ts`；前两档挂 D3.2 modal 做 diff 确认，clipboard 档走 toast + `navigator.clipboard` |
| **D3.4** | ✅ 已落地（2026-04-21） | `> Suggest tags for current note` 单命令；独立 `TagSuggestModal.svelte`（checkbox 勾选 UI）；prompt / 解析 / 合并 helper `src/lib/ai/suggestTagsPrompt.ts`；AI 候选注入 vault 顶 40 tags 做 soft few-shot；`parseSuggestedTags` 三档容错（JSON/CSV/hashtag）；`mergeTagsIntoFrontmatter` 统一写 flow 一行；`indexTags()` 拿 vault taxonomy 做 `复用`/`新建` 徽章区分 |
| **D3.5** | ✅ 已落地（2026-04-21，D3 收官） | `> Draft MOC from tag (AI)` 命令 + mocBuilder modal 加 "用 AI 草拟…" 次按钮；扩 `buildMocFromTag` 加可选 `entriesMarkdown` 覆写扁平列表（AI / 非 AI 路径 downstream 完全一致）；`draftMocPrompt.ts`（prompt + `buildFlatEntriesMarkdown` + `sanitizeDraftMoc` allowlist 校验 `[[title]]`、幻觉标题降级为注释不污染 vault graph）；复用 `DiffPreviewModal` 展示 flat baseline vs AI grouped 两套 entries block 的 diff；漏题时 toast 量化 `droppedCount` |

#### P3-D4 · Polish（原定 Phase 3 完成判定之外；D5 锁定后 D4.2+ 降为 backlog）

| 切片 | 状态 | 内容 |
| --- | --- | --- |
| **D4.1** | ✅ 已落地（2026-04-21）| Failure / cancel / retry UX hardening：summarize / suggest-tags / draft-MOC 三条写回流统一状态机；`normalizeCompleteFailure` / `partialResultNote` helper 共享；loading 态按取消进入"正在取消…"、保留 partial reply、Retry 按钮内嵌 modal |
| D4.2+ | ⬜ backlog | 批处理整个 tag / 目录做摘要 / tag 建议 |
| D4.3+ | ⬜ backlog | Cost dashboard（本月 token 用量 / 各 provider 分账）|
| D4.4+ | ⬜ backlog | 跨设备会话同步（若 P3-C Mobile 后续接对话面）|
| D4.5+ | ⬜ backlog | ChatPanel 同构 hardening（复用 D4.1 failure normalizer）|

**优先级说明**：D5 Agentic Chat 为新主线后，D4.2+ 退为 backlog，除非 D5 推进期间自然触发（例如 D4.5 对话面 hardening 可能并入 D5 某个切片）。

#### P3-D5 · Agentic Chat（新主线，2026-04-21 锁定）

**背景与动机**：

D3 落地后出现工具入口分裂：对话面（D2b）用于"问/探索"、命令面板（D3）用于"做/写回"。用户必须记命令名，学习曲线高。**D5 把对话面升级为 tool calling 入口，让 AI 自主在对话里完成 search / read / propose 写回全流程**；命令面板保留为"专家/调试快捷径"，Settings 里不作为主推路径。

**产品哲学确认**（与铁律兼容）：

- **铁律不废**：任何修改 markdown 的操作，最终落盘前用户必须看到 diff 并明确点"接受"
- **形态转化**：diff UI 从"外挂 DiffPreviewModal"搬到"对话气泡内的 inline diff 卡片"；"调整"按钮仍可展开为完整 DiffPreviewModal
- **权限分级**：读取类 tool 免确认，写回类必须 inline 确认，破坏类双重确认 + 审计日志
- **命令面板命运**：选 **(b) 降级为专家入口**——保留 `> Summarize` / `> Suggest tags` / `> Draft MOC from tag (AI)` 三条命令（底层与 D5 `propose_*` tool 共用实现），但 Settings 不从"AI 辅助"区块主推；首次引导 / 空态提示指向对话面，不指向命令名

**Tool 分类**：

| 类别 | 用户确认？ | 示例 |
|---|---|---|
| 🟢 **读取类** | 不需要 | `search_by_tag` / `search_fulltext` / `list_tags` / `read_note` / `get_related_notes` / `get_backlinks` / `list_notes_in_folder` |
| 🟡 **写回类**（`propose_*` 前缀）| **必须** inline diff 确认 | `propose_summary` / `propose_tag_update` / `propose_moc` / `propose_note_edit` / `propose_new_note` |
| 🔴 **破坏类** | 双重确认 + `.mynotes/ai/audit.log` 审计 | `delete_note` / `rename_note` / `move_note` |

**形态决策**：

- **对话面是主入口**：自然语言驱动，AI 自主决定 tool call
- **inline diff 卡片** vs **外挂 DiffPreviewModal**：对话里走 inline（`ToolCallCard.svelte` / `ProposalCard.svelte`）；"调整"按钮可展开为完整 `DiffPreviewModal` 让用户改 target 等参数
- **tool use trace**：对话里显示 "AI 正在搜索…" / "AI 正在读取 xxx.md…" 状态条
- **权限 Gate**：Settings 有"AI 工具权限"开关矩阵，每类 tool 独立开关；默认 🟢 全开 / 🟡 全开 / 🔴 全关

**切片路线图**：

| 切片 | 产物 | 规模 |
|---|---|---|
| **D5.1** | 后端协议：`AiProvider::chat_stream` 加 tool calling 支持（OpenAI `tools` 字段 / Anthropic `tool_use` 事件预留）；`ToolCall` / `ToolResult` / `ToolCallRequested` / `ToolCallResult` 数据类型；SSE 事件扩展（`ai:chat-stream:tool_call_requested` / `tool_call_result`）；`MockProvider` 扩展支持 scripted tool calls | 中 |
| **D5.2** | 🟢 读取类 tool 先上 5 个：`search_by_tag` / `search_fulltext` / `list_tags` / `read_note` / `get_related_notes`；每个 tool = 已有 IPC 的 wrapper（入参/出参 JSON schema + ts-rs 导出）；零新业务逻辑 | 小 |
| **D5.3** | 对话气泡 inline card 组件：`ToolCallCard.svelte`（读取类，展示 "AI 正在搜索 #xxx…" → 结果预览）+ `ProposalCard.svelte`（写回类，行级 diff + 接受/拒绝/调整三按钮；"调整"展开完整 `DiffPreviewModal`）；复用 `diffLines` 行级 diff 库 | 中 |
| **D5.4** | 🟡 写回类 tool 4 个：`propose_summary` / `propose_tag_update` / `propose_moc` / `propose_note_edit`；底层复用 D3 已有管线（`ai_complete` / `sanitizeDraftMoc` / `buildMocFromTag` / `mergeTagsIntoFrontmatter`）；tool 只吐提议，落盘仍由前端 `ProposalCard` 用户确认后触发 | 中 |
| **D5.5** | System prompt 升级：指导 AI 何时 search / 何时 propose；引用笔记统一用 `[[title]]`；tool use trace 显示逻辑（状态条 + 折叠 tool call 详情）；history 截断策略修正（tool messages 的 token budget 独立计算）| 小 |
| **D5.6** | Settings "AI 工具权限"开关矩阵：每类 tool 独立开关；关 AI 时整片变灰；`app-config.json` 持久化；运行时读取做 pre-flight gate | 小 |
| **D5.7** | 🔴 破坏类 tool（可选 / 低优先级）：`delete_note` / `rename_note` + `.mynotes/ai/audit.log` 审计 + 双重确认 UI | 小 |

**D5 完成判定**：

- 用户在对话里说 "把 #xxx 下的笔记整理成 MOC" → AI 自主调 `search_by_tag` → 吐 `ProposalCard(propose_moc)` → 对话内 inline diff → 用户点接受 → 文件落盘
- 用户在对话里说 "把这篇笔记摘要写到 frontmatter" → AI 自主调 `propose_summary(target='frontmatter')` → inline diff → 接受 → frontmatter 更新
- 命令面板 `> Summarize` 仍可用（底层走同一个 `propose_summary` tool），但 Settings 不主推
- Settings 有"AI 工具权限"区块，可逐类开关；关 AI 时整片禁用

**工作量估算**：D5.1~D5.6 合计 5–8 天；D5.7 可选，2–3 天。

### 4.3 P3-D 各子任务完成判定

| 子任务 | 完成判定                                                                                                                                                                |
| ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1     | ✅ 达成（打开笔记 → 相关笔记面板 Top N；关 AI 仍可用；单测 ≥ 10 条）                                                                                                    |
| D2a    | ✅ 达成（全 vault embed dry-run → 落 `embeddings.sqlite`；D1 升级 `embedding_cosine`；断网降级不污染 DB）                                                               |
| D2b    | ✅ 达成（右栏 + `⧉` detach 独立窗口共享会话；`chats/*.jsonl` 持久化；RAG 引用可跳转高亮；关 AI 自动收）                                                                  |
| D3     | ✅ 达成（三条写回命令全走 diff 预览；关 AI 命令消失；对话面写回也走同一 modal）                                                                                         |
| D4.1   | ✅ 达成（三条写回流 failure / cancel / retry UX 统一状态机；D4.2+ 降为 backlog）                                                                                        |
| D5     | ✅ 已完成：对话里自然语言 → AI 自主 `search_*` / `propose_*` → inline diff 卡片 → 用户接受才落盘；命令面板降级为专家入口；Settings 有 tool 权限矩阵                       |

---

## 5. P3-C · Mobile Quick Capture（D 之后）

### 5.1 范围

- **做**：
  - iOS / Android 客户端（Tauri Mobile 2.x）
  - Quick Capture：标题 + body + tag（可调用 AI 自动建议 tag）→ 写入 `0-inbox/`，触发云同步（iCloud Drive / Syncthing 由用户自选）
  - Browse：Home（今日/本周）+ 单篇只读查看 + 全文搜索
  - 共享扩展：从别的 app 分享文本/链接 → 自动落 inbox

- **不做**：
  - 完整编辑器（CM6 在移动端体验差）
  - 附件管理（图片上传 / 附件查看一期不做）
  - 图谱视图（屏幕太小）
  - 本地 SQLite 索引（一期依赖云同步后的桌面端再 index）

### 5.2 关键不确定性

- **Tauri Mobile 2.x 成熟度**：需要 PoC spike 评估 plugin 覆盖度（文件系统 / keychain / share extension）
- **同步策略**：不自己做 CRDT / 冲突合并；假设用户用 iCloud Drive / Syncthing，Mobile 和 Desktop 在同一目录读写，冲突按 last-write-wins 或由底层同步工具处理
- **Quick Capture 时的搜索**：用户在 Capture 时想 `[[link]]` 到已有笔记，但 Mobile 没有本地索引——策略：保留纯文本 `[[foo]]`，由 Desktop 下次打开时 index 补偿

### 5.3 P3-C 子任务（粗粒度，待 C 开工前再细化）

- C1：Tauri Mobile PoC（Hello World + 最小文件 I/O）
- C2：Quick Capture 窗口 + 模板 + 写 inbox
- C3：Home（今日/本周）+ 单篇 viewer
- C4：全文搜索（FTS5 或 lunr.js 二选一，取决于 Mobile 能否跑 Rust SQLite）
- C5：共享扩展 / App Intents

---

## 6. P3-B · Web 只读（可选，优先级最低）

### 6.1 做的前提

除非出现以下之一，否则此线不开：

- 用户想把某个 vault 作为公开主页发布
- 用户想临时从任何设备浏览 vault（且不想装 Mobile 客户端）

### 6.2 如果做，范围

- 基于 `vault_export_zip` + 静态生成器：把 Markdown → HTML，套 SvelteKit adapter-static
- 页面：Home / 单篇查看 / Tag 索引 / 图谱
- 部署：GitHub Pages / Cloudflare Pages
- 不做：编辑 / 搜索（除非加前端 lunr）/ 图片上传

### 6.3 P3-B 子任务（极粗粒度）

- B1：Static export pipeline（从 vault → HTML 目录结构）
- B2：只读前端（基于现有 SvelteKit 代码复用）
- B3：可选的 Worker-based search

---

## 7. Phase 3 完成判定

**硬性**：

- `P3-A` 全部完成 → ✅ 已满足
- `P3-D` 至少完成 D1 + D2a + D2b 的最小闭环 → ✅ 已满足（实际额外完成 D3 + D4.1）
- **新增硬闸 · P3-D5 最小闭环**（2026-04-21 追加）：
  - 后端 tool calling 协议落地（D5.1）
  - 读取类 tool ≥ 5 个（D5.2）
  - inline diff 卡片组件落地（D5.3）
  - 写回类 `propose_*` tool ≥ 2 个可在对话里端到端跑通（D5.4）
  - Settings 有 "AI 工具权限" 开关矩阵（D5.6）
  - 产品形态决策兑现：用户能在对话里自然语言驱动完成 "找笔记 → 提议改 → 接受 → 落盘" 全流程，不必记命令名

**软性（不再作为阻塞）**：

- `P3-D4.2+` / `P3-D5.7` 可延后或永久不做
- `P3-C` 如果 Tauri Mobile 成熟度 PoC 后决定不做，可以降级到 Phase 4 或永久搁置
- `P3-B` 可永久不做

---

## 8. Phase 4 入口（2026-04-21 起）

主线从 Phase 3 feature delivery 切到 Phase 4 质量工程，按以下顺序推进：

1. **文档对齐**：`plan_P3.md` / `delivery_log.md` / `README.md` 同步声明「D5 已完成，主线转入 Phase 4」。
2. **E2E 回归（第一优先级）**：以 Playwright 覆盖 agent-chat 关键链路：流式回复、tool call 展示、proposal accept/reject/adjust、writeback 后 editor reload、destructive 二次确认、权限矩阵 gate、provider 失败/超时/取消/重试。
3. **安全与一致性 hardening**：长历史截断稳定性、tool trace / usage log / audit log 完整性、配置切换同步、standalone 与侧栏行为一致、proposal 卡片刷新/重开后的状态恢复。
4. **CI 固化**：最低限度固定 `pnpm check` / `pnpm build` / `cargo test --lib` / `cargo build` 四项。
5. **再评估 backlog**：D4.2+（batch summarize/suggest-tags、cost dashboard、cross-device sync、chat polish）改为 Phase 4 后段或更后。

额外安全尾巴：`delete_note` 当前仍是永久删除，目标是转为 "move to Trash"；该项纳入 Phase 4 安全硬化，不阻塞 E2E 首轮落地。

---

## 9. Out of scope（明确不在当前主线内）

- **多人协作 / 多设备冲突合并 / CRDT**：Phase 3 不引入；同步由用户选择的底层工具（iCloud Drive / Syncthing）处理
- **端到端加密**：不做；`.mynotes/` 目录里可能出现 API key 和 embedding，依赖 OS keychain + 文件权限即可
- **Phase 4 质量工程**：E2E 测试（Playwright）、单测覆盖率、CI 签名分发——这些留给 Phase 4
- **全局搜索 UX 大改版**：当前命令面板 + TagView 够用；不做 Obsidian 式的 Quick Switcher 二次改造
- **插件系统 / 第三方扩展**：和"单用户自用"定位冲突；不做

---

## 10. 风险与未决问题

| 风险                                                                  | 缓解                                                                     |
| --------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Tauri Mobile 生态不成熟，P3-C 可能 blocked                            | 先做 D，D 不依赖 Mobile；C 开工前先一个 1-day spike 评估                 |
| AI API 成本不可控（用户不小心跑整个 vault 一次 embed）                | D2 设计时加 batch 上限 + 显式确认 + 成本预估；首次 embed 默认 dry-run    |
| AI 产物污染 markdown 的诱惑                                           | 架构层硬约束：`ai_*` 命令绝不直接 `file_save`；必须走 diff preview modal |
| 关 AI 开关后代码路径不彻底净化（DevTools 里仍能看到 provider import） | 开关逻辑在 IPC 入口做"短路 return"，而不是只隐藏 UI                      |
| 离线降级退化太多，用户感觉 AI 关掉后产品瘸腿                          | D1 的本地启发式版必须做到"值得一直开"的水平；D1 不能只做 API 调用占位    |

---

## 11. 下次任务开工前的检查清单

在进入 Phase 4 开工前：

- [ ] 扫一遍 `delivery_log.md` 顶部最近 3 条，确认 D5 收官状态与已知 gaps
- [ ] `pnpm check` + `pnpm build` + `cargo test --lib` + `cargo build` 起点为绿
- [ ] 先补 E2E 骨架，再逐步把关键链路从占位断言升级为真实断言
- [ ] destructive proposal 的验证用例先按"双重确认 + audit 记录"落最小闭环
- [ ] 开工后按 `delivery_log.md §0.1` 三段式（Scope / How to verify / Known gaps）持续记账

---

## 12. 历史偏离记录

| 日期       | 偏离点                                                          | 理由                                                                                                                                                                          |
| ---------- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-04-21 | 推进顺序从 `A→B→C→D` 调为 `A→D→C→(B opt)`                       | 单用户自用 KB 场景下 Web 只读分发不是刚需（§1.3 非目标）；AI 复用已稳定索引层，风险低、上限高                                                                                 |
| 2026-04-21 | 新增"P3-A2 补坑 sweep"作为非序列编号                            | Must-fix 缺口攒到一定量后打包修，避免插队打断主线；编号 A2 原本预留给 Must-fix 性质的补丁                                                                                     |
| 2026-04-21 | P3-D2 拆分为 D2a（Embedding 底座）+ D2b（对话面 α detach 模式） | 用户确认要"聊天页对话面"形态；原计划的单轮 Quick Ask 形态替换为持续对话面；β 双存共生方案放弃（不值得做跨窗口 focus 仲裁）；D4 的"历史会话持久化"前置到 D2b，对话面没历史即瘸 |
| 2026-04-21 | Phase 3 范围内加入 **P3-D5 · Agentic Chat** 主线；命令面板 AI 类命令降级为"专家/调试快捷径"（选项 b），Settings 不主推 | 用户偏好：对话为主入口、降低命令学习曲线。AI 通过 tool calling 自主完成 search / read / propose 写回；写回铁律以 inline diff 卡片形态兼容（任何写 markdown 仍需用户显式接受）。D5 即 Phase 3 新硬闸；D4.2+ 降为 backlog |
| 2026-04-21 | D5.1 协议层决策：`ChatTurn` 保持 **struct** 不改 enum；chat_store **宽松加载**（v=2 bump + 允许 v=1 行共存）；**持久化先于工具执行**；**原子单元截断**保证 Tool 不出现孤儿；`MockProvider` 改 **per-iteration script**；`MAX_TOOL_ITERATIONS = 8`；tool_registry 在 D5.1 空跑通，真实工具推到 D5.2 | Plan 子代理独立评审给出 10 条修正建议全部采纳。避免 5 个 ChatTurn 消费点全换 match；老 `.jsonl` 文件免迁移；cancel 在 tool 执行中不会产出孤儿 Tool 消息；多轮测试可按 turn 编排脚本；协议层与业务工具解耦便于逐片加 |
| 2026-04-21 | D5.2 决策：`Tool::execute` 签名吸收 **`ToolContext`** 而不是把 5 个依赖分别加参；工具都是 **零字节 unit struct**，registry 在 `lib.rs::setup()` 中 `app.manage(..)` 之前预组装（无 mutate-state-after-move 问题）；`related_notes_core` **抽 pub(crate)** 让命令 + 工具共用评分算法；`fts_sanitize` 提升 pub(crate)；FTS5 contentless `f.path=NULL` 改 **rowid JOIN** | 一次改 5 件工具比分 5 次便宜（ctx / helper / 测试脚手架全共用）；trait 吸收 ctx 比每工具加字段更可扩——未来 🟡 `propose_*` 可以顺手多拿 vault_writer 句柄；production `index_search` 同坑不顺手改（adjacent scope，待专门刀） |
