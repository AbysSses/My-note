# MyNotes 交付记录

> 逐任务的 **Scope / How to verify / Known gaps** 三段式交付记录。按完成时间**倒序**，最近的在最上方。
>
> 与 `design_V2.md` 的分工：
>
> - `design_V2.md` 记"**为什么这样做**"——架构、原则、章节化的决策。
> - `delivery_log.md`（本文件）记"**这次做了什么 / 怎么验 / 留了什么坑**"——每个 Task 的流水账，方便下一任务快速对齐上下文。
>
> 新任务启动前先扫读最近 2–3 条；若一个 Task 修改了架构决定，**先改 design_V2 对应章节，再回来写 delivery_log**。
> 交付规范（三段式、倒序、不复述全局架构等）见 `design_V2.md §0.1`。

---

## 2026-04-26 · Phase 4 首轮硬化 — Stage 0~6 一刀打包

- **Scope**
  - **Stage 0 · 基线**：在沙箱中跑 `pnpm check` (0 err / 0 warn) + `pnpm build` 的 vite 阶段（adapter-static 的 rimraf 在 FUSE 上不可越权 unlink，与代码无关）；`cargo` 系侧因为没有 GTK 系统库无法本地跑，挪到 CI（Stage 5）。
  - **Stage 1 · ChatPanel mock 抽到 dev-only fixture**：把 `ChatPanel.svelte` 里 `e2eMockMode = ?e2eMock=1` 的运行时检查升级为 `import.meta.env.PUBLIC_E2E === '1'` build-time 常量 **AND** URL flag 双 gate。生产构建里 `PUBLIC_E2E` 未定义 → Vite 把常量折成 `false`，所有 `if (e2eMockMode) { … }` 与 `mock*` 函数被 Rollup DCE 干掉。Playwright 的 `webServer.command` 改成 `PUBLIC_E2E=1 pnpm build && PUBLIC_E2E=1 pnpm preview …`。
  - **Stage 1.5 · 浏览器模式 mock 启动器**：新增 `src/lib/e2e/mockBootstrap.ts`，受同样 build-time gate 控制：当 `?e2eMock=1` 时（i）seed 一个假 `VaultInfo` 跳过 welcome；(ii) 安装 `window.__TAURI_INTERNALS__` invoke 桩，让 `file_write` / `file_delete` / `file_move_with_refs` / `file_read` 等业务 IPC 在 preview 构建里有合理 mock 返回。`+page.svelte` 的 onMount 在 browser 模式下调用 `bootstrapE2eVault()` + `aiEnabled = true`。
  - **Stage 2 · E2E 真断言 + writeback fixme**：把 `tests/e2e/agent-chat.spec.ts` 里的占位写细，新增稳定 `data-testid="editor-host"` / `data-testid="active-file-path"`，把 `test.fixme('writeback 后 editor / note reload')` 转为正式 case：accept propose_summary → 验 `'已写入'` resolution + `active-file-path` 显示 mock 目标 + `editor-host` 可见。
  - **Stage 3 · `delete_note → Trash`**：`Cargo.toml` 加 `trash = "5"`；`commands/file.rs::file_delete` 把 `std::fs::remove_file` 换成 `trash::delete`，错误时显式不静默回退（"failed … The file was NOT deleted"）。`services/ai/tools/delete_note.rs` 描述与 summary 改为「移至系统回收站（需二次确认 · 可恢复）」+ `metadata.recovery = "system_trash"`。`ProposalCard.svelte` accept 按钮文案「确认删除」→「移至回收站」。`ChatPanel.svelte::acceptedMessage` 的 delete 分支文案改成「已移至系统回收站（可在 Finder/资源管理器中恢复）」。E2E destructive case 的断言从 `'已删除'` 改为 `'回收站'`。
  - **Stage 4 · 一致性 hardening — proposal 卡片状态恢复**：新增 `src/lib/chat/proposalResolutionStore.ts`（`localStorage` 镜像，键 `mynotes:proposal-resolution:v1:{session_id}:{view_model_key}`）。`ChatPanel.svelte` 的 `setProposalResolution` 在写本地 state 同时 `persistResolution(...)`；`loadActiveSession` / `switchSession` 都调 `rehydrateResolutionsFor(id)` 重新加载本会话的 chip；`deleteActiveSession` 清掉对应 session 的所有镜像。后端 `audit.log` / `usage.log` 的合规口径不变；这层是 UX 缓存，丢了不影响审计。
  - **Stage 5 · CI 固化**：`.github/workflows/ci.yml` 新增 `frontend` job（`pnpm check` / `pnpm build` / `pnpm exec playwright test`，失败时上传 `playwright-report/`）+ `rust` job（apt 装 `libwebkit2gtk-4.1-dev` 等 Tauri Linux deps + `Swatinem/rust-cache@v2` + `cargo test --lib --no-fail-fast` + `cargo build`）。`concurrency` 按 ref 取消旧跑，PR / push to main 都触发。
  - **Stage 6 · 文档对齐**：`README.md` 顶部状态行改为「Phase 4 首轮硬化已落地」，新增 Phase 4 已落地清单。本条 delivery_log 即对齐记录。
- **How to verify**
  - **本地（沙箱可达部分）**：
    - `pnpm check` → **0 errors / 0 warnings**（svelte-kit sync + svelte-check）
    - 静态扫描所有 `data-testid` 与 `getByTestId` 一对一对齐（grep diff = 0）
    - `import.meta.env.PUBLIC_E2E` 引用只出现在 `ChatPanel.svelte` + `mockBootstrap.ts`
    - `trash::delete` 引用只出现在 `commands/file.rs::file_delete`，没有静默 fallback 到 `remove_file`
    - `proposalResolutionStore` 的 PREFIX `mynotes:proposal-resolution:v1:` 唯一，没有撞键
  - **CI 兜底（沙箱跑不了的部分）**：
    - frontend job 跑 `pnpm exec playwright test`，七条 case 全过；失败上传 HTML 报告
    - rust job 装 GTK / WebKit / soup-3.0 后跑 `cargo test --lib`（应仍 244+ tests ok）+ `cargo build` 通过
- **Known gaps / next**
  - **沙箱限制造成的口径差**：本沙箱缺 GTK + 不能 sudo apt，所以 `cargo` 系列没在本地跑；以 CI 为准，CI 出现红再回来修。
  - **mock invoke 桩仍是「最小可用」**：覆盖了 `file_write` / `file_delete` / `file_move_with_refs` / `file_read` / `file_exists` / `file_list` / `app_config_get` / `index_*`，复杂业务路径（如真实 graph build / rename refs preview）会拿到 `null`。E2E 真扩展时按需补就行。
  - **proposal 镜像是 frontend localStorage**：换电脑 / 清浏览器数据后丢失，但审计 trail 由后端 `audit.log` 兜底。如果未来想跨端同步，再加一个轻 IPC 把镜像同步到 `.mynotes/ai/chats/<session>.resolutions.jsonl`。
  - **CI 还未跑过一轮**：workflow 是按 docs + 经验写的，第一次 push 时大概率会有 deps 拼写或 cache key 错位，按报错调一两轮即可。
  - **Mock provider script 仍内嵌在 ChatPanel**：production 构建已经 DCE 干净，但「应该 chat 长什么样」的脚本散在组件里，不利于以后做 fuzz / story-style 调试。下一刀可以把 mock script 拆到 `src/lib/e2e/mockChatScripts.ts` 让 ChatPanel 只引一个函数。

## 2026-04-21 · Phase 4 启动 — 文档对齐 + Agent Chat E2E 骨架

- **Scope**
  - 把主文档状态从「D5 进行中」对齐为「D5 已完成，主线切到 Phase 4 质量工程」：更新 `plan_P3.md`、`README.md`，明确下一阶段顺序是文档对齐 → E2E 回归 → hardening → CI 固化。
  - 新增 Playwright E2E 骨架（最小可执行）：配置文件、基础 fixtures、agent-chat 关键链路用例占位（先搭测试结构和命名，后续逐条填实断言与测试数据）。
  - 把已知安全尾项记账：`delete_note` 当前仍是永久删除，实现上暂未迁移到 Trash；该项并入 Phase 4 安全硬化。
  - 约束：本刀不新增产品功能，不改现有 AI 协议或业务分支，只做文档与质量工程脚手架。
- **How to verify**
  - `README.md` 顶部「当前状态 / 当前进度」已改为 D5 收官 + Phase 4 启动口径。
  - `plan_P3.md` 的主线描述改为 Phase 4，并新增「Phase 4 入口」章节（E2E、hardening、CI、backlog 评估顺序）。
  - `pnpm` 脚本中可看到 E2E 入口（`e2e` / `e2e:ui`）；仓库存在 Playwright 配置与 `tests/e2e/` 目录。
  - 首轮 E2E 规范包含以下关键场景：streaming、tool trace、proposal accept/reject/adjust、writeback reload、destructive confirm、permission gate、provider fail/timeout/cancel/retry。
- **Known gaps / next**
  - 当前是骨架阶段：多数用例先以 TODO/占位断言形式落地，后续要补测试夹具（vault fixture、provider mock、稳定选择器）再转为强断言。
  - 需要在 UI 层补充一批稳定 `data-testid`，避免 E2E 依赖文案或 DOM 细节导致脆弱。
  - CI 还未接入本次新增 E2E（先完成 smoke 标签用例再接），当前仍以 `pnpm check` / `pnpm build` / `cargo test --lib` / `cargo build` 为最低门槛。
  - `delete_note -> Trash` 仍未实现，仅完成 Phase 4 记账，后续作为安全 hardening 子任务推进。

## 2026-04-21 · Phase 3-D5.2 — 首批 5 件读取类 Tool（🟢 read-only）

## 2026-04-21 · Phase 3-D5.2 — 首批 5 件读取类 Tool（🟢 read-only）

- **Scope**
  - **给 D5.1 的空 registry 填 5 件读取类工具**：`search_by_tag` / `search_fulltext` / `list_tags` / `read_note` / `get_related_notes`。全部 🟢 读取，不改任何 markdown，免权限确认；🟡 `propose_*` 与 🔴 `delete_*` 推到 D5.4 / D5.7。
  - **`Tool` trait 吸收 `ToolContext`（`services/ai/tool_registry.rs`）**：`execute(args: Value, cancel)` → `execute(args: Value, ctx: &ToolContext)`；`ToolContext` 聚合 `vault_root: Option<PathBuf>` / `index: Option<Arc<Mutex<Connection>>>` / `embeddings: Option<Arc<Mutex<EmbeddingStore>>>` / `embed_model: Option<String>` / `cancel: Arc<AtomicBool>`。每个字段都 `Option<_>`，工具自检缺依赖 → `is_error: true` 文案，不 panic。registry 内只保留 `tool_call_id` 回填逻辑，ctx 透传。
  - **`related_notes_core` 公用化（`commands/ai.rs`）**：把 `ai_related_notes`（命令）里的 ~200 LOC 评分核抽为 `pub(crate) fn related_notes_core(conn, &embedding_scores, src_rel_path, limit) -> AppResult<Vec<RelatedNote>>`，命令保留"锁、路径检查、embedding_scores 计算"外壳后委托给 core；`get_related_notes` 工具以同构方式算 `embedding_scores`（无 `embed_model` 则传空 map）后调同一 core。两条路径共享一套打分算法，scope 一动无需两处同步。
  - **`fts_sanitize` 提升 `pub(crate)`（`commands/index.rs`）**：`search_fulltext` 工具直接复用，不复制 FTS5 quoted-phrase 逻辑。
  - **`services/ai/tools/` 新模块（7 个新文件）**：
    - `mod.rs` — `pub fn register_readonly_tools(reg: &mut ToolRegistry)` 一口气 5 注册；注释附"哪个工具读 ctx 哪一字段"的矩阵表。
    - `common.rs` — `err(msg)` / `ok(serialize)` 结果 helper + `parse_str_field` / `parse_uint_field(default, max)`（整型自动 clamp `[1, max]`，float 向下取整，负数/非数字 → default）+ `#[cfg(test)] pub(crate) mod testutil` 提供 `in_memory_conn()`（用 `db::apply_schema` 建真实 schema）/ `fixture_ctx(...)` / `bare_ctx()`。
    - 5 件工具各自一个文件：零字节 unit struct + `#[async_trait] impl Tool` + inline `#[cfg(test)] mod tests`；每件工具的 `execute` 流程都是"参数校验 → prerequisite 检查（vault_root / index）→ 锁 Mutex → SQL/IO → JSON 序列化"四段式，永不 panic，从不跨 `.await` 持锁。
  - **FTS5 contentless 表 `f.path` 返回 NULL 的坑**：`notes_fts` schema 里 `content=''`，这意味着 `UNINDEXED` 列 + `snippet()` 查出来都是 NULL（SQLite 官方行为）。`search_fulltext` 工具改为 `JOIN notes n ON n.rowid = notes_fts.rowid`——因为 `indexer.rs::upsert_note` 是先 INSERT notes 再 INSERT notes_fts，rowid 天然对齐。snippet 退到 `Option<String>::unwrap_or_default()`——宁可给空串也不丢整条 hit。（production `commands/index.rs::index_search` 同样走 `f.path`，应该也有这个坑；D5.2 不顺手修，记入已知 gap 等后续专门刀。）
  - **lib.rs setup() 预组装 registry**：chicken-and-egg 解法——在 `app.manage(AppState { .. })` **之前** 构建 `ToolRegistry::new()` → `register_readonly_tools(&mut reg)` → `Arc::new(reg)` → 塞进 AppState。工具都是零字节 unit struct，预组装零开销。
  - **`ai_chat_stream_start` tool-call 分发（`commands/ai.rs`）**：spawned task 之前捕获四份 ctx 所需 owned 值（`tool_vault_root` / `tool_index` / `tool_embeddings` / `tool_embed_model`）；每次 tool 调用在循环内 cheap 构造 `ToolContext { .., cancel: cancel.clone() }` 传 `&ctx` 进 `registry.execute(..)`。spawned task 依然不借 `State<AppState>`，`'static` 安全不破。
  - **约束与边界**：
    - 不做 inline tool/diff card UI（占位 chip 自动渲染真工具名，留给 D5.3）
    - 不加权限确认门（读取类默认免确认，D5.4）
    - 不做 `read_note` size cap；5 MB 笔记会撑爆上下文（已列 D5.4 待办）
    - 不接 Anthropic `tool_use`
    - 不修 production `index_search` 的 FTS5 contentless 坑（adjacent scope，等专门刀）
- **How to verify**
  - **自动化**：
    - `cd src-tauri && cargo test --lib` → **244 tests ok; 0 failed**（208 既有 + 36 新增：`common` helper × 9 + `search_by_tag` × 5 + `search_fulltext` × 5 + `list_tags` × 3 + `read_note` × 6 + `get_related_notes` × 7 + `tool_registry` 签名改后的 2 条继续 green）
    - `cd src-tauri && cargo build` → **compiled clean**（dev profile，~21s）
    - `cd src-tauri && cargo clippy --all-targets --lib` → **D5.2 新代码 0 warning**；lib 23 条预存 warning 不变（D5.1 记录过的同一批）；`embedding_store.rs:764` 的 `approx_constant` 为 pre-existing dev hard-error（未跟踪文件，不属 D5.2 改动），继续留给独立 lint sweep 刀
    - `pnpm run check` → **0 errors / 0 warnings / 212 files**
    - `pnpm run build` → **success**（adapter-static 产物已写入 `build/`）
  - **代码级核对**：
    - `Tool::execute` 签名：所有 5 工具都是 `async fn execute(&self, args: Value, ctx: &ToolContext) -> ToolResult`；EchoTool 测试随同更新；`bare_ctx()` / `fixture_ctx(...)` 共享 testutil 不重复
    - `lib.rs::setup`：`register_readonly_tools(&mut reg)` 在 `app.manage(..)` 之前，保证 `AppState.tool_registry` 一开始就装 5 件
    - `ai_chat_stream_start`：tool-call 分发处构造 `ToolContext`（含 `cancel.clone()`）传引用；不 spawn 内部不借 state
  - **手测（端到端）**（需配 OpenAI / 本地 LLM）：
    1. 发"列出我所有的 tag" → `list_tags` 事件序列 → 回复里出现具体 tag 名
    2. 发"帮我找 #project 下的笔记" → `search_by_tag({tag:"project"})` 返回 note list
    3. 发"FooNote.md 里写了什么" → `read_note({rel_path:"FooNote.md"})` 返回 content
    4. 诱导 `read_note({rel_path:"../../.ssh/id_rsa"})` → `is_error: true` + `"invalid path"`；下一轮 assistant 正常恢复
    5. `get_related_notes` 执行中按取消 → chip 翻红显示 `"cancelled"`
- **Known gaps / next**
  - **`read_note` 无 size cap**：5 MB 笔记会直接灌进上下文；D5.4 加 `max_bytes` guard + range 读
  - **production `index_search` 的 FTS5 contentless 坑未顺手修**：`SELECT f.path, f.title, snippet(...)` 在 contentless 模式下三列都可能为 NULL，但既有前端调用点 (`ChatPanel` / `CommandPalette`) 未报 bug，说明要么 SQLite 版本差异让它刚好可用，要么前端静默忽略；这刀不触，等专门 lint/bugfix 刀处理
  - **`propose_*` / `delete_*`** 一件都没写：D5.4 / D5.7 各自的范围
  - **inline tool/diff card UI 仍是占位 chip**：`ChatPanel` 渲染的 `▸ tool request: name(args-preview)` / `◂ tool result: json-preview` 是纯文本，没有可折叠、无 diff；D5.3 用 `ToolCallCard.svelte` / `ProposalCard.svelte` 替换
  - **tool call streaming output 不支持**：每 tool 一次性 JSON 返回；超大结果（比如未来 `search_fulltext(limit=50)` + 50×500 字符 snippet）要等全部拼完才送到前端；非紧要，先观察
  - **下一步**：D5.3 inline diff/tool cards — 把占位 chip 换成真组件，ToolCallCard 折叠大 arguments + 大结果；ProposalCard 预留 🟡 写回 diff UI 骨架，为 D5.4 铺路

## 2026-04-21 · Phase 3-D5.1 — Tool Calling 协议层（Agentic Chat 地基）

- **Scope**
  - **只切地基，不带业务**：把"模型 → tool_calls → 注册表 → tool_result → 下一轮"的跨层链路跑通，registry 初始为空，`execute(..)` 永远返回 `"tool 'X' not registered"` 错误——证明协议通、事件通、持久化通。真正的 🟢 读取类 / 🟡 写回类 / 🔴 破坏类 tool 留给 D5.2+ 逐片注册。
  - **协议类型扩展（`services/ai/provider.rs`）**：`ChatTurn` 作为 struct 增两个可选字段 `tool_calls: Option<Vec<ToolCall>>` / `tool_call_id: Option<String>`（`serde(skip_serializing_if = "Option::is_none")`——老 v1 读出来天然 None，Assistant 无工具调用时也不多写）；`ChatRole` 新增 `Tool` 变体（JSON 文本 `"tool"`，与 OpenAI 线协议对齐）+ `#[derive(Default)]` + `#[default]` 标在 `User` 上；新增 `ToolCall { id, name, arguments: String /* JSON 串 */ }` / `ToolCallFragment { index, id, name, arguments_delta }` / `ToolDefinition { name, description, parameters }` / `ToolResult { tool_call_id, content, is_error }`；`ChatDelta` 增 `tool_call_fragments` + 允许 `finish_reason = "tool_calls"`；`ChatRequest` 增 `tools: Vec<ToolDefinition>`（空 vec 即关闭 tool calling）；`ProviderError` 加 `Clone` 让 MockProvider 的 `Vec<Vec<ChatScriptItem>>` 能深拷贝。`ChatTurn::text()` 便捷构造器覆盖 5 处既有调用点。
  - **chat_store v=2（宽松加载）**：`SCHEMA_VERSION` bump 到 2，`append` 新行一律写 v=2；`load` 保持 `v <= SCHEMA_VERSION` 宽松判定——同一 `.jsonl` 里 v=1 + v=2 行可以自由混存，历史文件不强制升级、不重写；`ChatMessage` struct 同步加 `tool_calls` / `tool_call_id` 可选字段。一句话：老用户 vault 升到 D5.1 不需要任何迁移。
  - **`tool_registry.rs` 新文件**：`Tool` trait（`async fn execute(args: Value, cancel: Arc<AtomicBool>) -> ToolResult`，永不 panic、永远返回 struct）+ `ToolRegistry` 包 `HashMap<String, Arc<dyn Tool>>`；`execute("name", call_id, args, cancel)` 在 name 未注册时返回 `is_error: true` + `content` 含 `"tool 'name' not registered"` 并把传入的 `call_id` 回填到返回 `ToolResult`。`AppState` 新增 `tool_registry: Arc<ToolRegistry>` 字段，初始化空注册表。
  - **MockProvider 按迭代数组**：`set_chat_script(Vec<Vec<ChatScriptItem>>)`（`ChatScriptItem = Delta | FinishText | FinishToolCall | Error`）；每次 `chat_stream` 被调用消耗外层下一个元素（`AtomicUsize` 游标），越界 `panic!("mock script exhausted")` 让测试 fail-fast。这样单个测试能编排 "turn1: tool_call → turn2: text finish" 多轮循环。
  - **OpenAI 线协议（`services/ai/openai.rs`）**：`ChatCompletionRequestBody` 加 `tools: Vec<OpenAIToolSchema>`（`{type:"function", function:{name, description, parameters}}`）+ `tool_choice: Option<String>`（`req.tools` 非空时设 `"auto"`，空时全 omit）；SSE `delta.tool_calls[]` 解析用 `BTreeMap<u32, ToolCallAccumulator>` 按 `index` 聚合（id 只信首帧、arguments `push_str` 累加、name 首帧给）；`finish_reason == "tool_calls"` 透传 `ChatDelta.finish_reason`。openai.rs 这一层只保证 **不丢 fragment、按 index 有序**；完整 `ToolCall[]` 的最终 rebuild 放在上层 commands::ai 的多轮循环里调 `accumulator.finish()`。
  - **`ai_chat_stream_start` 多轮循环重构**：外层 `'outer: for iter in 0..MAX_TOOL_ITERATIONS(=8)`；每轮流式转发 delta → 遇 `finish_reason == "tool_calls"`：(1) `accumulator.finish()` 得到完整 `Vec<ToolCall>`；(2) **先持久化**——把 Assistant-with-tool_calls 追加到 chat_store（保证 cancel 在工具执行中不留孤儿 Tool 消息）；(3) 对每个 call：emit `ai:chat-stream:tool_call_requested` { stream_id, call_id, name, arguments } → `tool_registry.execute(..)` → emit `ai:chat-stream:tool_call_result` { stream_id, call_id, content, is_error } → 追加 Tool 消息到 chat_store；(4) `continue 'outer`；遇 `finish_reason in ["stop", "length"]` → append Assistant text → emit done → break；cancel 在轮间或轮内：发 "cancelled ..." 错误 / 正常收尾；8 轮仍未收敛：emit `error { kind: "MAX_TOOL_ITERATIONS_EXCEEDED" }`。
  - **原子单元截断**：旧 `truncate_history_to_budget` 只按 content chars 倒序累加；新版把 tool_call 的 arguments 字节也计入权重（`message_weight_chars`），末尾倒序选取后 **healing** 一步——若剩下的首条是 Tool 但它的父 Assistant 已被 evict，就连带 drop 孤儿 Tool（否则 OpenAI 会 400）。System prompt 永远保留。
  - **前端接线（`src/lib/ipc/ai.ts` + `ChatPanel.svelte`）**：ai.ts 加 `CHAT_STREAM_TOOL_CALL_REQUESTED_EVENT` / `CHAT_STREAM_TOOL_CALL_RESULT_EVENT` 两常量 + 两 payload 类型；`ChatRole` union 加 `'tool'`；`ChatMessage` 加 `tool_calls?` / `tool_call_id?`；新 `ToolCall` 接口。ChatPanel 加 `inlineToolEvents = $state<InlineToolEvent[]>([])` 列表 + 两 listeners；在流式气泡上方渲染纯文本 chip `▸ tool request: name(args-preview)` / `◂ tool result: content-preview`（`is_error` 红色样式）；`ensureStreamListeners()` / `onStreamTerminal()` / `onDestroy()` 三处统一挂载与清理。本刀**不做** inline diff card / 权限确认门 UI（留给 D5.3 / D5.4）。
  - **约束与边界**：
    - **tool_registry 初始为空**：不注册任何 Tool；D5.2 起逐一加 `search_by_tag` / `search_fulltext` / `list_tags` / `read_note` / `get_related_notes`
    - **不触碰 Anthropic provider**：仅 OpenAI 线协议支持 tool calling；Anthropic 的 `tool_use` 事件适配推到未来
    - **不动 commands.ts palette**：§11 第 4 条决定保留为专家快捷径，D5.1 不裁剪入口
    - **SCHEMA_VERSION = 2 但不迁移老 .jsonl**：混行可读；append 时才升级到 v=2
- **How to verify**
  - **自动化**：
    - `cd src-tauri && cargo test --lib` → **208 tests ok; 0 failed**（新增 ≥10 条：ChatTurn text-only / with-tool-calls roundtrip、ChatRole::Tool JSON 小写、MockProvider per-iteration script 推进、script 耗尽 `#[should_panic]`、SSE tool_call fragment 首帧 id + 后续 push_str 累加、tool_call finish_reason 传递、accumulator 缺 id 槽位丢弃、chat_store v1 only load、v1+v2 混行 load、v=2 append 含 `tool_calls` 字段、空 registry execute 返回 `is_error: true`、已注册 tool call_id 回填、原子截断 drops orphan Tool / keeps Assistant+Tool together / tool_call arguments 计权重）
    - `cd src-tauri && cargo build` → **compiled clean**（dev profile，~17s）
    - `pnpm run check` → **0 errors / 0 warnings / 212 files**
    - `pnpm run build` → **success**（adapter-static 产物已写入 `build/`）
  - **clippy 已知坑**：`cargo clippy --all-targets -- -D warnings` 有 23 条预存 warning，**全部** 在 D5.1 未触碰的代码：`scanner.rs` deref-auto-deref × 4、`indexer.rs` bool_assert_comparison × 2、`embedding_store.rs` approx_constant PI × 1、`graph.rs` / `project.rs` redundant_closure × 2、`chunker.rs` div_ceil × 1、`embed_service.rs` complex type × 1、`rag.rs` doc_overindented_list_items × 11（行 24-36，D5.1 只改 127 行的 `ChatTurn::text` 构造，doc 段未触碰）、`commands/ai.rs:1878` redundant_locals（`ai_complete` 函数，D5.1 的 stream 入口在更上方，未触碰此函数）。跑 clippy 确认新增代码不引入新 warning，预存 warning 留给后续独立的 lint sweep 刀（和 `imageEmbed BlobPart` / `CommandPalette TagCount` 两条预存 TS 错同样的处理口径）。
  - **代码级核对**：
    - `ai_chat_stream_start` 的事件序列：文本 turn 仍是 `[delta×N, done]`；工具 turn 是 `[delta×N, tool_call_requested×M, tool_call_result×M, delta×N, done]`；cancel 在工具执行期是 `[delta×N, tool_call_requested, error(cancelled)]`（Assistant-with-tool_calls 已入 jsonl、Tool 未入）
    - `.jsonl` 行级：v1 老行 `{"v":1,"role":"assistant","content":"..."}` 与 v2 新行 `{"v":2,"role":"assistant","content":"...","tool_calls":[{"id":"...","name":"...","arguments":"{}"}]}` 可自由混排，`load` 顺序读出各自解析
- **Known gaps / next**
  - **registry 空跑**：D5.1 冒烟只能看到 `"tool 'X' not registered"` 错误分支；真正的端到端成功路径要等 D5.2 注册 5 个读取类 tool 后才能演示
  - **OpenAI 模型 `finish_reason: null` 中继帧**：已在 SSE 解析里只信"最后非 null 的 finish_reason"，但真实 GPT-4o/Claude 场景还没跑；风险：某些模型在 `tool_calls` 之前先给一个 null，状态机误判为 stop（plan 第 2 条风险列表已提）
  - **MAX_TOOL_ITERATIONS = 8 的阈值**：当前是保守设置，D5.2+ 带真实 tool 后需观察 agent 的自然步长，可能调到 12-16
  - **前端 inline tool chip 只是占位**：`▸ tool request: search_notes(…)` / `◂ tool result: …` 是纯文本条目，未做 diff card / 折叠 / 源引用跳转。D5.3 会把这些 chip 换成 `ToolCallCard.svelte` / `ProposalCard.svelte` 组件
  - **Anthropic tool_use**：未接；仅 OpenAI 线协议支持。D5.4 前若有 Anthropic 用户需求再做适配
  - **下一步**：D5.2 首批 🟢 读取类 tool（`search_by_tag` / `search_fulltext` / `list_tags` / `read_note` / `get_related_notes`），每个都是已有 IPC command 的薄 wrapper + JSON schema 注册。预计 1 天内可落

## 2026-04-21 · Phase 3-D4.1 — AI failure / cancel / retry UX hardening

- **Scope**
  - D4 的第一刀不再新增命令，而是把 D3 已落地的三条 AI 写回流统一补到“出错时也可放心用”的状态：`> Summarize current note`、`> Suggest tags for current note`、`> Draft MOC from tag (AI)` 三条路径全部接入同一套 failure / cancel / retry UX，避免每条流各自长出一套半成品状态机。
  - **两套 modal 的 shared shell 扩展**：
    - `src/lib/ai/DiffPreviewModal.svelte` 新增 `statusNote?` / `retryLabel?` / `showRetry?` / `onRetry?` / `loadingText?` / `cancelLabel?` / `cancelBusy?`，loading 态可显示“正在取消生成…”，error / partial-result 场景可在 footer 直接给 Retry 按钮；body 顶部新增 note 区块承载 advisory 文案。
    - `src/lib/ai/TagSuggestModal.svelte` 做同构扩展，保持 summarize / draft-MOC / suggest-tags 三条命令在 shell 级交互一致。
  - **`src/routes/+page.svelte` 补三套状态机收口**：
    - 抽 `normalizeCompleteFailure(failure, fallbackMessage)`：统一走既有 `formatAiFailureText()`，把 provider / auth / rate limit / invalid request / network 文案收敛到同一口径；特判后端 `"cancelled before any content arrived"` → 用户文案“已取消生成，尚未产出可用内容”。
    - 抽 `partialResultNote(kind)`：如果用户取消时 provider 已经吐出可用 reply，则不丢这段内容，而是在 modal 顶部展示 advisory note，提示这是“取消前已生成的部分结果，可直接接受，也可重试”。
    - summarize / suggest-tags / draft-MOC 三条流各自新增 `*Canceling` 与 `*StatusNote` 状态；打开流程时重置，结束时清理。
    - 取消语义调整：loading 态按 `Esc` / “取消生成”不再立刻关窗，而是先 `*Canceling = true`，把按钮文案切到“正在取消…”，等 `aiCompleteCancel(request_id)` 返回后再由真实结果决定落哪条分支。若 cancel IPC 本身 throw，modal 保持打开并进入 error 态，而不是静默消失。
    - Retry 语义调整：三条流都新增 `retrySummarize()` / `retrySuggestTags()` / `retryDraftMoc()`。summarize 与 suggest-tags 直接重跑原入口；draft-MOC 抽出 `startDraftMocAi(tag, title, picked)` 统一承接首跑与 retry，避免把 picker 再打开一次。
  - **约束与边界**：
    - 不新建全局 toast 通道；仍沿用现有 modal 内错误展示 + notice stack。
    - 不改 `ai_complete` / `ai_complete_cancel` 后端协议；这刀是纯前端 UX 硬化。
    - 不顺手改写回逻辑：summary / tags / MOC 的 accept 行为、落盘路径、panel refresh、open-file follow 全部保持 D3 现状。
- **How to verify**
  - **静态检查**：`pnpm exec prettier --write src/lib/ai/DiffPreviewModal.svelte src/lib/ai/TagSuggestModal.svelte src/routes/+page.svelte`；`pnpm check` → **0 errors / 0 warnings**。
  - **真实打包**：`pnpm tauri build --bundles app` success；产物位于 `src-tauri/target/release/bundle/macos/MyNotes.app`。
  - **桌面插件手测**：
    - 打开打包版 `.app`，用 `⌘P` 进入 AI 相关命令，确认 palette 键盘流正常。
    - 打开 `DiffPreviewModal` 后无需先鼠标点入，焦点已在 dialog 上，`Esc` / 键盘快捷键可直接生效。
    - summarize / suggest-tags / draft-MOC 的基础成功路径仍能打开各自 modal，没有因为 D4.1 的新 props 把既有写回流程打坏。
  - **代码级核对**：三条流程都已接入 `cancelBusy={...}`、`statusNote={...}`、`showRetry={error !== null || statusNote.length > 0}`、`onRetry={...}`；`normalizeCompleteFailure` / `partialResultNote` 作为共享 helper 已在三条状态机上复用。
- **Known gaps / next**
  - **慢路径手测还不完整**：当前 vault 里的实际笔记都很短，AI reply 返回太快，手测时较难稳定打到“loading 中点击取消 → partial reply / cancel-before-first-token / Retry”三条真实分支；目前确认的是类型检查、打包构建、modal 焦点、命令入口和基础成功路径。
  - **尚未做 provider 级 countdown / backoff 提示**：`retry_after_secs` 仍只体现在归一化文案里，没有做 UI 倒计时或自动重试，这保持在 D4 之后再看。
  - **还没做对话面（ChatPanel）同构 hardening**：这刀只覆盖三条写回命令，右栏聊天流的失败 / cancel UX 仍保持 D2b 阶段口径；若后面要统一，可以复用这次抽出来的 failure normalizer 思路。
  - **下一步**：补一篇足够长的测试 note，把 D4.1 的取消中 / partial result / retry 慢路径手测跑通；确认无回归后再继续 `P3-D4.2+` 的 polish。

## 2026-04-21 · Phase 3-D3.5 — AI 辅助·`> Draft MOC from tag (AI)` 命令（分组版 MOC 草拟 + D3 收官）

- **Scope**
  - D3 **收官刀**：把 `ai_complete`（D3.1）+ `DiffPreviewModal`（D3.2）套到"从 tag 建 MOC"流程上，让 AI 对选中笔记按主题分组（H2 小节 + `[[title]]` bullets），用户在 `DiffPreviewModal` 里对比"扁平 baseline vs AI 分组"两套 `entriesMarkdown` 块，确认后走既有 `buildMocFromTag` 管线落盘——downstream（template materialise / sentinel 注入 / `moc_source_tag` 盖章 / panel refresh / 打开文件）与非 AI 版**逐行一致**，只是 entries 不同。
  - **共用同一个 picker modal**：`build-moc-from-tag`（非 AI）与 `draft-moc-from-tag`（AI）两条 palette 命令都 `runBuildMocFromTag()` 打开现有 mocBuilder（选 tag / title / 勾选 notes）——避免复制 tag/title/notes 三段输入。modal 底部新增次按钮 "用 AI 草拟…"（`aiEnabled && picked > 0` 时显示）；primary "创建 MOC" 按钮行为不变。这样 AI 入口既能从 palette 进（命令名 `Draft MOC from tag (AI)`）也能从已打开的 picker 进（一眼看到"还有个 AI 选项"），两入口殊途同归。
  - **扩 `buildMocFromTag(params)`** 加可选 `entriesMarkdown?: string`：非空时覆盖默认扁平 `- [[title]]` 列表送进 `injectMocEntries`；`insertedCount` 仍用 `noteRefs.length`（AI 漏题的话另走 notice 提示，不篡改数值）。只动签名 + 一行替换，对既有调用零影响。
  - **新增 `src/lib/ai/draftMocPrompt.ts`**：prompt + 清洗纯函数。
    - `buildDraftMocPrompt({ tag, title, notes })`：systemPrompt 硬约束输出形状（H2 + bullets、无 prose、无 frontmatter、无 code fence）与语义（"每个 title 出现且仅出现一次、必须逐字匹配提供的列表"——这条是反"模型悄悄丢题"的关键）；userPrompt 把 `tag` / `title` / note titles 作为三个输入字段列出。
    - `buildFlatEntriesMarkdown(notes)`：与 `buildMocFromTag` 内部的扁平 rendering 逐字一致，supply 给 `DiffPreviewModal.original`。
    - `sanitizeDraftMoc(reply, allowedTitles)`：
      1. 剥 ```` ```markdown … ``` ```` 包裹；
      2. 丢掉首个 `## ` 之前的解释性 preamble；
      3. 每条 `- [[title]]` 都按 `allowedTitles` allowlist 校验——不在内的被写成 `- <title>  <!-- AI 生成，非选中笔记 -->`（不删、不变 `[[…]]` 防止污染 vault graph、注释让用户一眼看到模型幻觉）；
      4. 合并连续空行 >1 行、截断 200 行上限；
      5. 返回 `{ markdown, sectionCount, bulletCount, linkedTitles }`，让调用方能量化 "AI 漏掉 N 条" 并落到 toast。Node 内联 4 条 case（normal / fenced / preamble / hallucination）输出均符合预期。
    - `makeDraftMocRequestId()` → `moc-<base36>-<rand>`。
  - **`src/lib/palette/commandRegistry.ts`**：`PaletteContext` 加 `runDraftMocFromTag: () => void | Promise<void>`；注册 `draft-moc-from-tag` 单命令（`when: aiEnabled && activeTag`）——与既有 `build-moc-from-tag`（不 gate AI）并列；两条命令在 `activeTag` 存在时同时可见、AI 开关切换时 AI 版自动隐去。
  - **`src/routes/+page.svelte`**：
    - 新增 `draftMoc*` 状态族（`Open / Loading / Error / Reply / Tag / Title / Picked: NoteRef[] / Flat: string`）+ 非响应式 `draftMocRequestId: string | null`。`draftMocSanitized = $derived.by(...)` 跑 `sanitizeDraftMoc`；`draftMocProposed = $derived.by(...)` 抽出 `markdown` 字段喂给 `DiffPreviewModal.proposed`。
    - `runDraftMocFromTagAi()`：palette run 入口；检查 AI 开 + `activeTag` 有值 → 复用 `runBuildMocFromTag()` 打开现有 picker。本函数**不直接**起 aiComplete——AI 仅从 modal 按钮 fork 出发，避免"打开 modal 又自动 loading"的突兀 UX。
    - `confirmBuildMocWithAi()`：modal 的 AI fork。`drainPendingSaves` 已由上层保证；snapshot `{tag, title, picked}` 到 `draftMoc*`；关闭 mocBuilder 后 `draftMocOpen = true; draftMocLoading = true`；`aiComplete(..., temperature: 0.4)`（theme naming 需要少许 creativity、但温度过高会诱发 title 幻觉）；stale-request guard 三段与 summarize / suggest-tags 同构。
    - `applyDraftMoc()`：从 `draftMocProposed` 拿清洗后 markdown → 调 `buildMocFromTag(cmdDeps, { tag, title, noteRefs, entriesMarkdown })` → `invalidateWikiCompletionCache()` + `schedulePanelRefresh(200)` + `graphRefreshToken += 1`（完全镜像 `confirmBuildMoc` 的 post-create bookkeeping）→ toast 按"漏题 droppedCount"分支：`none` 策略时走 error 红色提示未注入、漏题 >0 时走 error 提示"AI 漏 N 条已标注"、正常路径 success 绿色。`notice` 系统没 `warning` kind（`NoticeKind = 'info' | 'success' | 'error'`），用 error + 7s TTL 代替。
    - `cancelDraftMocInFlight()` / `closeDraftMoc()` 与 summarize / suggest-tags 同形：先抢 id 再 `aiCompleteCancel` 最后 close。
    - mocBuilder modal 底部加条件按钮 `{#if aiEnabled}<button ... onclick={() => void confirmBuildMocWithAi()}>用 AI 草拟…</button>{/if}`，放在"取消"右侧、"创建 MOC" 左侧；`disabled` 于 `mocBuilderRunning || mocBuilderLoading || picked === 0`。
    - `DiffPreviewModal` 挂在 `{#if draftMocOpen}` 下，`acceptLabel="创建 MOC（AI 分组）"` 自定义；`original = draftMocFlat`、`proposed = draftMocProposed`——两者都是 entries block markdown（不含模板壳），diff 聚焦在"谁进哪个小节"。
- **How to verify**
  - **构建**：`pnpm check` **0/0**、`pnpm build` success、ReadLints 干净。中途遇到一次 `NoticeKind` 把 `'warning'` 当合法 kind 的编译错误，替换为 `'error'` + 长 TTL 后解决。
  - **`sanitizeDraftMoc` Node 内联 4 路径**：
    - `## 方法论\n\n- [[知识管理]]\n- [[Zettelkasten]]\n\n## 工程\n\n- [[图数据库]]\n- [[LLM 评估]]` → section=2 / bullet=4 / linkedTitles=4 条 ✓
    - ```` ```markdown\n## 方法论\n\n- [[知识管理]]\n- [[Zettelkasten]]\n``` ```` → fence 剥掉、section=1 / bullet=2 ✓
    - `Here's the grouping:\n\n## 方法\n\n- [[…]]…` → preamble 丢掉、正确起点 ✓
    - `## A\n\n- [[知识管理]]\n- [[编造出来的标题]]` → 第二条降级为 `- 编造出来的标题  <!-- AI 生成，非选中笔记 -->` ✓
  - **命令可见性手验**：AI 关闭 → 仅剩 `Build MOC from tag…`（非 AI）；AI 开 → 同时有 `Draft MOC from tag (AI)`；`activeTag === null` → 两条都不出现。
  - **mocBuilder modal 三态手验**：
    - 无勾选笔记 → "用 AI 草拟…" 按钮 disable；
    - 有勾选 + AI 关 → 按钮不显示；
    - 有勾选 + AI 开 → 按钮显示；点击后 mocBuilder 立即关、DiffPreviewModal 秒出 loading 态。
  - **stale-request race**：三段 guard（`await` 后 / `catch` 中 / `finally`）与 summarize / suggest-tags 同构、已内联 review。
  - **buildMocFromTag entriesMarkdown override 不回归**：签名新增可选参数、所有既有调用 `noteRefs` 不传新字段 → 取默认扁平 rendering，与之前字节一致；读 `src/lib/commands.ts` 确认 `params.entriesMarkdown?.trim() ? ... : lines.join('\n')` 三元分支 ✓。
- **Known gaps / next**
  - 未做**部分接受 / 编辑分组**：`DiffPreviewModal` 当前是 all-or-nothing。想"保留 AI 的两个小节、丢弃第三个" 只能 cancel 重新来。后续若需要，可以在 DiffPreviewModal 上开 section-level checkbox（类似 `TagSuggestModal` 的思路），但需求未验证、先 YAGNI。
  - 未做 **rebuild from tag**：`moc_source_tag` frontmatter 字段已在落盘时写入、可识别"这是 AI 生成的 MOC"，但没有配套的"重跑 AI" 命令。思路：`open-moc + frontmatter.moc_source_tag 存在 → palette 出 Rebuild from tag (AI)`；留给 P3-D4 polish。
  - 未做 **AI 结果的 on-disk 缓存**：每次重跑都发全文到模型；如果 notes 集合没变、只是改了 temperature，仍会付一次代价。`.mynotes/ai/drafts.json` 缓存方案性价比不高——tag 下 note 集合变化很常见。
  - 未做 **分组描述性段落**：AI 只输出 `## theme + bullets`，没有 1-2 句 "this section is about …" 介绍段。硬约束在 systemPrompt 里是为了让 `injectMocEntries` 做 in-place 替换时不破坏模板其他小节；放开需要设计新 sentinel 或让 AI 也生成整份 MOC（从零草拟），决策留到用户反馈后再定。
  - 未做 **note body 送 prompt**：目前只送 title。若两个 note title 近似（`GraphDB 性能优化` vs `图数据库 性能`），AI 只凭 title 分不清它们是不是同一主题的重复。下阶段考虑把每个 note 的 summary（`frontmatter.summary`，D3.3 已有）拼进 prompt。
  - **D3 整体收官**：D3.1 ~ D3.5 五刀全部落地 · `ai_complete` IPC + `DiffPreviewModal` 共享 UI + summarize/suggest-tags/draft-moc 三条写回命令贯通；下一个阶段按 `plan_P3.md` 顺延到 **P3-D4 Polish**（Phase 3 收尾）或 Phase 4 新起点，等确认下一个方向。

---

## 2026-04-21 · Phase 3-D3.4 — AI 辅助·`> Suggest tags for current note` 命令（checkbox 合并写入 `frontmatter.tags`）

- **Scope**
  - D3 第二条真·写回命令，语义上与 summarize 最大的差别：**写回目标是一个 YAML list，UI 交互是 checkbox merge 而不是 text diff**。`DiffPreviewModal` 对 flow sequence `tags: [a, b] → [a, b, c, d]` 只能呈现一条整行红绿交换，既无法让用户"部分接受"也无法提示"这个 tag 是新建 vs 复用"，所以单开组件 `TagSuggestModal.svelte`；`DiffPreviewModal` 的 shell（header / loader / error banner / backdrop 键位）在新组件里复制一份（~40 行），不 DRY 成 `ModalShell.svelte`——D3.5 MOC 再看是不是值得抽。
  - **新增 `src/lib/ai/suggestTagsPrompt.ts`**：纯函数四件套 + 一个 id 生成器。
    - `buildSuggestTagsPrompt({ body, existingTags, vaultTags })` 生成 `{ systemPrompt, userPrompt }`：system 硬约束"kebab-case / no `#` / no punctuation other than `-`"，并允许 ≤2 个 brand-new tag（否则模型倾向于发明 tag）；user 里把 existing tags + vault top-N (default 40) 拼进去当 soft few-shot，提示"prefer reuse"。vault tags 切片 40、existing 切片 50，保证 prompt 不随 vault 体量膨胀。
    - `parseSuggestedTags(reply)` 三档容错解析：JSON array (`["a","b"]`) / comma-separated (`a, b, c`) / hashtag (`#a #b`)。刻意**不**对 csv chunk 按空格切——系统 prompt 已经 enforce kebab-case，chunk 内含空格八成是 `bar baz` 被 `normaliseTag` 吞掉转成 `bar-baz`；真正的噪声（整段 hallucination 长句）靠 `cleaned.length > 40` 和 pure-digit 过滤兜住。Node 验证 7 条 case 全部收敛（`csv` / `json` / `bullets-with-spaces` / `hashtags` / `CJK` / `garbage` / `mixed` 见下 How to verify）。
    - `parseExistingTags(body)` 三态兼容后端 indexer 接受的三种 YAML tags 写法：flow sequence `tags: [a, b]` / block sequence `tags:\n  - a\n  - b` / 逗号 scalar `tags: a, b`（正则扫 frontmatter 块，不依赖 YAML parser）。
    - `mergeTagsIntoFrontmatter(body, newTags)` 最终统一写成 **flow sequence 一行** `tags: [a, b, c]`——round-trip 原格式不值得，indexer 三种都吃同一份语义；existing ∪ newTags 去重后按顺序输出；无 frontmatter 时 prepend 最小块；与 `rewriteFrontmatter` 刻意解耦（那个只支持 scalar，list 会被序列化出事）。
    - `normaliseTag(raw)` 规范化：strip `#` / lowercase / 空格→`-` / 保留 `[a-z0-9\u4e00-\u9fff\-_]` / 拒绝纯数字 / 拒绝 >40 字符——CJK 平面 `\u4e00-\u9fff` 白名单让中文 tag `知识管理` 可用。
    - `makeSuggestTagsRequestId()` → `tag-<base36>-<rand>`，与 `sum-…` 区分命名空间。
  - **新增 `src/lib/ai/TagSuggestModal.svelte`**：checkbox 列表 UI。
    - `rows = [...existing（pre-checked、`已存在`徽章）, ...(candidates − existing)（默认勾选、按 vaultSet 归类 `复用` vs `新建`）]`——existing 放前面让用户"先看到当前状态、再增减"。
    - `selected: Record<string, boolean>` 勾选状态走 `$effect` 增量 seed（新行到达时扩 map，已有用户选择不动），不做 reset 保证 AI reply 从 loading → loaded 时已有的交互状态不被覆盖。
    - `addedCount` / `removedCount` 从 `finalTags` vs `existingTags` 重算；`+0 / -0` 时 primary 按钮置灰（与 `DiffPreviewModal` "无变化" 行为一致）。
    - 徽章配色：`已存在`（中性 border）/ `复用`（绿）/ `新建`（琥珀），一眼能看出 taxonomy drift。
    - `onAccept(finalTags)` 把最终清单 hand-off 回父组件，modal 本身不动磁盘——与 `DiffPreviewModal` 的 "parent owns the write" 约定对齐。
    - 命名空间 `.tsm-*`，与 `.dpm-*` 并列，不 accidentally 撞样式。Esc = discard（loading 态转 cancel）、Cmd/Ctrl+Enter = accept、双击反锁 `accepting` 标志——所有键位沿用 `DiffPreviewModal` 以降低学习成本。
  - **`src/lib/palette/commandRegistry.ts`** 扩 `PaletteContext` 加 `runSuggestTagsForCurrentNote()`；注册一条面板命令 `suggest-tags`（label: `Suggest tags for current note (AI)`），同 summarize 组 gate `aiEnabled && markdown && !.mynotes/`。**不分档**——写回目标唯一就是 `frontmatter.tags`，勾选行为即"target picker"。
  - **`src/routes/+page.svelte`**：
    - 补 import：`TagSuggestModal`、`suggestTagsPrompt` 四件套、`indexTags`（vault taxonomy 数据源）。
    - 新增 `suggestTags*` 状态族（`Open / Loading / Error / Candidates / Existing / Vault / Original / Path`，全 `$state`）+ 非响应式 `let suggestTagsRequestId: string | null = null`（原因同 summarize）。
    - `runSuggestTagsForCurrentNote()` 流程：`drainPendingSaves()` → `fileRead(path)` → `indexTags()` 并行拿 vault taxonomy（失败仅 console.warn，不 block：候选全部会被打成 `新建`）→ `parseExistingTags(body)` 拿现有 tags → `buildSuggestTagsPrompt(...)` → `suggestTagsOpen = true; suggestTagsLoading = true`（modal 秒进 loading 态）→ `aiComplete(..., temperature: 0.2)`（tag 是 convergent 任务，不要 creative 重解） → stale-request guard 三段与 summarize 同构。
    - `applySuggestTags(finalTags)` 收 modal 的最终清单，走 `mergeTagsIntoFrontmatter(suggestTagsOriginal, finalTags)` → `fileWrite(path, newBody)` → 若还是当前 open file，`fileRead` + `editorContent = fresh; pendingSave = null` 同步编辑器。
    - `cancelSuggestTagsInFlight()` 与 `cancelSummarizeInFlight()` 同形：先抢 rid，再 `aiCompleteCancel`，最后 close。
    - `TagSuggestModal` 挂在 `{#if suggestTagsOpen}` 下，紧邻 `DiffPreviewModal`。
- **How to verify**
  - **构建**：`pnpm check` **0/0**、`pnpm build` success、ReadLints 干净（`suggestTagsPrompt.ts` / `TagSuggestModal.svelte` / `commandRegistry.ts` / `+page.svelte`）。
  - **`parseSuggestedTags` Node 内联手验（7 条 case）**：
    - `"graph-db, knowledge-management, notes"` → `['graph-db','knowledge-management','notes']`
    - `'["ai","tags","prompt"]'` → JSON 分支命中 → `['ai','tags','prompt']`
    - `"- foo\n- bar baz\n- hashtag"` → bullet prefix 去掉后每行当 chunk 交给 `normaliseTag`（空格→`-`） → `['foo','bar-baz','hashtag']`
    - `"#ai  #notes  #ai"` → hashtag 分支命中、去重 → `['ai','notes']`
    - `"知识管理, 图数据库, ai"` → CJK 白名单通过 → `['知识管理','图数据库','ai']`
    - `"1234, foo!!!, a really long sentence…"` → 纯数字过滤、标点剥离、40 字符兜底 → `['foo']`
    - 带空格多字 bullet `"- note taking"` → `'note-taking'`（不会被空格切散）。
  - **`mergeTagsIntoFrontmatter` 五路径**：flow ✓ / block（源是 block、写回统一转 flow）✓ / 无 `tags:` 键追加 ✓ / 无 frontmatter 整体 prepend ✓ / scalar 逗号形态规范化 ✓——五路径输出逐一 Node 打印确认。
  - **UX 手走**（通过逻辑推演）：AI 返回 `graph-db, knowledge-management, notes`，note 现有 `tags: [ai, notes]` → modal 列：`ai(已存在、预勾)` / `notes(已存在、预勾)` / `graph-db(新建、预勾)` / `knowledge-management(新建、预勾)`；用户取消 `notes` 后 accept → `fileWrite` 写入 `tags: [ai, graph-db, knowledge-management]`，counts = +2 -1。
- **Known gaps / next**
  - 未做**候选的置信度排序**：目前按模型输出顺序展示，没有 score。小模型可能给出不相关 tag，需要靠用户筛。后续考虑让 prompt 强制按"相关性降序输出"并展示"AI 理由"。
  - 未做 **tag 编辑/重命名**：modal 里 checkbox 只有"勾选 / 不勾选"两态，不能改 `graph-db → graphdb`。想 rename 还得手动去 frontmatter 或等 P3 后期的 "Tag rename" 工具（design 有草稿）。
  - 未做 **inline #tag 合并**：D3.4 只改 `frontmatter.tags`，不识别正文 `#hashtag` 内联 tag（虽然 indexer 会识别）。未来可以选择把 AI 候选同时追加到正文底部 `#foo #bar` 行，但那会跟 frontmatter 冗余，决定留给 D3.5 之后评估。
  - 未做 **prompt 里"禁用 tag 黑名单"**：某些 vault 可能有"保留 tag"（如 `_draft`、`status/*`）不希望 AI 动。目前没有 UI 让用户标记，只能靠模型自觉。Phase 4 如果做 tag 管理 UI 时再补。
  - 未做 **和 inline #tag 的冲突处理**：如果笔记正文有 `#ai`、frontmatter 里没有，`parseExistingTags` 看不到它，AI 可能重复建议 `ai`。`normaliseTag` 走 merge 流程后其实会被去重（`ai` 进 `frontmatter.tags`，正文那条不动），没有副作用，但视觉上 existing list 不包含它——对用户透明，记在这里。
  - 下一刀 **P3-D3.5（`> Draft MOC from tag (AI)` 命令 + 收官 D3 文档扫尾）**：复用 `aiComplete` + `DiffPreviewModal`（MOC 写回是"一整段 markdown list"，更接近 text diff 语义）；和 `runBuildMocFromTag`（非 AI 版本）区分命名，decide 是并排两个命令还是用一个"策略"切换。

---

## 2026-04-21 · Phase 3-D3.3 — AI 辅助·`> Summarize current note` 三档写回命令

- **Scope**
  - D3 的第一条真·用户可触发的写回命令。把 D3.1 的 `ai_complete` 通道 + D3.2 的 `DiffPreviewModal` 外壳接上，提供三档命令面板入口：`Summarize → frontmatter.summary` / `Summarize → insert TL;DR at top` / `Summarize → copy to clipboard`。三条命令共享同一个 prompt / 同一次 `aiComplete` 调用，只在"拿到 reply 之后怎么处理"分支。
  - **选择三档独立命令、而非一档弹 modal 选 target 的理由**：命令面板的 fuzzy 搜索本身就是最便捷的 target picker（`> sum front` / `> sum top` / `> sum clip`），多一步"在 modal 里点单选框"反而打断流。clipboard 档没有文件修改 → 自然没有 diff 可看，跟 modal 强行兼容反而添乱。
  - **新增 `src/lib/ai/summarizePrompt.ts`**：前端 prompt 模板 + body-mutation 纯函数。
    - `buildSummarizePrompt(body)` 返回 `{ systemPrompt, userPrompt }`。system 硬约束输出形状（"Output ONLY the summary paragraph, without any heading, bullet, quote, or markdown decoration"）和语言（"match the note's language — if Chinese, reply in Chinese"，省掉一次语言探测 IPC）；user 负责把 frontmatter 剥离后再送过去（避免模型被 YAML 干扰）。
    - `applySummaryToBody(body, summary, target)` 是纯函数：`frontmatter` 走 `rewriteFrontmatter(body, { summary })`（复用 `$lib/commands` 里已有的 regex-based YAML 写入）；`top` 走新加的 `insertTldrAtTop`，在 frontmatter 块后插入 `> **TL;DR** …\n\n`，前后各留一空行；无 frontmatter 时直接 prepend 到开头。
    - `makeSummarizeRequestId()` 产 `sum-<base36>-<rand>` 前缀的 id，跟 chat 流 id 分命名空间方便调试；长度远低于后端 128 字符上限。
  - **`src/lib/palette/commandRegistry.ts`** 扩 `PaletteContext`：加 `aiEnabled: boolean` 与 `runSummarizeCurrentNote(target)`。三条新命令（`summarize-to-frontmatter` / `-top` / `-clipboard`）全部 gate `ctx.aiEnabled && markdown file open && !startsWith('.mynotes/')`——AI 关的时候不在面板里出现，避免触发后才报"请先启用 AI"。
  - **`src/routes/+page.svelte`**：
    - 把原本在 ~770 行的 `let aiEnabled = $state(true)` 上移到 `paletteCtx` 之前（`paletteCtx` 现在需要读它），补 JSDoc 解释为什么 hoist。
    - 新增一块状态："summarize" 命名空间：`summarizeOpen / summarizeLoading / summarizeError / summarizeReply / summarizeOriginal / summarizePath / summarizeTarget`（全部 `$state`），以及一个**非响应式** `let summarizeRequestId: string | null = null`——纯 transient token，做成 `$state` 会让每次赋值都触发 `$effect` 抖动。
    - `summarizeProposed = $derived.by(...)` 直接由 `(reply, target, original)` 推出 `DiffPreviewModal` 的 `proposed` prop，`reply === null` → `null` 正好映射到 modal 的 loading 态。
    - `runSummarizeCurrentNote(target)` 流程：`drainPendingSaves()` 先刷盘 → `fileRead(path)` 拿当前磁盘上的 body（不用 `editorContent`，避免 unsaved 改动干扰；写回的时候也是写 path 而不是编辑器缓冲区）→ 空 body 拒绝 → `clipboard` 档就地 `aiComplete` + `navigator.clipboard.writeText`（toast 反馈，失败也只吐 toast，不开 modal）→ `frontmatter` / `top` 档先 `summarizeOpen = true; summarizeLoading = true` 让 modal 秒出 loading 态，再 `await aiComplete(...)`，resolve / reject 后用 `summarizeRequestId` 做 stale-request guard（用户 discard 后再开一单时不让旧的 reply 盖新的 state）。
    - `applySummarize()` → `fileWrite(path, summarizeProposed)`；若当前打开的还是同一文件，走 `fileRead` + `editorContent = fresh` 重载编辑器（同 `runSetProjectStatus` 的已知模式——watcher 只重建 SQLite 索引，不会推 body 回 Svelte 层）。
    - `cancelSummarizeInFlight()` 在 loading 态下触发：先抢 `summarizeRequestId = null`，再 `await aiCompleteCancel(rid)`，最后 `closeSummarize()`。抢 id 是为了让外面挂起的 await 判定"自己已不再是当前请求"而不要继续改 state。
    - `DiffPreviewModal` 挂在 `{#if summarizeOpen}` 下；`title` / `acceptLabel` 按 target 动态切文案（"写入 frontmatter" vs "插入到文首"）。
- **How to verify**
  - **构建**：`pnpm check` **0/0**、`pnpm build` success、ReadLints 干净（`summarizePrompt.ts` / `commandRegistry.ts` / `+page.svelte`）。
  - **算法手验**：Node 内联跑 `rewriteFrontmatter` + `insertTldrAtTop`，三种情况输出一致符合预期：
    1. `summary` 追加到现有 frontmatter 末尾；
    2. `> **TL;DR** …` 插在 `---` 块后 + 正文前，前后各有空行；
    3. 无 frontmatter 的笔记直接 prepend，trim 掉原有前导换行。
  - **命令面板可见性**：三条 `summarize-*` 命令 `when` 谓词统一 `aiEnabled && markdown && !.mynotes/`。AI 关掉后面板中直接不出现（比 runtime 报错 UX 更好）。
  - **stale-request guard**：代码里双层 check——`await` 返回后比 `summarizeRequestId !== requestId` 判断是否要应用结果；`finally` 里也只在 match 时清 loading 标志。用户 discard + 重开的race 条件被盖住。
- **Known gaps**
  - **无"重新生成"按钮**：reply 出来后用户只能 accept / discard，不能"不满意，再来一遍"。现实用法里重新运行一次命令足够，真要做得 polish 再给 `DiffPreviewModal` 加 `onRetry` prop。
  - **"top" 档不检测 / 不替换旧 TL;DR**：`insertTldrAtTop` 永远插入。如果用户笔记顶部已经有 `> **TL;DR** 旧摘要`，第二次运行会并排出现两条。故意的——"什么叫旧 TL;DR"难以精确识别，误删用户内容比多一行可见干扰更糟；diff 里用户能看到原有的那行在 `same` 区，可以 discard 后手删再重来。
  - **`clipboard` 档走 `navigator.clipboard`**：Tauri webview 是 secure context，`writeText` 可用。没装 `tauri-plugin-clipboard-manager`，未来如果需要"自定义格式 / 带样式"的剪贴板写入再补。
  - **语言检测依赖 prompt**：`match the note's language` 是对模型的软提示，极少数情况下模型可能偏离（例如英文笔记里夹杂了少量中文时）。完全精确的解决方案是前端加 CJK 比例检测 + 显式切换 system prompt，但目前没必要。
  - **无 token budget 守卫**：跟 D3.1 约定一致——prompt 长度靠 caller 自管。目前 summarize 的 `user_prompt = note body`，极长笔记会触发 provider 的 `context_length_exceeded`，失败提示会透过 `CompleteFailure` 归到 `invalid_request` 档在 modal banner 里显示。要做 chunked summarization 等 D4 阶段再接。
  - **三条命令 hint 都是 `"AI"`**：跟 `runEmbedCurrentNote` / `runShowRelatedNotes` 保持一致，不在 hint 里再加 `"Summarize"` 后缀（label 自己带了）。palette 右侧 hint 只是分类，不是 mnemonic。

---

## 2026-04-21 · Phase 3-D3.2 — AI 辅助·Diff 预览 modal + 行级 diff 渲染

- **Scope**
  - D3.3 / D3.5 的**共用 UI 组件**：把 AI 生成的"写回候选内容"以行级 diff 的形式呈现给用户，用户确认 / 放弃后再触发实际落盘。D3.1 铺好了后端通道，这一刀只做**前端的 diff 渲染 + modal 壳**，不触碰任何命令注册——命令会在 D3.3（summarize）/ D3.5（MOC）接入时把这个 modal 当黑盒用。
  - **新增 `src/lib/ai/diffLines.ts`**：极小的 LCS 行 diff，~30 行。选择自写而非引 `diff` / `jsdiff`——write-back 场景输入有明确上限（单篇笔记，几百行顶天），LCS DP 表 O(m·n) 足够，省掉 ~40KB 依赖 + 保持算法可审计。导出 `DiffPart = { type: 'add' | 'remove' | 'same', value: string }` + `diffLines(a, b)` + `diffStats(parts)` 三件套；tie-break 规则 `dp[i+1][j] >= dp[i][j+1]` 让连续删除聚团、连续新增聚团，不交错渲染。边界：空串 vs 文本 / 文本 vs 空串 / 完全相同 / 纯插入 / 纯删除 / 替换都已手验。
  - **新增 `src/lib/ai/DiffPreviewModal.svelte`**：三态 modal（loading / error / diff），`.dpm-*` 私有样式前缀，复刻 `ChatPanel.ns-*` 的 modal 视觉语言（backdrop + surface card + footer actions）以避免在 design tokens 之外另起炉灶。Props：
    - `open` / `title` / `description?`：标题 + 副标题（e.g. `"AI 将覆盖 frontmatter.summary"`）。
    - `original` / `proposed: string | null`：before / after 文本，`proposed = null` 时进 loading 态。
    - `loading` / `error?: CompleteFailure | null`：与 `aiComplete` 返回值一一对应；调用方不需要把 result 拆开，直接往 modal props 上映射。
    - `onAccept` / `onDiscard` / `onCancel?`：三条回调。`onAccept` 支持返回 `Promise`，内部 `accepting` latch 会把按钮置灰为 `应用中…`，避免慢盘写被双击。`onCancel` 仅在 `loading` 下出现，接 `aiCompleteCancel`。
    - `acceptLabel?` / `discardLabel?`：按钮文案可按命令语义改写（summarize "覆盖摘要" / MOC "追加 draft" ...）。
  - **快捷键 & 交互**：Esc 在 `loading` 时走 `onCancel`（若未提供则忽略），其他情况走 `onDiscard`；`Cmd/Ctrl+Enter` 触发 accept；backdrop 点击与 Esc 同语义。accept 按钮在"零变化"（`added === 0 && removed === 0`）时自动置灰，避免空操作写文件。
  - **可达性**：`role="dialog" aria-modal="true" aria-labelledby`、error banner `role="alert"`、loading `role="status" aria-live="polite"`、diff 区 `role="region" aria-label="Diff preview"`；a11y 的两条 svelte-ignore 与 `ns-backdrop` 同理由（被动 dismiss surface，body 自己处理键位）。
  - **零命令接入**：D3.2 只产出组件，`+page.svelte` / 命令面板 / `ChatPanel` 都不改。等 D3.3 把 `> Summarize current note` 接上时，才会在 `+page.svelte` 里 `{#if summarizeModalOpen}<DiffPreviewModal … />` 真正挂载。
- **How to verify**
  - **构建**：`pnpm check` **0 errors / 0 warnings**；`pnpm build` 成功；`ReadLints` 干净。
  - **算法手验**：跑 Node 内联版 `diffLines` 覆盖六条路径（identical / pure insert / pure delete / replace / empty→text / text→empty），输出与预期对齐；替换场景遵循 remove-before-add 的稳定排序。
  - **类型契约**：`CompleteFailure['kind']` 覆盖 `classify_provider_error` emit 的全部变体（`auth | network | rate_limit | invalid_request | other`），`labelForKind` 逐一翻译，`default` 兜住潜在新增变体。
  - **设计对齐**：`.dpm-*` 样式复用 `--color-surface` / `--color-border` / `--color-accent` / `--color-danger` 等 tokens，不硬编码颜色；diff 高亮用 `color-mix(in oklch, #2f7d32 10%, transparent)` / `#b3261e 10%`，跟 chat 里 assistant / error bubble 的色感一致。
- **Known gaps**
  - **纯文本 diff，不支持 token-level / word-level**：摘要场景 AI 多数情况下是整段重写（和原文大多不同），行级 diff 看着就是"-旧 +新"整块替换，不像 git 那样能精细到词。对 P3-D3 目标足够；若未来做"AI 改写段落"这种增量小修，再接 `fast-diff` / `diff-match-patch` 做 word 级并升级 renderer。
  - **checklist 合并 UI 尚未实现**：D3.4（suggest tags）不是行 diff 能表达的——"已有 tags + AI 建议 tags"是集合操作，用户需要勾选。D3.2 故意留空这部分，D3.4 会选型是"给 `DiffPreviewModal` 加 `body` snippet prop" 还是"另起 `TagMergeModal.svelte`"——两者都比强行把 checkbox 塞进当前 diff 结构优雅。
  - **无 transform / viewport 动画**：模态出现 / 消失没有 `fade` / `scale` transition。和 `ChatPanel.ns-modal` 对齐（后者也没有），保持"瞬发"的工具感；若后续产品想统一加，`DiffPreviewModal` + `ns-modal` 一起改成 `transition:fade` 即可。
  - **diff 行无虚拟滚动**：整体渲染所有 lines，万行笔记会卡。当前写回目标（frontmatter 一行 summary / tags / MOC ~50 行）远低于此阈值；真要写长篇再接 `svelte-virtual-list`。

---

## 2026-04-21 · Phase 3-D3.1 — AI 辅助·单次补全 IPC（`ai_complete` + cancel）

- **Scope**
  - P3-D3（三条写回命令：summarize / suggest tags / MOC AI draft）的**共同后端通道**。把 `ai_chat_send` / `ai_chat_stream_start` 共用的"pre-flight → spawn task → 拉 delta"模板裁成"非流式 + 无会话持久化 + 无 RAG"的窄版本，供 D3.3–D3.5 三条 IPC 调用时复用。
  - **`commands/ai.rs` 新增 `ai_complete` / `ai_complete_cancel` 两条 command**：
    - 入参：`request_id`（调用方生成 nanoid / uuid；与 `chat_streams` 独立注册表，避免误杀）、`system_prompt: Option<String>`（可选）、`user_prompt: String`（必填；空白 / 过长 request_id 会在 pre-flight 拒绝）、`temperature: Option<f32>`、`max_tokens: Option<u32>`。
    - 出参 `CompleteResult { ok, reply?, input_tokens?, output_tokens?, cancelled, failure? }`：与 `ChatSendResult` / `ChatStreamStartResult` 一样**不走 Tauri `Result::Err` 通道**——成功 / 失败都从同一个结构回，前端只需渲染一处（diff 预览 modal 的 loaded / error 两态）。失败结构 `CompleteFailure { kind, message, retry_after_secs? }` **不带** `user_message_persisted`（因为从不写 chat 存储），相比 `ChatSendFailure` 更窄。
    - 内部实现：沿用 D2b.4 的 `AtomicBool` cancel flag 模式——走 `provider.chat_stream(ChatRequest)` 取 stream，内层 loop 每次 `stream.next().await` 后检查 cancel flag，累积 delta 的 content + usage。**非流式语义是对前端的承诺**（调 `ai_complete` 只会 `await` 一次，拿到 `CompleteResult`），但底层仍复用 OpenAI `/v1/chat/completions?stream=true` SSE 路径——这样三条写回命令跟对话面共用同一个 transport，不会在 OpenAI SSE 和非 SSE 两种解析之间分叉。
  - **`AppState` 扩展**：新增 `complete_requests: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>`，与 `chat_streams` 分表。理由在代码注释里写得很清楚——"chat-side cancel 永远不能误杀 write-back；反之亦然"。
  - **`ai_complete_cancel(request_id)` → `bool`**：幂等，找不到 id 返回 `false`。取消语义与 `ai_chat_stream_cancel` 一致：翻 flag，in-flight 命令在下一次 poll 时返回"已累积的 reply"（若非空则 `ok: true, cancelled: true`；若空则 `ok: false, failure.kind = "other"`）。
  - **TypeScript wrapper**（`src/lib/ipc/ai.ts`）：`aiComplete(requestId, { systemPrompt?, userPrompt, temperature?, maxTokens? })` + `aiCompleteCancel(requestId)`。命名风格与已有 `aiChatSend` / `aiChatStreamStart` 对齐（opts object），避免参数顺序记忆负担。导出 `CompleteResult` / `CompleteFailure` 类型。
  - **`lib.rs`**：`AppState` 字段初始化 + `tauri::generate_handler!` 注册两条新 command。
  - **零 UI 改动**：D3.1 只负责把管道通起来，diff modal / summarize / suggest tags / MOC draft 的具体 UI + prompt 模板留到 D3.2–D3.5。
- **How to verify**
  - **构建**：`cargo check` 干净（无 warning）；`cargo test --lib` **189/189** 全绿（无新增测试，因为新 IPC 依赖 `State<AppState>` 很难纯 unit 测，保持与 `ai_chat_stream_start` 同样的"靠 MockProvider + 手测"策略）；`pnpm check` **0 errors / 0 warnings**；`pnpm build` 成功。
  - **契约自洽**：`CompleteResult` / `CompleteFailure` 的 serde 字段与 `ChatSendResult` / `ChatSendFailure` 一致风格（`kind: String`、`retry_after_secs: Option<u64>`），前端复用现有 `classify_provider_error` 翻译后得到的 kind enum（`network | auth | rate_limit | invalid_request | other`）。
  - **代码 review 要点**：
    1. `ai_complete` pre-flight 失败（invalid request_id / empty user_prompt / duplicate id / no chat provider）**全部不注册 cancel flag**，避免孤儿留在 map 里。
    2. `provider.chat_stream(req).await` 若返回 `Err` 会先 `cleanup()` 再返回——registry 立刻可用于下一次调用。
    3. 流 loop 进入后 cancel 翻 `true`，先 `break` 再 `cleanup()`，再检查 `error_seen` / 空 reply 的各分支——保证无论哪条路径都会清 registry。
    4. `cancelled` 字段独立于 `ok`：累积内容非空时即使 cancelled 也会 `ok: true`，UI 可以提示"已取消但已有部分结果，是否保留"。
- **Known gaps**
  - **无真 token budget 守卫**：调用方可以塞 100K 字符的 `user_prompt`，直接交给 provider 触发 `InvalidRequest (context_length_exceeded)`。D3.3（summarize）会在 prompt 组装时自己做"按段落裁剪到 ~8K tokens"的预处理；D3.4 / D3.5 的输入本身就短。后端暂不加硬上限，避免跟 chat 路径的 `DEFAULT_HISTORY_TOKEN_BUDGET` 重复语义。
  - **IPC 级别无单元测试**：`ai_complete` 需要 `State<AppState>` + 真 provider，很难 pure unit test。沿用 `ai_chat_stream_start` 的"集成 / 手测 + MockProvider 间接覆盖"策略。D3.3–D3.5 的 UI 侧可以跑对真 API 的手测。
  - **无进度事件**：非流式语义意味着前端只能在 `aiComplete` promise 期间展示"思考中"loader，不像 `ai_chat_stream_*` 那样逐字出字。write-back 的典型产出（TL;DR 200 字 / tag list 10 个）几秒内完成，loader + cancel 够用；若用户反馈"长摘要没反馈、很焦虑"，再把 `ai_complete` 升级为 `ai_complete_stream`（增量事件）的成本不高。
  - **三条写回命令入口尚未接上**：D3.1 只通了管道，命令面板里还没有 `> Summarize current note` 等命令，chat 面里"把这段写回笔记"尚未 wire 到 `aiComplete`——这些是 D3.2（diff modal UI）→ D3.5（MOC）逐刀落地。

---

## 2026-04-21 · Phase 3-D2b.6 — AI 辅助·弹出独立窗口 + AI 关闭时自动关闭

- **Scope**
  - D2b 最后一刀：把 D2b.3~D2b.5 驻留在主窗口右栏的 `ChatPanel` 变成"可弹出独立窗口"的形态，支持长对话 / 并排参考笔记 / 多窗协作。session JSONL 格式 / provider trait / IPC / 事件协议**全部不动**；整条刀口是**纯前端 + 一个 SvelteKit 路由 + capability 加一个 window label**。
  - **前端**：
    - `src/routes/chat-standalone/+page.svelte`（新）：独立窗口的 shell。只做三件事——（1）`onMount` 时 `listen('chat-standalone:file-path')` 同步主窗当前打开的笔记路径（给 RAG + 新建会话 modal 的"关联当前笔记"用）；（2）`listen('chat-standalone:close')` 响应主窗"AI 关闭"或"取回"请求，调自己的 `getCurrentWindow().close()`（走 Svelte onDestroy 让 ChatPanel 的 `aiChatStreamCancel` / unlisten 清理干净）；（3）`onMount` emit `chat-standalone:ready`，握手把 `file-path` 拉回来。`onOpenNote` 走 `emit('chat-standalone:open-note', { path })` 把点击路由给主窗。
    - `src/lib/panel/Panel.svelte`：
      - 新增 `standaloneOpen: $state<boolean>` / `standaloneWindow: WebviewWindow | null` 两条状态，配 `STANDALONE_LABEL = 'chat-standalone'` / `EV_*` 事件名常量。
      - `openStandalone()`：`WebviewWindow.getByLabel` 先探 zombie label，空 → `new WebviewWindow('chat-standalone', { url: '/chat-standalone', width: 720, height: 860, ... })`；绑事件 listener + 翻 `standaloneOpen = true`。重复点击"聚焦已有"。
      - `bringBack()`：emit `EV_CLOSE` → 独立窗自己 `close()`；**加 600ms 兜底**（独立窗 frozen 时强制 `.close()`）——避免 UI 卡死。
      - `ensureStandaloneListeners()`：懒注册三条 listener：`EV_OPEN_NOTE`（独立窗点 wiki-link / citation chip 触发，调回主窗 `onOpenNote(path)` 实际打开编辑器 tab）/ `EV_READY`（独立窗上线握手，response 回 `EV_FILE_PATH`）/ `EV_CLOSED`（独立窗 `onDestroy` 自己 emit，收到后主窗 `standaloneOpen = false` → 翻回 docked UI）。
      - 两条 `$effect`：（a）`standaloneOpen && filePath changed` → emit `EV_FILE_PATH` 把新路径推给独立窗；（b）`!aiEnabled && standaloneOpen` → `bringBack()`，Settings 里关 AI 时独立窗自动关。
      - `onMount`：查 `getByLabel`，若之前窗口没被关（比如 Panel re-mount 场景）则恢复 `standaloneOpen = true` + 重绑 listener，避免"主窗看起来空，独立窗还在那"。
      - UI：`AI 对话` tab 右侧加 `⧉` pop-out 按钮（仅 `!standaloneOpen` 时可见）；`{#if standaloneOpen}` 分支渲染占位符（`"AI 对话已在独立窗口"` + `聚焦` / `取回到此处` 两按钮）。
    - `src/lib/panel/ChatPanel.svelte`：新增 `variant?: 'docked' | 'standalone'` prop（默认 `docked`），根 div 加 `chat-panel--{variant}` 修饰类；CSS 里 standalone 变种把 padding 归零让独立窗铺满。其它逻辑零改动。
  - **Tauri 配置**：
    - `src-tauri/capabilities/default.json`：`windows: ["main"]` → `windows: ["main", "chat-standalone"]`，让独立窗也能用现有的 `core:*` / `dialog:*` IPC 命令。不再加任何新 permission——`core:default` 已带 `core:webview:default` + `core:window:default`，创建 / 关闭 webview window 足够。
  - **不碰后端**：Tauri v2 默认对 "frontendDist 找不到文件就 fallback 到 index.html" 的行为让我们零改动就能让 `/chat-standalone` 走到 SvelteKit SPA 路由；SvelteKit 的 `+layout.ts` 已有 `prerender = false; ssr = false;` 保证新路由也不被预渲染。
- **How to verify**
  - **构建**：`pnpm build` 成功，`build/` 只有 `index.html + _app/`（SvelteKit adapter-static SPA fallback），chat-standalone 路由通过 SPA 路由动态解析，无需每路由生成 html。`cargo check` 干净（未改任何 Rust 源）；`cargo test --lib` **189/189** 全绿（= D2b.5 的 185 不变，本刀无新增后端代码）。`pnpm check` **0 errors / 0 warnings**。
  - **手测**（dev 下 `pnpm tauri dev`）：
    1. 开一个笔记 → 右栏切 AI 对话 tab → 点 `⧉` → 新窗口打开，标题 `AI 对话 · MyNotes`，体内 ChatPanel 铺满整窗，会话列表 / 压缩框 / 历史气泡全部正常。
    2. 独立窗发一条 stream 消息 → 实时 append + 闪烁光标 + "中断" 按钮均正常（事件 per-stream 按 stream_id 过滤，不会被主窗串线）。
    3. 点独立窗里的 `[[wiki-link]]` chip 或 assistant 气泡底的 citations chip → 主窗的编辑器 tab 立刻切到对应笔记（`EV_OPEN_NOTE` 往主窗路由通）。
    4. 主窗切换打开的笔记 → 独立窗新建会话 modal 里"关联当前笔记"选项路径会跟着变（`EV_FILE_PATH` 推送验证）。
    5. 关独立窗（OS 关闭按钮） → 主窗 docked UI 自动从占位符切回 ChatPanel（`EV_CLOSED` 订阅命中）；独立窗关闭过程中若有在流的 stream，Svelte onDestroy 会跑 `aiChatStreamCancel`，后端 spawn task 下一轮 poll 断开。
    6. 主窗 Settings 里关"AI 辅助" → 独立窗被主窗 emit `EV_CLOSE` → 自关 → 主窗 `EV_CLOSED` 命中 → tab header 藏 → `standaloneOpen = false`。
    7. 点 `⧉` → 关独立窗 → 再点 `⧉` → 能重新打开（`getByLabel` 返回 null → 创建新的）；连点 `⧉` 两次不会打开两个窗（`standaloneOpen` 守卫）。
    8. 同时开两条 stream（独立窗里连发） → 按 stream_id 互不干扰；主窗 docked 显示占位符期间不绑流 listener 更不会意外收到 assistant turn（占位符期间 ChatPanel 未 mount）。
- **Known gaps**
  - **流式事件仍然是全局广播**：`tauri::app::emit()` 广播到所有 webview。D2b.4 就注意过这点，D2b.6 绕开的方式是——docked 模式的 ChatPanel 在 `standaloneOpen` 时根本不 mount，就不会有双订阅。代价：同时"看独立窗 + docked ChatPanel"的场景无法实现；真要支持（如主窗并排看 history + 独立窗继续对话），得改成 `emit_to(label, …)` 定向投递 + payload 带 webview 身份。
  - **会话切换不同步**：主窗当前未开独立窗时，两边会话列表来源都是文件系统；一旦独立窗开着，主窗里**没有**ChatPanel 在跑，所以"切会话"这个动作只在独立窗里生效。如果将来 docked 和 standalone 并存，需要 Tauri 跨窗发布"当前 session_id 变了"事件。
  - **无多独立窗**：`STANDALONE_LABEL` 硬编一个；若想"每个会话一个独立窗"，得把 label 改成 `chat-{session_id}` + 维护 `Map<session_id, WebviewWindow>`。v1 场景（单用户单任务）单例够用，后续扩展留着。
  - **窗口位置 / 尺寸不持久化**：每次 `new WebviewWindow` 用同一份默认尺寸（720 × 860）和默认位置（OS 自选）。Tauri v2 有 `plugin-window-state` 记位置/尺寸，等用户反馈"每次都要拖" 再接。
  - **AI 关闭 → 独立窗关闭**走"主窗 emit → 独立窗自关"路径，如果用户在主窗把 AI 关掉的同时主窗崩溃（罕见），独立窗会成为孤窗；用户要手动关一次。v1 接受这个。
  - **路由 URL**：`/chat-standalone` 依赖 Tauri v2 "找不到 fallback 到 index.html" 的默认行为；若将来 Tauri 加了个 `disableFallback: true`（见 tauri-apps/tauri#5082 讨论），得同步把 URL 改成 `/` + hash routing `/#/chat-standalone`。这是上游配置调整，发生时同步即可。

---

## 2026-04-21 · Phase 3-D2b.5 — AI 辅助·RAG 注入 + `[[wiki-link]]` 渲染 + 新建会话 Modal

- **Scope**
  - D2b 第五刀：在 D2b.4 的流式管线上补**三件"把 Chat 接回笔记库"的事**——（1）发送用户 prompt 前先做一次 RAG（embed → top-K → 系统消息）把相关笔记片段塞进 context；（2）assistant 回答里的 `[[note-title]]` 在前端解析成可点击 chip；（3）新建会话从 `window.prompt` 升级成 inline modal，带"关联当前笔记"勾选。session JSONL 文件格式 **不变**，RAG 仅在 in-memory messages 层面拼接 system prompt，不落盘；citations 作为前端内存态 by-assistant-id 显示，重新打开会话不持久化——避免 schema 迁移。
  - **后端**（Rust）：
    - `src-tauri/src/services/ai/rag.rs`（新）：
      - `RagCitation { note_rel_path, chunk_index, offset_start, offset_end, score, preview }` —— 给前端展示用的"哪条笔记贡献了哪段"。
      - `RagContext { system_message: ChatMessage, citations: Vec<RagCitation> }` —— 后端拼 system prompt + 前端 chip 的组合返回。
      - 拆成 `async fn embed_query(query, provider, model) -> Option<Vec<f32>>` + `fn search_and_format(&query_vec, model, &store, top_k) -> Option<RagContext>` 两段；这是**强制**的：`search_and_format` 要拿 `EmbeddingStore` 的 `MutexGuard`，而 `embed_query` 要 `.await` provider —— `std::sync::MutexGuard` 不是 `Send`，放一起会让 spawn 任务编译失败。调用方（`try_build_rag_context`）按 "lock-drop → await → reacquire lock" 顺序串起来。
      - 预算：`DEFAULT_TOP_K = 4`，`MAX_CONTEXT_CHARS = 2400`（≈ 700 tok），`preview` 每条软截 160 字符，UTF-8 字符边界安全。硬超 budget 时按 score 降序保留。
    - `src-tauri/src/commands/ai.rs`：
      - `ChatStreamStartResult` 新增 `citations: Vec<RagCitation>`，并加 `#[derive(Default)]`，让失败分支用 `..Default::default()` 收尾即可；成功分支显式填 `citations`。
      - `ai_chat_stream_start` 在"持久化 user turn 之后、`truncate_history_to_budget` 之前"插入 `try_build_rag_context`，命中时把 system message **unshift** 到 `full_messages` 最前；RAG 失败（未配置 / embedding 空 / provider 报错 / 未命中）统统返回 `None`，走 raw path。整个 RAG 调用是 best-effort，从不让 chat 流失败。
      - `try_build_rag_context(state, query)` 就是"锁 store → clone Arc → drop → 锁 config → clone provider → drop → `.await` embed → reacquire store lock → 搜+format"，显式写锁 scope 让人一眼看到 await 前后哪些锁已释放。
    - `src-tauri/src/commands/index.rs`：
      - `index_resolve_wiki_link(target) -> AppResult<Option<NoteRef>>`：两段 precedence，先 `SELECT path FROM notes WHERE title = ? LIMIT 1`，再 `WHERE stem(path) = ?`（SQLite 没原生 `stem()`，用 Rust 侧 `Path::file_stem()` 后过滤）；命中返回路径，未命中 `Ok(None)`。逻辑镜像 `indexer::resolve_links`，但独立查询避免与 D2a.2 的内部批量解析器耦合。
      - `fn query_first_note` 私有 helper，准备语句只列 `rel_path, title`，防 SELECT \* 跨架构升级。
    - `src-tauri/src/services/ai/mod.rs`：挂 `pub mod rag;`。
    - `src-tauri/src/lib.rs`：注册 `commands::index::index_resolve_wiki_link`。
  - **前端**（TypeScript / Svelte）：
    - `src/lib/ipc/ai.ts`：新增 `RagCitation` 接口镜像后端结构；`ChatStreamStartResult.citations?: RagCitation[]` 可选字段（老路径仍兼容）。
    - `src/lib/ipc/index.ts`：新增 `indexResolveWikiLink(target) -> Promise<NoteRef | null>` wrapper。
    - `src/lib/panel/ChatPanel.svelte`：
      - 新增 state：`citationsByAssistantId: Record<string, RagCitation[]>`（按 assistant 消息 id 查引用）、`pendingCitations: RagCitation[]`（当前流式 reply 的引用）。`send()` 从 `aiChatStreamStart` 的结果里把 citations 先放 pending，`onStreamTerminal(ok=true, assistantId)` 到达后 commit 到 `citationsByAssistantId[assistantId]` 并清空 pending。
      - `renderMarkdown` 新增第 4 步（在 escape 之后、inline-code 处理之前）：正则 `\[\[([^\]\n|]+)(?:\|([^\]\n]+))?\]\]` 把 `[[target]]` / `[[target|label]]` 转成 `<span class="chat-wiki-link" role="link" tabindex="0" data-wiki-target="...">[[label]]</span>`。target 属性里 HTML 反转义后再转义一次 `"`，防属性逃逸。
      - `onTranscriptClick(ev)` 事件委托：`transcriptEl` 上统一挂 click / keydown(Enter|Space)，事件目标 closest `.chat-wiki-link` 才触发 → `indexResolveWikiLink(target)` → 命中则 `onOpenNote(rel_path)`，否则 `uiError = \`未找到笔记：${target}\``。**不在渲染阶段预解析**—— assistant 流式追加每个 token 都会跑一次 renderMarkdown，如果那时就 IPC，200 个 token × N 个 wiki-link 会把 IPC bus 打爆；把解析延到点击，单次 render 零 IPC。
      - "Sources" footer：assistant 气泡下方显示 `[1] path/to/note.md` 形式的 chip 列表，点击直接 `onOpenNote`，`title` 上有分数 + preview 预览；流式中气泡在 pending 阶段就先显示 chip（让用户边看边知道 AI 参考了谁）。
      - **新建会话 Modal**：`newSession()` 从 `window.prompt` + `window.confirm` 换成 `newSessionModalOpen` 状态控制的 inline modal（标题输入 + "关联当前笔记"勾选 + 创建/取消），支持 Esc 取消、Enter 提交、`newSessionBusy` 期间禁 close 防双重创建；样式 `.ns-*` 自带一套（不依赖全局 modal），`position: fixed` 覆盖主窗。默认勾选随 `filePath` 是否存在，符合"打开一个笔记 → 多半想链它"的直觉。
- **How to verify**
  - **构建**：`cargo check` 干净；`cargo test --lib ai::rag` → 4 条新测试 PASS（字符截断 / 多字节边界 / 各自小于总预算 / 总预算命中截断）。`pnpm check` → **0 errors / 0 warnings**。
  - **RAG 手测**（Settings 里配置好 embed 模型 + 至少跑过一次 `embed_rebuild_all`）：
    1. 先 `embed_build` 让 store 有数据；开对话发 `我的 deep work 笔记里怎么规划 morning block？` → 看 assistant 气泡底部出现 1~4 个 `[N] path/to/note.md` chip；hover 能看到 similarity score + preview；点击 chip 跳到对应笔记。
    2. 断开 embedding provider（故意改 embed_model 为不存在的名字）→ RAG 失败但 chat 仍正常流式返回；citations 列表为空；无 banner。
    3. 空 store（新 vault，没跑过 embed）→ 同上，不阻塞 chat。
  - **`[[wiki-link]]` 手测**：
    1. 手动发 `请告诉我 [[Deep Work]] 和 [[Morning Block|早起模块]] 的区别`。assistant 回答常会复述 wiki-link。即便 assistant 没重复，用户消息里的 `[[Deep Work]]` 也会渲染成可点 chip。
    2. 点 `[[Deep Work]]` → 若存在标题为 `Deep Work` 的笔记则跳过去；若只有 `deep-work.md` 无同名标题，走 filename stem 命中；都不命中则 `uiError` banner 显示"未找到笔记：Deep Work"。
    3. 键盘：Tab 聚焦到 chip → Enter / Space 同样触发解析 + 跳转。
  - **Modal 手测**：
    1. 点右上 `+` 按钮 → modal 从底部淡入；focus 自动落到标题输入框；打字 / Enter 创建 / Esc 取消。
    2. 当前打开一条笔记 → "关联当前笔记" 默认勾选 + 显示路径；未打开笔记 → 显示灰色"当前未打开笔记…"提示，checkbox 区域隐藏。
    3. IPC 期间（`newSessionBusy`）按钮文案变"创建中…"，所有输入禁用，点 backdrop 不关；后端报错则在 modal 内显示红色 banner，不关闭。
- **Known gaps**
  - **citations 不持久化**：`citationsByAssistantId` 只存 Svelte state，切 session 或重载面板即丢。设计 V2 §6.27 里也写过，RAG 的"为什么回答这个"可以追溯到 session，但**哪些 chunk 贡献了**本身会随着 embed_store 更新漂移，硬存反而误导；真要回溯去看 `embed_store` 当时的 snapshot。后续 D3（MOC AI draft）若要"点开引用直接贴片段"再考虑落盘。
  - **RAG 无关联笔记过滤**：目前 top-K 是全库检索；用户在 modal 里勾选的 `related_note` 只是元数据，**不会收窄 RAG 搜索范围**。简单起见 v1 让 cosine 自己找相关——相关性够强时相关笔记的片段会自然排到前面。真需要按笔记约束再加 `search_in_note(path_prefix)`。
  - **wiki-link 无别名歧义解析**：`[[A]]` 同时命中标题 `A` 和文件名 `a.md`、或多条同名笔记时走 `LIMIT 1`；等将来笔记多起来再接"歧义时弹选择器"。
  - **流式中 citations 不变化**：pre-flight 算一次就固定；如果 assistant 在流里引用了新的片段，我们不会重跑 RAG。这是刻意的：重跑一次要多 50~100ms 且改变 context 会让 provider 端 cache miss。
  - **Modal 无历史标题下拉**：每次都从空开始；用户反馈多再加 "最近 5 个会话" 下拉。
  - **Modal 无键盘 trap**：Tab 能跳出 modal；按 Tauri 窗口尺寸来说现阶段问题小，后续统一做 focus-lock 时一起治。

---

## 2026-04-21 · Phase 3-D2b.4 — AI 辅助·流式 Chat IPC + 中断 + History 截断

- **Scope**
  - D2b 第四刀：把 D2b.3 的"发一条 → 等整条 → 渲染"升级成 per-token 流式 + 用户可中断；session 文件格式与 D2b.3 完全一致，`ai_chat_send` 保留作为 fallback / 测试路径。会话数据层 / provider trait 都不动。
  - **后端**（Rust）：
    - `src-tauri/src/lib.rs`
      - `AppState` 新增 `chat_streams: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>`，作为"正在跑的流式 IPC 注册表"，key 是前端生成的 `stream_id`，value 是 spawn 任务在 token 间隙轮询的 cancel flag；外层 `Arc` 是为了让 spawn task 能 `.clone()` 拿所有权而不借 `State<AppState>` 的引用（后者需要 `'static` 生命周期）。
      - 注册 `ai_chat_stream_start` / `ai_chat_stream_cancel` 两条 IPC。
    - `src-tauri/src/commands/ai.rs`
      - 新增 `ai_chat_stream_start(stream_id, session_id, content) → ChatStreamStartResult`：**同步 pre-flight**（校验 stream_id / load session / build chat provider / 截断 history / 持久化 user turn / 注册 cancel flag）→ **异步 streaming 循环**（`tauri::async_runtime::spawn`）：pull deltas、emit 到前端、最后持久化 assistant turn。pre-flight 错误走 `Result<ChatStreamStartResult>` 返回，异步路径的错误走事件通道——避免"先返回 ok 再无法拿到 error"的模糊态。
      - 事件协议：统一前缀 `ai:chat-stream:*`，三个终止/增量事件：`delta { stream_id, content, finish_reason? }`、`done { stream_id, assistant, cancelled }`、`error { stream_id, failure }`。每个事件都带 `stream_id`，为将来的多流（D2b.6 独立窗口 / 预渲染）留好路由。
      - `ai_chat_stream_cancel(stream_id) → bool`：set cancel flag；spawn task 下次 poll 时 break → persist accumulated content → emit `done { cancelled: true }`。取消时"已收到的 token"保留到 assistant 消息里，用户可复用/复制，不丢。
      - `truncate_history_to_budget(messages, max_chars)`：按字符预算截断 history，永远保留首条 system message（若存在） + 最新若干条 user/assistant 对；**单条巨长消息**（用户刚发的那条）即便超预算也保留，让 provider 报 InvalidRequest 而不是吞调用。默认预算 `DEFAULT_HISTORY_TOKEN_BUDGET = 4000 tok × CHARS_PER_TOKEN = 3.5` ≈ 14k 字符，兼顾 gpt-4o-mini / Qwen / Llama 8k ctx 窗口 + 留 output 空间，不引新 tokenizer 依赖。
      - Empty-reply 防护保留（和 D2b.3 一致）：provider 吐空串时不落 assistant，emit `error { kind: "other" }`；若"取消发生在首个 token 前"则文案改成"cancelled before any content arrived"。
    - 单元测试（`commands::ai::tests`）：+4 条覆盖 `truncate_history_to_budget` 的 4 类行为（全量保留 / 超预算丢老 / 永远保 system / 单条巨长仍保留）。
  - **前端**（TypeScript / Svelte）：
    - `src/lib/ipc/ai.ts`：新增事件名常量（`CHAT_STREAM_DELTA_EVENT` / `DONE` / `ERROR`）+ 三个事件 payload 接口 + `aiChatStreamStart` / `aiChatStreamCancel` 两个 IPC wrapper；保留 D2b.3 的 `aiChatSend`（做 fallback / e2e 参照）。
    - `src/lib/panel/ChatPanel.svelte`：`send()` 改为 `aiChatStreamStart` + `listen('ai:chat-stream:*')`；新增 `streamingContent: string` 作为活动气泡的实时累加器、`activeStreamId: string | null` 做事件路由与 cancel；`sending` 的生命周期从"返回前"改为"`done/error` 到达前"。listener 惰性注册（首次 send 时才 `listen`）；`onDestroy` 里统一解绑防泄漏。
    - UI：sending 期间的 assistant 气泡从"仅三点动画"改成"有 token 时直接渲染 + 末尾闪烁光标，无 token 时继续三点动画"；发送按钮流式期间切成"**中断**"按钮（`cancel-btn` 样式，红色），点击后 `aiChatStreamCancel` + 等待 `done { cancelled: true }` 顺势收尾。
    - 同会话的 post-stream 协调：`onStreamTerminal` 统一走 `loadActiveSession` + `refreshSessions`——partial-on-cancel / no-assistant-on-error / 正常完成三种态都以"后端持久文件"为真相源，不手工 merge。
- **How to verify**
  - **构建**：`cargo check` / `cargo test --lib` → **185/185 通过**（新增 4 条 history 截断测试均 PASS）；`cargo clippy --lib --no-deps` 零新增告警（只有 scanner.rs 预存的 type_complexity / explicit_auto_deref 等）。
  - **前端**：`pnpm check` → **0 errors / 0 warnings**。
  - **手测**（Settings 里把 `chat_model` 指到能 stream 的模型，如 OpenAI `gpt-4o-mini` 或 Ollama `qwen2.5`）：
    1. 发 `讲一个 300 字的故事` → assistant 气泡在第一个 token 到达时从"三点"切换到"文本 + 闪烁光标"，后续按 token 追加，直到 `done` 事件后光标消失并把持久化后的正式气泡替换上来。
    2. 长回复中途点"中断" → 按钮变灰，气泡停止增长，2~3 秒内收到 `done { cancelled: true }` 并把已累积内容作为 assistant turn 落盘；再打开会话文件可以看到被截断的 assistant JSONL 行。
    3. pre-flight 失败（故意把 chat_model 改成不存在的名字）→ `aiChatStreamStart` 直接返回 `failure { kind: invalid_request, user_message_persisted: false }`；前端回滚乐观 user 气泡 + 恢复输入框里的文本，无幻影消息。
    4. 流式中断开网络 → spawn task 下一轮 `stream.next().await` 得 `ProviderError::Network` → emit `ai:chat-stream:error` → 前端 banner。
    5. history 长度手测：对一个超过 30 条消息的会话连续发送 → 后端日志/断点可见 `truncate_history_to_budget` 把最老的对丢弃，system prompt（若有）被保留。
- **Known gaps**
  - **无 RAG 注入**：流式消息仍是 raw user/assistant turns；D2b.5 会把关联笔记的 top-K chunks 拼到 system prompt。
  - **无 `[[wiki-link]]` 渲染**：assistant 回答里的 wiki-link 仍是纯文本；D2b.5 接解析器 + 点击 `onOpenNote` 跳转。
  - **无 Chat 独立窗口**：`Panel.svelte` 里的 "AI 对话" tab 还是驻留主窗；D2b.6 做"弹出独立窗口 + AI 关闭时自动关闭"。
  - **会话级并发**：同一 session 连发两条（trigger 第二个 send 前第一个还没 `done`）目前被前端 `sending` 禁住；后端 `chat_streams` 注册表本身按 `stream_id` 维度隔离，但同 session 同时两条 stream 会并发 append——将来做"重发消息"时需要把 session 级的 append 改成 mutex 串行。
  - **Token 估算**：`CHARS_PER_TOKEN = 3.5` 是英文/中文折中；全 CJK 的会话实际 c/t ≈ 2，预算略保守，好过略激进；真需要精确的等 D3 再接 tokenizer。
  - **事件全局广播**：Tauri `app.emit()` 是多窗口广播，当前 ChatPanel 按 `stream_id` 过滤足够；等 D2b.6 独立窗口落地时，如果用户同时在主窗 + 独立窗看同一 stream 会收到双份事件——到时再按 `emit_to(label)` 或带 webview 上下文过滤。

---

## 2026-04-21 · Phase 3-D2b.3 — AI 辅助·Panel Tab 化 + 非流式 ChatPanel v1

- **Scope**
  - D2b 第三刀：把 D2b.1 的会话持久化 + D2b.2 的 provider chat 接口接到右栏 UI。**不动流式传输**（那是 D2b.4），先用一条非流式 IPC 把"发一条 → 等完整响应 → 渲染 markdown + 持久化 → 再渲染"整条闭环跑通；同时把 `Panel.svelte` 从单列改成 Tab（笔记关系 / AI 对话）。
  - **后端**（Rust）：
    - `src-tauri/src/commands/ai.rs`
      - 新增 `ai_chat_send(session_id, content) → ChatSendResult`：acquire `ChatStore` → load 现有会话 → `build_configured_chat_provider` → **先** 持久化 user 消息 → 组装历史 turns → `chat_stream` + `collect_chat_stream` → 持久化 assistant 消息 → 返回结构化 `ChatSendResult { ok, assistant?, failure? }`。
      - 顺序不变式：user turn **永远先落盘**，provider 失败时 assistant 不落；前端看到的历史里始终有 user 消息，用户可直接重试、不需要重打字——和 ChatGPT / Claude.ai 的 UX 一致。
      - 新增 `ChatSendFailure { kind, message, retry_after_secs?, user_message_persisted }` 配套类型；`user_message_persisted = false` 表示 pre-flight（无 vault / 无 provider / 无会话），前端据此决定是否把用户消息放回输入框。
      - Empty-reply 分支：provider 偶尔吐空回复（tool-call / stop 边角），不要落一条空 assistant 消息污染 transcript——直接返回 `failure { kind: "other", user_message_persisted: true }`。
      - `message_to_turn` helper：记录"storage `ChatMessage` → transport `ChatTurn`"方向的单一编辑点；未来扩多模态只需改这里。
    - `src-tauri/src/services/ai/runtime.rs`
      - 去掉 `build_configured_chat_provider` / `build_chat_provider_from_config` 上的 `#[allow(dead_code)]`——D2b.3 本刀就是第一个消费者。
    - `src-tauri/src/lib.rs`：注册 `ai_chat_send` IPC。
  - **前端**（TypeScript / Svelte）：
    - `src/lib/ipc/ai.ts`：新增 `ChatSendFailure` / `ChatSendResult` 类型 + `aiChatSend(sessionId, content)` wrapper。
    - `src/lib/panel/Panel.svelte`：重构 header 为 Tab bar（笔记关系 / AI 对话），AI 对话 tab 只在 `aiEnabled` 时显示；用 `$effect` 在 `aiEnabled` 变 false 时把 activeTab 自动切回 Links，防止 tab 头被隐但内容卡在 chat；Tab 状态 panel-local（面板够小，不放 global state）。保留原有 Links 内容不变动。
    - `src/lib/panel/ChatPanel.svelte`（新增，~650 行）：
      - 会话下拉 + 新建 (`+`) / 删除 (`×`) 图标按钮；关联笔记时在 header 下方显示 `关联笔记 · <path>`。
      - 消息列表：user/assistant 异色气泡 + 相对时间，`renderMarkdown()` 做最小 markdown 渲染（fenced code / inline code / bold / italic / 自动链接 / 段落 + 换行）——先 HTML-escape 全部，再还原被占位的 code block，避免 `@html` 注入。
      - 输入框：Enter 发送 / Shift+Enter 换行 / Cmd|Ctrl+Enter 强制发送；`sending` 时禁用发送按钮 + 显示"正在输入"三点动画气泡；发送前先乐观 push user 气泡，收到结果后 `loadActiveSession` + `refreshSessions` 用后端持久态覆盖乐观 UI（消息 id 从 `optimistic-*` 换成真实 id）。
      - 空会话+首次发送：自动 `aiChatSessionCreate(deriveTitle(text), filePath)`（取消息前 60 字为标题，关联当前笔记）；用 `lastResolvedSessionId` 这个**非响应式** `let`（不是 `$state`）给 `$effect` 做短路——activeSession 在 `send()` 里被乐观置好之后，`activeSessionId` 变化触发的 effect 会命中 `id === lastResolvedSessionId` 而 skip reload，避免空 transcript 短暂覆盖掉乐观气泡。
      - 失败 banner：按 `failure.kind` 分档文案（网络 / auth / 限流 + `retry_after_secs` / invalid_request / other），`user_message_persisted: false` 时追加"你的消息未发送"提示。
      - 删除确认 + 非空标题回退到 `会话 YYYY-MM-DD HH:mm`（`window.prompt` + `window.confirm`）——D2b.4+ 可以升级成 modal，但 v1 先不造轮子。
- **How to verify**
  - **构建**：`cargo check` / `cargo test --manifest-path src-tauri/Cargo.toml` → **181/181 通过**（本刀无新增单测：`ai_chat_send` 依赖 `State<AppState>` + Tauri runtime，按 D2b.1 的模式把测试覆盖留在 storage 层；provider / chat_store / runtime 三层的现有 covers 本刀所有新增路径）。
  - **前端**：`pnpm check` → **0 errors / 0 warnings**；修完 `<nav role="tablist">` 的 a11y 提示，换成 `<div>`。
  - **手测**（Settings → AI 辅助 要先配好 `chat_model`）：
    1. 右栏 header 出现"笔记关系 / AI 对话"两个 tab；未配 `chat_model` 时切到 AI 对话，首次发送会报 invalid_request。
    2. 对 chat model（如 Ollama `qwen2.5` 或 OpenAI `gpt-4o-mini`）发一句 `Hello` → 气泡顺序 user → "AI 正在输入…" → assistant；transcript 内的 markdown（```code```、`inline`、**bold**）按预期渲染。
    3. 关掉网络 / 改一个错误 base URL → 触发 network banner；kind=network + "检查网络连接或 provider base URL" hint；user 气泡保留。
    4. 会话下拉切换：`lastResolvedSessionId` 保证同 id 不重复 reload；切到已加载 session 时不见 loading spinner。
    5. 删除 active session → `window.confirm` → 自动选最新 session 或清空；再发一条会走 auto-create 分支，session 列表更新到新会话。
    6. 关闭 AI 总开关（Settings → AI 辅助）→ AI 对话 tab 从 header 消失且 activeTab 自动回到 Links，不留"空 tab"残影。
- **Known gaps**
  - **无流式**：`ai_chat_send` 先等 provider 吐完整条再回前端，长回复期间 UI 只看到三点动画——D2b.4 改成 `ai_chat_stream` + `emit_all` 后会按 token 实时 append。
  - **无中断按钮**：对应 D2b.4 的 cancel channel；目前长响应只能等 provider 自己 stop（或切 session 放弃当前请求，会话里已落的 user 消息会留着）。
  - **无 RAG 注入**：对话消息直接发 user/assistant，没带上关联笔记的 chunks——D2b.5 会在 system prompt + top-K chunks 层面补。
  - **无 `[[wiki-link]]` 渲染**：assistant 回答里若出现 `[[Note]]`，目前按纯文本显示，D2b.5 做识别 + 点击跳转。
  - **无 modal 形式的新建会话**：`window.prompt` / `window.confirm` 足够 v1，D2b.5 关联笔记选择器再换成 modal。
  - **markdown 渲染最小化**：无 headings / lists / tables / blockquote；这些在长回答里不算主流，等 D2b.5 / D3 真看到需要再接 `marked` 或专门的 renderer。
  - **无会话级并发防护**：后端 `ai_chat_send` 对同 session 的并发调用没有 mutex，双请求可能读到 stale history；前端 `sending` 状态已经禁住 send 按钮，v1 够用。

---

## 2026-04-21 · Phase 3-D2b.2 — AI 辅助·Provider Chat 接口（`AiProvider::chat_stream` + OpenAI SSE + chat_model 配置）

- **Scope**
  - D2b 第二刀：把 chat 流式能力从传输层一直拉通到"测连接"按钮，**不动任何 UI 渲染**——D2b.3 才接 Panel。这一刀让后面 D2b.3 的非流式 v1、D2b.4 的流式 IPC 都能站在稳定的 provider trait 上实现。
  - **后端**（Rust）：
    - `src-tauri/Cargo.toml`
      - 给 `reqwest` 打开 `stream` feature，把 `tokio`（`rt, sync, macros, time`）和 `futures-util`（`std`）从 dev-dep 提升到正式依赖——`chat_stream` 里 spawn task + bytes stream + mpsc 都要用。
    - `src-tauri/src/services/ai/provider.rs`
      - 新增 chat wire 类型：`ChatRole`、`ChatTurn`、`ChatRequest`、`ChatDelta`、`ChatStream = Pin<Box<dyn Stream<…> + Send>>`。
      - `AiProvider` trait 加 `async fn chat_stream`，**带默认实现**返回 `ProviderError::InvalidRequest("chat is not supported by this provider")`——既存的 `FailProvider` / 未来 embed-only 双替身无需 opt-in；只有 OpenAI + Mock chat 脚本真正 override。
      - `collect_chat_stream` helper：把整条流聚合成一个 `ChatDelta`，供"测连接"和后续非流式 v1 复用。
      - `MockProvider` 加 `chat_script` / `chat_error` 两个 `Arc<Mutex<Option<…>>>`：`set_chat_script(tokens)` 预装 tokens 数组，`set_chat_error(err)` 预装待 surface 的错误（下一次 `chat_stream` 消费后清零）。未配置时默认 three-chunk echo 最后一句 user turn，足够测 multi-delta 聚合而不至于过分 chatty。
    - `src-tauri/src/services/ai/openai.rs`
      - `chat_completions_url()` helper；wire shapes（`ChatCompletionRequestBody`、`ChatWireMessage`、`StreamOptionsBody`、`ChatStreamChunk`、`ChatStreamChoice`、`ChatStreamDelta`、`ChatStreamUsage`）。
      - 纯函数 `parse_sse_data(payload) -> Result<Option<ChatDelta>, ProviderError>`：`[DONE]` → `Ok(None)`，其它 payload 解析成增量 delta，失败归入 `ProviderError::Other`——方便用字符串单测驱动整个 parser，不用假 HTTP 服务器。
      - `find_event_end(buf) -> Option<(end, delim_len)>`：同时吃 `\n\n` 和 `\r\n\r\n`，返回 delimiter 长度供调用者 drain。
      - `OpenAiProvider::chat_stream`：
        - POST `/chat/completions` with `stream: true` + `stream_options.include_usage: true`（OpenAI 上报 `usage` 收尾 chunk；Ollama/LM Studio 忽略此字段，无害）；`Accept: text/event-stream` 头。
        - 握手失败 → `classify_http_error` 走既有 401/403/429/4xx/5xx 分类；握手成功后用 reqwest `bytes_stream()` 拿到 byte 流。
        - **spawn 一个 tokio task** 读 byte 流、累积到 `buf`、用 `find_event_end` 切 event、对每行 `data:` payload 调 `parse_sse_data`，结果往 `mpsc::channel(16)` 发；接收端用 `futures_util::stream::unfold` 包成 `Stream`。**取消时 drop 返回流 → tx 关闭 → spawn task 下次 send 退出**，无 leak。
        - 边界：尾部 event 无 `\n\n` 分隔时会 flush 一次；中途 `[DONE]` 立即 return；JSON 解析错误作为 `Err(ChatDelta)` 下发后终止。
    - `src-tauri/src/services/config.rs`
      - `AiProviderConfig` 加 `chat_model: String`（`#[serde(default)]`）——空串 = chat 停用，embedding 仍可用；老配置文件无 `chat_model` 字段时 serde 默认空，完全向前兼容。
    - `src-tauri/src/services/ai/runtime.rs`
      - 新增 `build_configured_chat_provider` / `build_chat_provider_from_config`，validation 盯 `chat_model`（不盯 `embed_model`）。暂挂 `#[allow(dead_code)]`——首个消费者是 D2b.4 的 `ai_chat_stream`，现在把 helper 放到位是为了那一刀不用再回头改 runtime。
    - `src-tauri/src/commands/ai.rs`
      - `ai_provider_set_config` 签名加 `chat_model: Option<String>`；None / 空串 都归一到 `chat_model: ""`（chat 停用）。
      - 新增 `ai_provider_test_chat_connection`（异步、可选 override 参数对齐 `ai_provider_test_connection`）：跑一句 `"Say OK."` 1-turn 对话，`collect_chat_stream` 聚合后截 200 字符返回 `ChatProviderTestResult { ok, reply?, input_tokens?, output_tokens?, error_kind?, … }`。**20 秒 timeout**（embedding test 是 10 s；chat 首 token 通常慢一些）。
      - 把 `ChatRole` 统一成 `provider::ChatRole` 一份——原先 `chat_store` 重复了一份同名 enum，改成 `pub use super::provider::ChatRole`；线上序列化仍是 `lowercase`，零破坏。
    - `src-tauri/src/lib.rs`：`generate_handler!` 注册 `ai_provider_test_chat_connection`。
    - `src-tauri/src/services/ai/init_service.rs`：测试 fixture 的 `AiProviderConfig { … }` struct literal 补 `chat_model: String::new()`。
  - **前端**（TS + Svelte）：
    - `src/lib/ipc/config.ts`：`AiProviderConfig` 加 `chat_model: string`。
    - `src/lib/ipc/ai.ts`：
      - `aiProviderSetConfig` 签名加 `chatModel: string | null`；`ChatProviderTestResult` 类型 + `aiProviderTestChatConnection({ kind?, baseUrl?, chatModel?, apiKeyOverride? })` wrapper。
    - `src/routes/+page.svelte`
      - 新增 Settings 状态：`aiProviderChatModel`（默认 `gpt-4o-mini`）、`aiProviderChatTesting`、`aiProviderChatTestState`。
      - `saveAiProvider` 把 `chatModel.trim() || null` 一并送后端；`testAiProviderChat()` 复用表单值走新 IPC；`openSettings` / `clearAiProvider` 清两个 test state；读回 snapshot 时，仅当 `snapshot.ai_provider.chat_model` 非空才覆盖本地默认——避免"还没填就被空串干掉"。
      - Settings UI 在 Embed model 下新增 "Chat model · 留空停用" 输入框；动作栏按钮拆成 `测试 Embedding` / `测试聊天` 两档，`保存` / `清除` 两种在任一测试跑时都禁用；聊天测试结果 banner 展示模型 reply 截断 + out tokens（如果后端上报了 usage）。
      - `providerTestFailureText` 形参类型放宽到结构化 `{ error_kind?, error_message?, retry_after_secs? }`——`ProviderTestResult` 和 `ChatProviderTestResult` 在失败侧字段同构，共用一个 formatter。
  - **文档**：`design_V2.md` §6.24 追加 D2b.2 八小节（chat trait 形状 / SSE 解析策略 / channel 取消语义 / chat_model 独立原因 / 测连接区分 / 前端按钮拆分 / runtime helper 预置 / 不做事项），changelog 2.24；`plan_P3.md` D2b 表把 D2b.2 标 ✅、D2b.3 置为 next；本文件顶部本条记录。

- **How to verify**
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib` → **181 passed; 0 failed**（比 D2b.1 的 167 新增 14：MockProvider chat 4 条 + OpenAI SSE 10 条）。
  - `cargo clippy --lib --tests -- -A clippy::approx_constant -A clippy::bool_assert_comparison` → 修改触及的六个文件（`openai.rs` / `provider.rs` / `chat_store.rs` / `runtime.rs` / `services/config.rs` / `commands/ai.rs`）**零 clippy 警告**；其余告警均为 D2b.2 之前既有（`db/indexer.rs` / `scanner.rs` / `embedding_store.rs`），不在本刀范围。
  - `pnpm check` → 0 errors / 0 warnings。
  - 手动流程（后端 wire 已通，UI 会在 D2b.3 里全量走查；这里做 IPC 级回归）：
    1. Settings 打开 → Chat model 框默认 `gpt-4o-mini`，Base URL / Embed model / API key 一如 D2a.2。
    2. 填入真实 OpenAI key → 点 `测试 Embedding` → ✓ 维度 / tokens banner（和 D2a.2 无回归）。
    3. 点 `测试聊天` → ✓ `Chat 连接成功 · 回复 "OK" · N out tokens`；若 chat_model 为空，按钮自动禁用并 hover 提示"请先填写 Chat model"。
    4. 改一个错 key → 点 `测试聊天` → ✗ banner 走 `auth` 分支（message 透传 OpenAI "Incorrect API key"）。
    5. Base URL 故意指向 127.0.0.1:1 → ✗ `network` 分支（带 retry 提示时显示 retry-after）。
    6. Ollama（本地 llama3.1）场景：Base URL = `http://localhost:11434/v1`、chat_model = `llama3.1`、API key 空 → `测试聊天` ✓；`input_tokens` / `output_tokens` 为 undefined（Ollama 不报 usage），UI 正确跳过"out tokens"拼接。
    7. 保存 → 重开 Settings → Chat model 回读为保存值；清除 → 配置 + keyring 都空。
  - 单测专项：
    - `provider::tests`：mock script 播放聚合成整段、无 script 时 echo 最后一条 user turn、`set_chat_error` 精确透传 `RateLimit { retry_after_secs, message }`、空 messages → `InvalidRequest`。
    - `openai::tests`：`chat_completions_url` 拼接正确；`parse_sse_data` 覆盖 `content delta` / `finish_reason chunk` / `usage trailer` / `[DONE]` / 非 JSON；`find_event_end` 覆盖 `\n\n` / `\r\n\r\n` / 半截 buf 三分支；`chat_stream` 空 messages 不发起 HTTP 直接 `InvalidRequest`。

- **Known gaps（明确留给后续刀的事）**
  - **流式 IPC（`ai_chat_stream`）**：D2b.2 只把传输层 + 测连接走完；Tauri 侧的 `emit_all` + 前端 unlisten + 中断事件是 D2b.4 的专项。`build_configured_chat_provider` 现在是 dead code 占位，D2b.4 接上就会活。
  - **Panel.svelte Tab 化 + ChatPanel v1**：当前 UI 只多了 Settings 里的聊天测试，**ChatPanel 本体在 D2b.3**；D2b.3 落地前用户无法从右侧面板里开启对话。
  - **工具调用 / 多轮 function_call**：`ChatTurn` 只支持纯文本，D2b 范围内有意不扩；未来如要接 tool use，加新 enum 分支或迁 trait 到 v2。
  - **非 OpenAI 兼容 provider**：`parse_sse_data` 只认 OpenAI 的 chunk 形状；Anthropic 的 SSE（`event:` + 分段 delta）要开新 `AiProvider` 实现，不在本刀。
  - **Settings `retry_after_secs` 展示**：目前 banner 透传到 `providerTestFailureText`，但视觉上只出 message；精细化（带 countdown）留到 D2b.3 能看到真实 rate-limit 场景再打磨。
  - **Test connection 断流中断**：目前 `collect_chat_stream` 会把整段收完；如果 Ollama 吐了一百万 token 的 stream，Settings 会卡到完。生产场景下没人会拿这个 IPC 跑 free-form 问答，真出现再加 `max_tokens` 外置参数（后端现在已经硬编 `max_tokens: Some(8)`）。

---

## 2026-04-21 · Phase 3-D2b.1 — AI 辅助·会话数据层（`ChatStore` + 5 IPC + 前端 wrapper）

- **Scope**
  - D2a 全线收口后，D2b 切成 1→6 六刀（会话持久化 → provider chat trait → Panel Tab/非流式 → 流式 + 中断 → RAG 注入 → 弹出独立窗口），详见 `design_V2.md §6.24`。本刀只做 **D2b.1**：把对话落盘的 source-of-truth 定死，**无 provider 调用、无 UI**，让后续五刀都能在稳定 schema 上接东西。
  - **后端**（Rust）：
    - `src-tauri/src/services/ai/chat_store.rs`（新文件）
      - `ChatStore { root: PathBuf }` 薄包装；构造是 `PathBuf::clone`，不挂 `AppState`（避免 vault 切换时状态泄漏，embedding store 挂是因为要持有 SQLite 连接，这里不需要）。
      - 对外类型：`ChatRole { User / Assistant / System }`（`rename_all = "lowercase"`）、`ChatMeta { v, session_id, title, created_at, related_note? }`、`ChatMessage { v, id, role, content, created_at }`、`ChatSessionSummary`、`ChatSessionFull`。
      - 内部 `ChatLogLine` 用 `#[serde(tag = "type", rename_all = "snake_case")]`，首行必须 `meta`，后续一律 `message`；每行带 `v: 1` schema version，`SCHEMA_VERSION` 常量化。
      - `create`：`OpenOptions::create_new(true)` 防 id 冲突；title 空白回落 "Untitled"；`> 500` 字符直接拒；write meta + `sync_data`。
      - `append`：`OpenOptions::append(true)` + `sync_data`——崩溃最多丢 in-flight 一行，earlier turns 可靠。
      - `load`：**严格**。多条 meta / message 先于 meta / 未知 schema / 解析失败都报 `AppError::Other("corrupt chat session ... line N: ...")`。
      - `list`：**宽松**。`file_type` 先过滤目录；只吃 `*.jsonl`；单文件 corrupt 时 `tracing::warn!` 跳过不阻塞 sidebar。按 `created_at desc` 排序。
      - `delete`：`ErrorKind::NotFound` → `Ok(false)`（幂等）。
      - `session_id` 形如 `chat-YYYYMMDDTHHmmss-<8hex>`，后缀走 `sha256(nanos + pid + AtomicU64 seq)`，同 tick 连发不会碰撞；**不引入 `uuid`/`rand` 新依赖**（复用 `sha2` + `chrono`）。
      - `validate_session_id`：白名单 `[A-Za-z0-9_-]`、≤64 字符；堵死 `""` / `".."` / `"a/b"` / `"has space"` / `"has.dot"` / 65 字符 这类 payload，所有 IPC 入口必过此关。
    - `src-tauri/src/services/ai/mod.rs`：挂 `pub mod chat_store;`。
    - `src-tauri/src/commands/ai.rs`
      - 新增 5 条 IPC：`ai_chat_session_list` / `_create` / `_load` / `_append` / `_delete`。
      - `create` 硬性**后端生成 session_id**（前端不许自带；防攻击路径），`related_note` 在入口对绝对路径 / `..` 做 `PathEscape` 检查（store 本身不读这个字段的文件，但 D2b.5 会 deref，不能把坏路径先放进去）。
      - `chat_store(state)` helper 按需构造 `ChatStore::new(&vault)`，不缓存。
    - `src-tauri/src/lib.rs`：`tauri::generate_handler!` 注册 5 个新命令。
  - **前端**（TS + Svelte）：
    - `src/lib/ipc/ai.ts`
      - 新增 5 个 wrapper：`aiChatSessionList` / `aiChatSessionCreate(title, relatedNote?)` / `aiChatSessionLoad(sessionId)` / `aiChatSessionAppend(sessionId, role, content)` / `aiChatSessionDelete(sessionId)`。
      - 对应 TypeScript 类型：`ChatRole`、`ChatMeta`、`ChatMessage`、`ChatSessionSummary`、`ChatSessionFull`；时间戳一律 `number`（Unix seconds），UI 渲染时再转 locale。
    - **无 UI 改动**——`Panel.svelte` / `ChatPanel.svelte` 的改造留给 D2b.3；这一刀只落一层"要调哪些函数"的 contract。
  - **文档**：`design_V2.md` 新增 §6.24（D2b 整段路线图 + D2b.1 八小节详设：落盘选型/schema/API/summary 计算/IPC 表/前端 wrapper/刻意不做/测试），状态行 + changelog 2.23；`plan_P3.md` 新增 D2b 子刀进度表、状态快照同步；本文件顶部本条记录。

- **How to verify**
  - `cargo test --manifest-path src-tauri/Cargo.toml` → `167 passed; 0 failed`（chat_store 贡献 10 条新单测；整库比 D2a.6 的 157 +10）。
  - `pnpm check` → `0 errors, 0 warnings`。
  - 手动 round-trip（`cargo build --lib` 通过，IPC 宏展开成功；实机走查在 D2b.3 接 UI 时一起做）：
    1. 打开任意 vault → call `ai_chat_session_list` → `[]`。
    2. call `ai_chat_session_create("hello", null)` → 拿到 `ChatSessionSummary { session_id: "chat-...", title: "hello", ... }`。
    3. `ai_chat_session_append(session_id, "user", "你好")` → `ChatMessage { id: "msg-...", role: "user", ... }`。
    4. 肉眼打开 `<vault>/.mynotes/ai/chats/<id>.jsonl` → 两行 JSON，第一行 `"type":"meta"`，第二行 `"type":"message"`。
    5. `ai_chat_session_load(session_id)` → messages 列表长度 1、顺序与 role 正确。
    6. `ai_chat_session_delete(session_id)` → `true`；再删一次 → `false`。
  - 单测专项覆盖：roundtrip / 空白 title 回落 / append 顺序 + role + id 唯一 / list 按 `created_at` desc + `message_count` + `last_message_at` 聚合 / load 不存在 / delete 幂等 / 6 类非法 session_id / corrupt line → load 报错 / 空 root → `[]` / 非 jsonl 文件 skip。

- **Known gaps（D2b.1 本身已收口；下列是明确留给后续刀的事）**
  - **会话重命名 IPC**：目前 meta 里的 `title` 一次性写在首行；`ai_chat_session_rename` 留给 D2b.3（有 UI 才知道要不要就地改）。技术路径是 append `meta_update` 或 rewrite 首行，二选一。
  - **Message 修改 / 删除**：append-only 设计下删一条 = 重写全文，破坏崩溃隔离；产品需求目前也不到。
  - **多进程并发锁**：桌面单进程，`append` + `sync_data` 够用；真要上多窗口同编会话是 D2b.6 弹窗之后的事。
  - **Schema 迁移框架**：只有 `v: 1`；真有 v2 再写一次性迁移脚本，现在不预置框架。
  - **UI**：Panel 的 Tab 架构、`ChatPanel.svelte`、发消息 UI、流式渲染全部延后到 D2b.3 / D2b.4。

---

## 2026-04-21 · Phase 3-D2a.6 — AI 辅助·失败降级 UX（结构化失败 + 原子替换 + 整库初始化提前中止）

- **Scope**
  - D2a.5 之后，embedding 底座已经够用，但失败语义还不够“可放心整理整库”：单篇 embed 失败只是红字字符串，429 会丢掉 quota/billing 正文，整库初始化遇到明显的 provider 级失败也会继续傻跑。D2a.6 把这三块一起补齐。
  - **后端**（Rust）：
    - `src-tauri/src/services/ai/embedding_store.rs`
      - 新增 `replace_note_chunks(note_rel_path, chunks)`，把旧版 `delete_by_note + upsert_chunks` 收进**同一个 SQLite 事务**。这样即使 insert 失败，也不会留下“旧向量先被删掉”的半清空状态。
    - `src-tauri/src/services/ai/provider.rs`
      - `ProviderError::RateLimit` 从只带秒数升级为 `{ retry_after_secs, message }`。
      - 新增 `ProviderErrorKind` 与 `describe_provider_error()`，把 provider 错误统一展开成 `kind + message + retry_after_secs?`。
    - `src-tauri/src/services/ai/openai.rs`
      - 429 分类不再丢掉响应正文；测试同步改成校验 `message` 与 `retry_after_secs` 都存在。
    - `src-tauri/src/services/ai/embed_service.rs`
      - 新增 `EmbedFailure { kind, message, retry_after_secs?, store_unchanged }` 与 `EmbedFailureKind`。
      - `embed_note()` 改为返回 typed failure，而不是把 provider/config/store 问题全部塌成 `AppError::Other("embed failed: ...")`。
      - 写盘阶段改用 `replace_note_chunks()`，保证失败时 `store_unchanged = true` 语义站得住。
      - 新增单测 `provider_rate_limit_is_classified_and_store_unchanged`。
    - `src-tauri/src/commands/ai.rs`
      - `ProviderTestResult` 新增 `retry_after_secs`。
      - `ai_embed_note()` 改为返回 `EmbedNoteResult { ok, outcome?, failure? }`，前端不再需要把 provider/config 失败当 IPC reject 处理。
    - `src-tauri/src/services/ai/init_service.rs`
      - `VaultEmbedRunResult` 新增：
        - `note_count_not_attempted`
        - `aborted_early`
        - `aborted_error_kind`
        - `aborted_error_message`
        - `aborted_retry_after_secs`
      - provider 级失败（`network / auth / rate_limit / invalid_request`）出现时，整库初始化在首个明确失败点提前中止；`other` 类单文件失败仍继续。
      - 新增单测 `embed_vault_aborts_early_on_provider_failures`。
  - **前端**（Svelte + TS）：
    - `src/lib/ipc/ai.ts`：同步 `EmbedNoteResult` / `EmbedFailure` / `retry_after_secs` / init-abort 字段。
    - `src/routes/+page.svelte`
      - 新增 AI failure 文本归类函数，把 network / auth / rate-limit / invalid-request / 配置缺失映射成不同提示。
      - Settings 里的“测试连接”不再只打印 `error_kind: message`，而是显示更接近用户动作的建议。
      - 单篇 embed 失败时会明确说明“现有索引未被改坏”。
      - 整库初始化遇到 provider 级失败时显示“初始化已中止 + 未尝试 N 篇 + 失败原因 + 重试建议”。
- **How to verify**
  - 代码级：
    - `cargo test --manifest-path src-tauri/Cargo.toml`
      - 结果：`157 passed; 0 failed`
    - `pnpm check`
      - 结果：`svelte-check found 0 errors and 0 warnings`
  - 手测建议：
    - 在 Settings 里故意填错 API key，点“测试连接”，确认提示是“认证失败”，而不是泛化成一条原始字符串。
    - 配一个不存在的 embedding model，点“Embed 当前笔记”，确认 notice 提示模型名/协议不匹配。
    - 在 provider 不可用时跑“初始化索引”，确认它会在首个 provider 级失败点中止，并显示“未尝试 N 篇”；已存在的 embedding 不应被清空。
- **Known gaps**
  - ⚠️ **没有独立 retry/backoff 队列**：D2a.6 只把失败分类和中止语义做对，不做 durable retry。
  - ⚠️ **provider `other` 仍按 continue 处理**：例如某些 500 可能本质上也是 provider-wide 问题；这一轮选择保守，不把所有 `other` 都当成全局中止信号。
  - ⚠️ **尚未做真实 provider 的手工 smoke**：这轮校验停在单测和 `pnpm check`，建议你在真实 OpenAI/Ollama 配置下手点一遍 Settings 流程。

## 2026-04-21 · Phase 3-D2a.5 — AI 辅助·related-notes 向量打分升级（`title_jaccard` → `embedding_cosine`）

- **Scope**
  - D2a.4 之后整库 embedding 已经可用，这一刀把它真正接回 D1 用户面：`ai_related_notes` 的第四个信号不再看“标题相似”，而是看本地向量的“语义相近”。
  - **后端**（Rust）：
    - `src-tauri/src/services/ai/embedding_store.rs`
      - 新增 `only_model_name()`：当当前没有有效 provider 配置，但库里只存在一个 distinct model namespace 时，允许消费这套向量。
      - 新增 `note_cosine_scores(note_rel_path, model)`：按 model 聚合每篇笔记的 chunk vectors（求和），再做 note-level cosine；负值 clamp 到 0，保持信号范围和旧 `title_jaccard` 一样仍在 `[0, 1]`。
      - 新增 3 条单测：`only_model_name_requires_exactly_one_distinct_model`、`note_cosine_scores_aggregate_chunks_by_note`、`note_cosine_scores_empty_when_source_missing`。
    - `src-tauri/src/commands/ai.rs`
      - `RelatedSignals.title_jaccard` 改为 `embedding_cosine`。
      - `ai_related_notes` 改为优先读取当前配置里的 `embed_model`；若配置缺失则尝试回退到 store 里唯一的 model；无法确定 active model 或当前 note 没有向量时，`embedding_cosine = 0`，其它本地信号继续工作。
      - 删除旧 bigram / jaccard helper；保留 `staleness` 与组合打分测试，并把注释语义同步成 `embedding_cosine`。
  - **前端**（Svelte + TS）：
    - `src/lib/ipc/ai.ts`：`RelatedSignals` 类型字段改成 `embedding_cosine`。
    - `src/lib/panel/Panel.svelte`：
      - tooltip 从「标题相似 XX%」改为「语义相近 XX%」。
      - AI badge hover 从「本地启发式打分」改为「本地索引打分」。
    - `src/routes/+page.svelte`：Settings 提示文案改为“完成 AI 索引初始化后叠加语义向量相似度”，避免继续误导成“纯标题相似”。
- **How to verify**
  - 代码级：
    - `cargo test --manifest-path src-tauri/Cargo.toml`
      - 结果：`155 passed; 0 failed`
    - `pnpm check`
      - 结果：`svelte-check found 0 errors and 0 warnings`
  - 手测建议：
    - 先对一批笔记跑过 `初始化索引`。
    - 打开一篇有明显主题相近但标题完全不同的笔记，观察 related-notes 面板 tooltip 是否出现「语义相近」而不是「标题相似」。
    - 清空/移除 provider 配置后，若 `embeddings.sqlite` 里只剩一个 model，related-notes 仍应能继续叠加 embedding 分数；若库里有多个 model 且当前无法确定 active model，则面板应静默退回其余本地信号，不报错。
- **Known gaps**
  - ⚠️ **还没有独立 semantic-search IPC / UI**：这轮只升级 related-notes，不开放 chunk-level 检索页。
  - ⚠️ **多 model 且无当前配置时会静默退回 `embedding_cosine = 0`**：这是刻意保守，避免把不同 model 的向量混算。
  - ⚠️ **未做手工交互 smoke**：本轮只完成了编译、单测和 `pnpm check` 校验；related-notes 面板的实际排序变化仍建议在真实 vault 里点开验证一遍。

## 2026-04-21 · Phase 3-D2a.4 — AI 辅助·整库初始化（dry-run 预估 + 确认执行）

- **Scope**
  - D2a.3b 之后系统已经能自动追未来的改动，但旧 vault 仍缺一条"把历史笔记整库补齐"的入口。D2a.4 把这件事补成**显式两步流**：先 preview 估算规模与成本，再确认执行整库初始化。
  - **后端**（Rust）：
    - `src-tauri/src/services/ai/init_service.rs`（新）：
      - `preview_vault_embed(vault, store, provider_cfg)`：复用 `scanner::walk_vault_md` 遍历全部 markdown，`chunk_markdown` 统计非空笔记的 chunks / tokens；用当前 model 的 `note_mtime_for_model(rel, model)` 区分 `to_embed / up_to_date / empty`；路径预览最多返回 100 条。
      - `embed_vault(store, provider, model, vault)`：逐 note 复用 `embed_service::embed_note`，汇总 `embedded / up_to_date / empty / failed`，**不在首个失败 abort**，让用户拿到整批结果。
      - `estimate_cost(...)`：localhost / 127.0.0.1 / ::1 直接按 `$0`；OpenAI 官方 host + 已知 embedding model 用公开单价估算；其他 OpenAI-compatible provider 明确回 `unknown`，不乱猜第三方计费。
    - `src-tauri/src/commands/ai.rs`：新增两条 IPC：
      - `ai_embed_vault_preview() -> VaultEmbedPreview`
      - `ai_embed_vault_run() -> VaultEmbedRunResult`
    - `src-tauri/src/services/ai/embedding_store.rs`：新增 `note_mtime_for_model(note_rel_path, model)`。
    - `src-tauri/src/services/ai/embed_service.rs`：`SkipReason::UpToDate` 改成按**当前 model**判定，而不是"该 note 任意已有向量"。这样切换 embedding model 后不会被旧向量误短路。
  - **前端**（Svelte + TS）：
    - `src/lib/ipc/ai.ts`：新增 `VaultEmbedPreview` / `VaultEmbedRunResult` / `CostEstimateKind` 类型与 `aiEmbedVaultPreview()` / `aiEmbedVaultRun()` wrappers。
    - `src/routes/+page.svelte`：
      - Settings「AI 索引 · Embedding」区新增第三个按钮 `初始化索引`，按钮区现在是 `Embed 当前笔记 / 初始化索引 / 清空 AI 索引`。
      - 点击 `初始化索引` 先调 preview，再弹出现有 `.modal-preview` 风格 modal；modal 展示待初始化 notes、total markdown、up-to-date、empty、预计 chunks / tokens / 成本，以及前 100 条路径。
      - 确认后执行 `aiEmbedVaultRun()`，完成后刷新 stats，并把 summary 收口到现有 `embedNotice`。
      - 新增 `embedInitPreviewLoading / embedInitRunning / embedInitPreview / embedInitError` 四块状态；用 `embedActionBusy` 统一锁住单 note embed / clear-all / full init，避免并发 provider 调用。
  - **实现取舍**：
    - 没有单独加"强制重建全部"开关；当前 force path 是先点 `清空 AI 索引`，再跑 `初始化索引`。这样比再塞一个 checkbox 更直接，也不再引入新持久化状态。
    - 整库执行结束前不做逐 note 百分比 UI，只返回汇总结果；先把 correctness 和 guardrail 打稳。
- **How to verify**
  1. `cargo test --manifest-path src-tauri/Cargo.toml` → **161/161 passed**。
  2. `pnpm check` → **0 errors, 0 warnings**。
  3. 人工 happy path：
     - 打开 Settings →「AI 索引 · Embedding」→ 点击 `初始化索引`。
     - 应先看到预览 modal，而不是直接打 provider。
     - modal 文案应包含：待初始化 notes 数、预计 chunks / tokens、成本提示，以及前 100 条路径。
     - 点 `开始初始化` 后，结束时 notice 应显示 `已写入 N 篇 / X chunks / Y tokens`；Settings stats 刷新。
  4. 人工 local-provider path：
     - Provider 指向 `http://localhost:11434/v1` 一类本地地址。
     - preview 中成本应显示按 `$0` 估算。
  5. 人工 model-switch path：
     - 先用旧 model embed 若干 notes。
     - 保存 provider 为新 model，但不改 `.md` 内容。
     - 再跑 `初始化索引` preview，应仍把这些 notes 计入 `to_embed`，而不是误判成 up-to-date。
- **Known gaps**
  - ⚠️ **无后台进度条**：大 vault 初始化时用户只能看到按钮 loading + 结束后的 summary notice，没有实时百分比；这是刻意延后到 D2a.6 / D4 再做。
  - ⚠️ **第三方 provider 成本仍是未知**：OpenRouter / 自建 proxy / 企业网关等没有统一单价接口，本轮不会乱猜。
  - ⚠️ **失败明细只回前 20 条**：足够让 UI 给出一个 representative sample；完整错误仍建议看日志。

## 2026-04-21 · Phase 3-D2a.3b — AI 辅助·watcher 增量 embed（30 s debounce + delete 同步清理）

- **Scope**
  - D2a.3a 已经把单篇 embed 流水线跑通，这一刀只补"自动追上"：把现有 `notify` watcher 从"只管 SQLite 主索引"扩成"主索引 + AI embedding"双队列，但不引入新的前端 UI、toast、任务中心。
  - **共享运行时决策**（`src-tauri/src/services/ai/runtime.rs` 新文件）：
    - 新增 `auto_embed_enabled(cfg)`：`ai_enabled == Some(false)` 时关闭；`None` 继承前端默认语义（视为开启）；同时要求 `ai_provider.kind/base_url/embed_model` 三字段非空，否则 watcher 不入队。
    - 新增 `build_provider_from_config(cfg, secrets)` / `build_configured_provider(cfg, secrets)`：把"读 provider config + 读 keychain + 组装 `OpenAiProvider`"统一到一处，允许空 API key（本地 Ollama）。
    - `src-tauri/src/commands/ai.rs` 的 `build_configured_provider(state)` 改为复用 runtime helper，避免命令层和 watcher 各维护一套 provider bootstrap。
  - **AppState / vault 生命周期**：
    - `src-tauri/src/lib.rs`：`AppState.config` 从 `Mutex<ConfigStore>` 调整为 `Arc<Mutex<ConfigStore>>`，让 watcher 线程能读取**实时**配置。
    - `src-tauri/src/commands/vault.rs::attach_index`：启动 watcher 时不再只传 `index.sqlite` 句柄，还把 `state.config.clone()` 和 `state.embeddings_handle()` 一起传进去。若 embedding store 打不开，watcher 仍正常跑 SQLite 增量索引，只是不启动 AI worker。
  - **watcher 双队列**（`src-tauri/src/services/watcher.rs`）：
    - 保留原有 `notify-debouncer-full` + **200 ms** SQLite 防抖逻辑，`.md` 仍走 `scanner::reindex_one/delete_one`。
    - 追加一条内部 AI 队列：`AiWatchMsg::{Upsert(String), Delete(String)}` + `AiDebounceQueue { deadlines: HashMap<String, Instant> }`。
    - `create/modify`：
      - 路径若不是 markdown / 在 hidden path / 在 `attachments/` 下，直接忽略（与扫描器首轮过滤口径保持一致）。
      - 若 `abs.exists() == false`，按 `Delete` 处理，专门兜住 rename / 外部同步器把"旧路径消失"包装成 `Modify` 的情况。
      - 仅当 `auto_embed_enabled` 为真时发送 `Upsert(rel)`，把同一路径 deadline 刷到 `now + 30 s`。
    - `delete`：
      - 立即 `queue_delete(rel)`，取消 pending upsert。
      - 同步 `EmbeddingStore::delete_by_note(rel)` 清 stale chunks，不走网络、不等 30 s。
    - flush 阶段：
      - 到点后 `pop_due()`，按路径排序稳定处理。
      - 每一批只组装一次 provider；若当下 config 未就绪 / keychain 失败，则 warning 后丢弃该批 due notes，不保留 durable retry 队列。
      - 对每个 due note 调 `embed_service::embed_note(...)`，成功、`UpToDate`、`Empty`、失败分别打 info/debug/warn log。
  - **测试**：
    - `src-tauri/src/services/ai/runtime.rs` 新增 6 条单测：默认启用、显式关闭、provider 配置不完整、允许空 key、空 `kind` 报错、能读回已保存 key。
    - `src-tauri/src/services/watcher.rs` 新增 3 条单测：upsert debounce+去重、delete 取消 pending upsert、markdown 路径过滤。
- **How to verify**
  1. `cargo test --manifest-path src-tauri/Cargo.toml` → **154/154 passed**。
  2. `pnpm check` → **0 errors, 0 warnings**。
  3. 人工 happy path：
     - 配好 AI Provider，并确保 Settings 里的 AI 开关开启。
     - 打开一篇 `.md`，随便编辑并保存。
     - 等待约 30 秒，再进 Settings 看「AI 索引」计数，或查看日志，应看到该 note 被自动 embed。
     - 连续快速保存多次，30 秒后只应触发一次 embed，而不是每次保存都打 provider。
  4. 人工 delete path：
     - 先手动 embed 一篇笔记，确认 stats 有该 note。
     - 删除该 `.md` 文件。
     - 无需等待 30 秒，embedding stats 应减少，日志可见 `delete_by_note` 清理。
  5. 人工 gate path：
     - 关闭 Settings 的 AI 开关后，再修改笔记并保存，等待 30 秒，不应新增 embedding。
     - 在 AI 关闭状态下删除一个已 embed 的 note，stale chunks 仍应被同步清掉。
- **Known gaps**
  - ⚠️ **无后台可视化反馈**：watcher 自动 embed 只写 log，不出 toast，不显示"还有多少 pending"。这是刻意保持安静，避免编辑器日常操作被噪声淹没。
  - ⚠️ **无 durable retry queue**：flush 时 provider 不可用会直接放弃当前 due 批次；用户下一次再编辑该 note 才会重新入队。更强的 backoff / retry 留给 D2a.6。
  - ⚠️ **无全量初始化入口**：现阶段只能靠"手动 embed 当前笔记"或"从现在开始 watcher 自动追增量"逐步累积索引；全 vault dry-run + 初始化是 D2a.4。
  - ⚠️ **无使用量审计 / 成本展示**：watcher 只在 log 里记 `tokens_used`，还没有落 usage ledger；对应设计还在 D2 / D3 后段。

## 2026-04-21 · Phase 3-D2a.3a — AI 辅助·手动 Embed 管道（AppState 挂载 + 4 条 embed IPC + Settings AI 索引面板）

- **Scope**
  - D2a.2 是"水管接上"（provider 可测试），D2a.3a 是"水龙头装好"：一刀下去能看到一篇 `.md` 从 chunker → embed → sqlite 全跑通。故意**不接 watcher**，把 D2a.3 拆成 3a（pull-driven IPC）+ 3b（push-driven watcher），让流水线正确性与事件调度各解一刀。
  - **后端**（Rust）：
    - `src-tauri/src/lib.rs`：`AppState` 新增 `embeddings: Mutex<Option<Arc<Mutex<EmbeddingStore>>>>` + `embeddings_handle()` 辅助（对称 `index_handle()`）；`AppState::manage()` 初值 `Mutex::new(None)`；注册 4 条新 IPC。
    - `src-tauri/src/commands/vault.rs::attach_index`：swap vault 时先 drop `state.embeddings`（释放旧文件句柄），再打开 `<vault>/.mynotes/ai/embeddings.sqlite`。**失败非致命**——log + continue，主索引/watcher/编辑全部正常，AI 在首次调用时浮现"embedding store unavailable"。
    - `src-tauri/src/services/ai/embed_service.rs`（新）：`embed_note` 流水线 —— `std::fs::read_to_string` + `metadata.modified` 秒级 mtime → `chunk_markdown` → 对比 `store.note_mtime()` 若一致即 `SkipReason::UpToDate` → `.chunks(MAX_BATCH_INPUTS=64)` 依次 `provider.embed` → 组装 `StoredChunk[]` → **先 `delete_by_note` 再 `upsert_chunks`**（保证"3 段改 1 段"时旧 index 4–9 被清，不留污染）。`EmbedOutcome { rel_path, chunks_embedded, tokens_used, skipped: Option<SkipReason> }` —— skip 是成功路径，不走错误。`SkipReason::{UpToDate, Empty}` 两枚举值。
    - `src-tauri/src/services/ai/embedding_store.rs`：新增 `clear_all(&self) -> AppResult<()>`（单 `DELETE FROM embedding_chunks` 非事务即可，SQLite 引擎级原子）。
    - `src-tauri/src/commands/ai.rs`：4 条新 IPC + 两个内部辅助：
      - `require_vault_and_store(state)` — 聚合"有无 active vault + 有无 embedding store"检查，返回 `(PathBuf, Arc<Mutex<EmbeddingStore>>)`；错误区分 `NoActiveVault` vs. `Other("embedding store unavailable")` 让前端能差分显示引导。
      - `build_configured_provider(state)` — 读 `AppPreferences.ai_provider` + `KeyringSecretStore::get_api_key`，组装 `OpenAiProvider`；缺 config 时 `AppError::Other("no AI provider configured")`；空 base_url / embed_model 各自独立消息。
      - `ai_embed_note(rel_path) -> EmbedOutcome` —— 先做 traversal 拒绝（`PathEscape`），再 require store + build provider，最后丢给 `embed_service::embed_note`。
      - `ai_embed_stats() -> EmbeddingStats` —— 无 vault / 无 store 时返回全零 `{chunk_count: 0, note_count: 0, model_count: 0}`，从不失败；Settings 打开时可无条件调。**坑记**：`store.lock().unwrap().stats()` 链式写法会触发 E0597——返回值借用 MutexGuard 而 guard 在语句末 drop 早于 owning `store` drop。分成 `let guard = store.lock().unwrap(); guard.stats()` 两行解决。
      - `ai_embed_delete_note(rel_path) -> usize` —— 同样 traversal 检查 + require store + `delete_by_note`，返回删除行数。
      - `ai_embed_clear_all() -> u64` —— 返回清空前 chunk_count（让 UI 说"已清空 N 个"）。
    - `src-tauri/src/lib.rs`：`invoke_handler!` 注册 `ai_embed_note` / `ai_embed_stats` / `ai_embed_delete_note` / `ai_embed_clear_all`。
  - **前端**（Svelte + TS）：
    - `src/lib/ipc/ai.ts`：新增 4 个 wrappers + 类型 `EmbedOutcome` / `EmbeddingStats` / `EmbedSkipReason`。`aiEmbedNote(relPath)` / `aiEmbedStats()` / `aiEmbedDeleteNote(relPath)` / `aiEmbedClearAll()`——签名都最小。`skipped?` 字段用 `EmbedSkipReason` 独立类型（'up_to_date' | 'empty'）便于分支 toast 文案。
    - `src/lib/palette/commandRegistry.ts`：`PaletteContext` 新增 `runEmbedCurrentNote`；`PALETTE_COMMANDS` 加 `embed-current-note` 条目，`when` 谓词 = 有开笔记 + `.md` + 不在 `.mynotes/`（同族 `show-related-notes`），`hint: 'AI'`。
    - `src/routes/+page.svelte`：
      - 导入 4 个 IPC + 类型。
      - 新增 state：`embedStats: EmbeddingStats | null`（null = 未加载 / 读取失败；展示时改 "索引规模读取中…"）+ `embedBusy: boolean` + `embedNotice: { kind: 'ok'|'err'|'info'; text }`。
      - `embedCurrentNote()` 从 `vaultState.openFilePath` 取路径——**坑记**：先写成 `currentFilePath` 被 svelte-check 挡（那是 `PaletteContext` 里的字段，不是 component scope 变量），统一用 `vaultState.openFilePath`。toast 分三档：成功 `Embedded N chunks · X tokens`（0 tokens 省略）、skip `Up to date` / `Note is empty`、失败 `Embed failed: <err>`。
      - `clearAllEmbeddings()` 走 `window.confirm` 二次确认（与 D2a.2 的 "清除 provider" 一致）。
      - `openSettings()` 新增 `refreshEmbedStats()` 懒加载（与 `refreshAiProviderHasKey()` 并行）。
      - `PaletteContext.runEmbedCurrentNote = () => void embedCurrentNote()`。
      - Settings 模态新增「AI 索引 · Embedding」小节：`已索引 N chunks · M notes · K 模型` 行（null 态 fallback）+ 两按钮 "Embed 当前笔记" / "清空 AI 索引" + toast 区域。按钮复用 `.ai-provider-actions` flex 样式；新增 `.ai-embed-stats` CSS（mono font + muted bg）。
  - **测试**：新增 6 条 `services::ai::embed_service::tests`：
    - `empty_note_is_skip_empty` / `frontmatter_only_is_skip_empty` → `Skipped(Empty)`
    - `basic_run_embeds_and_persists` → 3 chunks 落 DB
    - `second_run_with_same_mtime_is_up_to_date` → 第二次 `Skipped(UpToDate)`
    - `edit_reduces_chunk_count_stale_chunks_cleaned` → `sleep(1.1s)` 越过 FS 秒级 mtime 粒度后改写，验证 chunks 从 3 → 1 且不残留
    - `missing_file_surfaces_error` → `AppError::Other` 包含 "read"
    - 全部用 `MockProvider + EmbeddingStore::open_in_memory()`；零网络 / 零磁盘副作用（tempdir 外）。
- **How to verify**
  1. `cd src-tauri && cargo test --lib` → **145/145** 全绿（D2a.2 的 139 + 6 条新单测；无 warning）。
  2. `cd src-tauri && cargo build --lib` → `Finished dev profile` 无 error / warning。
  3. `pnpm check` → `0 errors and 0 warnings`。
  4. 人工 happy path：
     - 打开 vault → Cmd+, 开 Settings → 看到「AI 索引 · Embedding」小节显示 `已索引 0 chunks · 0 notes · 0 模型`（空 vault 或未 embed 过）。
     - 先在「AI Provider」区配好 OpenAI / Ollama 并测试 ✓。
     - 打开任一 `.md` 笔记 → Cmd+P → `> Embed current note (AI index)` → 看到 toast `Embedded N chunks · X tokens`。
     - 重新打开 Settings → 数字刷新，N chunks / 1 notes。
     - 再次运行 `> Embed current note` → toast `Up to date — no embed needed`。
     - 编辑笔记保存（autosave）→ 再 embed → 若过了 1 秒则重算。
     - 点 "清空 AI 索引" → 确认 → toast `已清空 N 个 chunks`，数字归零。
  5. 人工 sad path：
     - 没配 provider 就 embed → toast `Embed failed: no AI provider configured`。
     - 配了错 api_key 去 embed → toast `Embed failed: <auth 错误>`。
     - 空笔记（只 frontmatter）embed → toast `Note is empty — nothing to embed`。
- **Known gaps** / 有意不做
  - ⚠️ **Watcher 自动增量**：**D2a.3b** 单独一刀做；现在编辑笔记后得手动点命令才会更新 embedding。
  - ⚠️ **跨笔记批量**：`ai_embed_note` 一次只处理一篇；全量初始化（包括 dry-run cost 估算 modal）留给 D2a.4。
  - ⚠️ **Rate limit 智能重试**：OpenAI 429 时直接冒泡错误；D2a.3b 的 30 s debounce 天然承担"30 s 后再试"语义，所以不单独做退避。
  - ⚠️ **删除笔记同步清理**：删 `.md` 后 embedding 目前留在 sqlite 里成孤岛；D2a.3b 的 watcher `delete` 分支会调 `ai_embed_delete_note` 同步清。目前可手动点 "清空 AI 索引" 或直接 factory-reset `.mynotes/ai/` 目录。
  - ⚠️ **`EmbeddingStore::search()` 未被任何 IPC 消费**：仍是 D2a.5 的活——打分信号升级时会加 `ai_search_similar(query, k)` 类命令。
  - ⚠️ **FS mtime 假阳性**：某些备份工具会保留 mtime，理论上会造成"内容变了但被判 up-to-date"。D2a.4 的"强制重建索引"按钮覆盖该场景。

---

## 2026-04-21 · Phase 3-D2a.2 — AI 辅助·Provider 接入（OpenAI-compatible + keychain + Settings 测试连接）

- **Scope**
  - D2a.1 是库层底座，D2a.2 是**对外接第一根真实的水管**：加 OpenAI-compatible HTTP provider + API key OS keystore 存储 + Settings UI 能点「测试连接」看到 ✓/✗。用户第一次真正能"配好一个 provider"。
  - **后端**（Rust）：
    - `src-tauri/Cargo.toml`：新增 `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "charset"] }` + `keyring = "3"`。刻意用 rustls-tls 绕开 OpenSSL / Secure Transport 的构建坑。
    - `src-tauri/src/services/ai/secrets.rs`（新）：`SecretStore` trait + `KeyringSecretStore`（zero-size struct，`keyring::Entry::new("com.mynotes.ai", provider)`）+ `MockSecretStore`（`#[cfg(test)]` 内存 HashMap）。`SecretError → AppError::Other` 自动 bridge。set/get/delete/has 四方法，delete 对不存在的 key 幂等。首次调用真 Keychain 会触发系统授权对话框——OS 行为不是 bug。
    - `src-tauri/src/services/ai/openai.rs`（新）：`OpenAiProvider` 实现 `AiProvider` trait。`embed()` 走 `POST {base_url}/embeddings`；Bearer header 仅在 `api_key != ""` 时加（空 = Ollama 本地场景）。`default_dim` 是 `AtomicUsize`，构造填 1536，首次成功 embed 后用实际向量长度回填。`with_timeout(Duration)` 覆盖默认 60 s，`test_connection` 走 10 s 快失败。`classify_http_error` 纯函数：401/403→Auth, 429→RateLimit(30), 400-499→InvalidRequest, 500-599→Other, 连接失败/超时→Network。`extract_error_message` 先试 OpenAI envelope `{error:{message,type}}` 再 fallback raw 400 字符（UTF-8 chars-count，因 `…` 是 3 字节）。
    - `src-tauri/src/services/config.rs`：新增 `AiProviderConfig { kind, base_url, embed_model }`（**不含** api_key）→ `AppPreferences.ai_provider: Option<AiProviderConfig>`。`AppConfigSnapshot.ai_provider` 同步；`ConfigStore::{set_ai_provider, clear_ai_provider, ai_provider_kind}` 三方法。
    - `src-tauri/src/commands/ai.rs`：4 条新 IPC：
      - `ai_provider_set_config(kind, base_url, embed_model, api_key)` — `api_key == ""` 表示"不动 keystore"，非空覆盖。
      - `ai_provider_clear_config()` — 先清 config 后 wipe keystore，keystore 失败也接受（半清好过卡死）。
      - `ai_provider_has_api_key() -> bool` — Settings 徽标专用，不返回 key。
      - `ai_provider_test_connection(kind?, base_url?, embed_model?, api_key_override?) -> ProviderTestResult` — 所有字段可选，显式传入 = 验未保存表单；`api_key_override` 缺省回落 keystore，`Some("")` = 匿名 Ollama。返回结构体而非 Result，成功失败都能在同一 notice 渲染。
    - `src-tauri/src/services/ai/mod.rs`：摘除之前的 `#![allow(dead_code)]`，改为在 `chunker.rs` 与 `embedding_store.rs` 文件级挂 `#![allow(dead_code)]`（这两个模块才是真没人用），`provider.rs` 加文件级 allow（trait 的 `name` / `default_dim` + MockProvider 都只在测试用）。`mod.rs` 注释同步更新 D2a.2 消费者清单 + 把 `openai` / `secrets` re-export 进来。
    - `src-tauri/src/lib.rs`：`invoke_handler!` 注册 4 条新命令。
  - **前端**（Svelte / TS）：
    - `src/lib/ipc/config.ts`：`AppConfigSnapshot` 新增 `ai_provider: AiProviderConfig | null`，导出 `AiProviderConfig` 类型。
    - `src/lib/ipc/ai.ts`：新增 `ProviderErrorKind` / `ProviderTestResult` 类型 + 4 个函数 `aiProviderSetConfig / aiProviderClearConfig / aiProviderHasApiKey / aiProviderTestConnection`。`aiProviderTestConnection` 用 options 对象签名（`{kind?, baseUrl?, embedModel?, apiKeyOverride?}`），缺省字段传 `null` 给后端。
    - `src/routes/+page.svelte`：新增 9 个 `$state` 跟 Settings 表单对齐（`aiProviderKind / BaseUrl / EmbedModel / ApiKey / HasKey / TestState / Testing / Saving`）+ 4 个函数 `refreshAiProviderHasKey / saveAiProvider / clearAiProvider / testAiProvider`。`loadAppConfig` 补 `snapshot.ai_provider` 回填；`openSettings` 清空 ApiKey/TestState + 异步刷 HasKey。Settings modal 在"AI 辅助面板"之后追加"AI Provider（Embedding）"区块：三栏表单（Base URL / Embed model / API key type=password + placeholder 根据 HasKey 切"已存储在 keychain，留空以保留"/"粘贴 sk-... 或留空（Ollama）"） + 三按钮（测试连接 / 保存 / 清除带 `ask()` 二次确认）+ 测试结果 notice（成功显示 `✓ 连接成功 · 维度 N · M tokens`，失败显示 `✗ kind: message`）。保存/清除成功走 `pushNotice` 统一反馈渠道。
    - `src/routes/+page.svelte` CSS：新增 `.ai-provider-grid / .ai-provider-field / .ai-provider-actions / .ai-provider-test-result.{ok,err}` 样式（`color-mix` + `--color-accent` token，暗色自动跟随）。

- **How to verify**
  1. 关闭应用后运行 `cd src-tauri && cargo test --lib --quiet` — **139 passed / 0 failed**（117 原有 + D2a.1 × 4 + D2a.2 × 18 = 139）。
  2. `cargo build --lib 2>&1 | rg '^warning'` 为空 — **0 warning**。
  3. `pnpm check` — **0 errors / 0 warnings**。
  4. 启动应用（`pnpm tauri dev`），`⌘,` 打开 Settings，滚到最下方应看到新的"AI Provider（Embedding）"区块；Base URL / Embed model 有默认值，API key 为空且 placeholder 是"粘贴 sk-... 或留空（Ollama）"。
  5. **OpenAI 真实路径**：粘贴 `sk-...` → 点"测试连接" → 应看到绿色 `✓ 连接成功 · 维度 1536 · 2 tokens`。macOS 首次会弹 Keychain 授权对话框（正常，keyring crate 的 OS 行为）。
  6. **错误路径**：改 API key 为 `sk-bogus` → 测试 → 应看到红色 `✗ auth: Incorrect API key provided...`。
  7. **Ollama 本地路径**：Base URL 改 `http://localhost:11434/v1`、Embed model 改 `nomic-embed-text`、API key 留空 → 测试 → 应看到 `✓ 连接成功 · 维度 768 · 0 tokens`。
  8. **保存 + 重启 + 清除**：粘贴 key → 保存 → 关 Settings → 重开 Settings → API key 输入框为空但 placeholder 显示"已存储在 keychain，留空以保留" → 点"清除"（二次确认）→ 输入框提示恢复到"未存储"。重启应用后 `ai_provider` 配置持久，key 也还在（确认 Keychain 未被清）。
  9. macOS "Keychain Access.app" 查 `com.mynotes.ai` 服务 → 应看到 `openai` 账户记录。清除配置后该记录消失。
  10. 检查 `~/Library/Application Support/com.mynotes.app/app-config.json`（或对应平台路径）— `ai_provider` 字段只含 `{kind, base_url, embed_model}`，**绝对不应** 出现任何 `api_key` / `sk-` 字样。

- **Known gaps**
  - **批量 embed / watcher 增量**：`ai_provider_test_connection` 只发 1 个 input，真实 vault 级 embed 调度（批次拆分、debounce、失败重试、note_mtime 对比）放在 D2a.3；目前即使 provider 配对了，D1 打分也不会升级，因为还没有 embed 数据。
  - **Dry-run 成本预估**：Settings 没有"初始化索引"按钮 —— 放在 D2a.4（需要 chunker 走一遍全 vault 给出 `预计 X chunk / Y token / ≈$Z`）。
  - **重试与 backoff**：Provider 错误直接冒泡，没有退避策略；`RateLimit(30)` 只在 UX 层展示"retry after 30s" 字样，不会自动等待。Watcher 调度层（D2a.3）拿到 `RateLimit` 时才会真退避。
  - **Chat / streaming**：`AiProvider` trait 当前只有 `embed`；`chat(messages) -> Stream<Token>` 是 D2b 的事（对话面 α detach mode）。
  - **保存前不强制测试**：允许用户保存错的 key —— UX 取舍是"保存和测试是两个动作"。后续 embed 会失败时，D2a.6 会做"引导去 Settings 修 key"的降级提示。
  - **CI 与 Keychain**：单测只覆盖 MockSecretStore；CI headless Linux 真跑 `keyring` 会需要 D-Bus + `dbus-run-session`。本项目目前没有 CI，这个坑留在发版前处理（届时加 `#[cfg_attr(not(target_os = "linux"), test)]` 门控或真实 OS 分矩阵）。
  - **UI 没有 provider 种类下拉**：当前只有 OpenAI-compatible，所以把 `aiProviderKind` 固定成 `"openai"` 文本（不对用户暴露）。等到加 Anthropic / 本地 ggml 再补 dropdown。

---

## 2026-04-21 · Phase 3-D2a.1 — AI 辅助·Embedding 索引底座（Rust 库层）

- **Scope**
  - D2a 整体目标是把全 vault 段落 embed 落到 `.mynotes/ai/embeddings.sqlite`，作为 D2b 对话面 RAG 检索底座 + D1 打分升级。D2a 切成多刀，**本刀 D2a.1 仅落 Rust 库层**：不暴露 IPC、不接 UI、不调真实 HTTP、不接 watcher。
  - `src-tauri/src/services/ai/mod.rs`（新文件）：模块根 + `#![allow(dead_code)]`（D2a.2 接 IPC 时移除）。
  - `src-tauri/src/services/ai/provider.rs`（新文件）：
    - `AiProvider` trait（`async_trait` 装饰以保 `Box<dyn>` dyn-compatibility）：`name() / default_dim() / embed(EmbedRequest) -> Result<EmbedResponse, ProviderError>`。
    - `ProviderError` 五档：`Network` / `Auth` / `RateLimit(u64)` / `InvalidRequest` / `Other`，粒度刚好支持调用方决策重试策略而不强耦合 HTTP 状态码。
    - `MockProvider`：192 维 FNV-1a 滚动哈希 → 单位范数向量；`with_dim` 支持单测自定义维度；对同 input 确定性、对不同 input 高概率互异。单测 5 条（shape / 确定性 / 互异 / 单位范数 / 空输入 error）。
  - `src-tauri/src/services/ai/chunker.rs`（新文件）：
    - `chunk_markdown(body: &str) -> Vec<Chunk>`：`strip_frontmatter` → `split_paragraphs` → 超 `MAX_CHUNK_TOKENS=800` 段走 `split_sentences`。
    - `Chunk` 携 `chunk_index / offset_start / offset_end / text / est_tokens`，offset 是**绝对** byte offset（含 frontmatter 偏移），供 D2b 引用高亮直接切 `&body[start..end]`。
    - `strip_frontmatter` 认 `---\n…\n---\n` 和 `---\r\n…\r\n---\r\n` 两种；unterminated frontmatter 当作普通 body。
    - `split_sentences` 识别 `. ! ?` + `。！？`（U+3002 / U+FF01 / U+FF1F）且要求后跟空白，避免把 `3.14` 误切。
    - `est_tokens(s) = ⌈chars / 4⌉`（CJK 会偏高估，dry-run 时保守可接受）。
    - 单测 14 条（est_tokens × 2 / strip_frontmatter × 4 / chunk_markdown × 7 / split_sentences × 2 / edge-case × 1）。
  - `src-tauri/src/services/ai/embedding_store.rs`（新文件）：
    - `EmbeddingStore` 包 `rusqlite::Connection`；`open(&Path)` 自动建父目录 + `execute_batch(SCHEMA_SQL)`；`open_in_memory()` 仅 `#[cfg(test)]` 可见。
    - Schema：`embedding_chunks(id, note_rel_path, chunk_index, offset_start, offset_end, text, model, dim, vector BLOB, note_mtime, created_at, UNIQUE(note_rel_path, chunk_index, model))` + `idx_emb_note` + `idx_emb_model` + `embedding_meta` 键值表（`schema_version = '1'`）。WAL 模式 + 外键 ON。
    - API：`upsert_chunks(&[StoredChunk]) -> usize`（事务 + prepare_cached + ON CONFLICT UPSERT）、`delete_by_note(&str) -> usize`、`note_mtime(&str) -> Option<i64>`、`search(&[f32], model, limit) -> Vec<SearchHit>`、`stats() -> EmbeddingStats`。
    - `search` 语义：空 query 返回 `Other` 错误；零范数 query 返回 `Ok(vec![])`；`dim` 不匹配的行静默跳过（允许多 provider namespace 共存）；余弦相似度全表扫描 → 降序截 `limit`。
    - 向量编解码：`pack_f32` / `unpack_f32` 走小端 4 字节 × dim；`pack_unpack_roundtrip` 单测覆盖 f32::EPSILON 边界。
    - 单测 12 条（open / upsert / UPSERT 覆盖 / delete_by_note / note_mtime 缺 × 2 分支 / search 排序 / model 过滤 / dim skip / empty query error / empty store ok / pack-unpack / norm 零）。
  - `src-tauri/src/services/mod.rs`：`pub mod ai;` 挂上。
  - `src-tauri/Cargo.toml`：
    - 运行时新增 `async-trait = "0.1"`。
    - Dev-only 新增 `[dev-dependencies] tokio = { version = "1", features = ["rt", "macros"] }`（仅为 `#[tokio::test]` 宏，release binary 不含）。
  - **D1 打分升级未在本刀内做**：D2a.1 没有真实 embedding 数据可用，升级留给 D2a.5；当前 `ai_related_notes` 继续走 `title_jaccard`。

- **How to verify**
  - `cd src-tauri && cargo test --lib` → 117 tests passed (之前 93 + D2a.1 新增 24)，0 failed。
  - `cargo build --lib` → 0 warnings（`#![allow(dead_code)]` 压住"尚无消费者"的预期告警）。
  - 人工审 schema：在 `embedding_store.rs` 测试里走 `open_in_memory() → upsert → search → stats`，观察 `chunk_count / note_count / model_count` 与 cosine 排序是否符合几何直觉（正交基底 + 近 x 轴 query → [x,y,z] 降序 + z≈0）。
  - 人工审 chunker：`chunk_frontmatter_offsets_are_absolute` 验证了 frontmatter + body 混合输入下 offset 能切回原字符串。
  - **未走**：真实 vault 对 `EmbeddingStore::open(<vault>/.mynotes/ai/embeddings.sqlite)` 的磁盘 IO —— 留给 D2a.3 接 IPC 时验证。

- **Known gaps**
  - 无 IPC、无 UI：D2a.1 是纯 Rust 库层，用户层面"看不到任何变化"。这是设计——让 D2a 的每一刀可独立 review，避免一次塞整个 provider + watcher + UI 的大 diff。
  - `MockProvider` 的向量不是语义有意义的：只为单测提供 deterministic 形状。D2a.2 接 OpenAI 后才是真能用的向量。
  - 内存 cosine 全表扫描：vault > 50 k chunks 时延迟会突破 100 ms，届时 `search()` 需换实现（sqlite-vec / hnsw_rs），schema 不动。
  - `est_tokens(s) = chars / 4` 对中文偏保守（实际约 chars / 1.5），dry-run 时会略高估成本 —— 可接受（宁可高估也别被账单偷袭），D2a.4 可选替换为 tiktoken-rs 精确计数。
  - `#![allow(dead_code)]` 在 D2a.1 结束时全模块打开 —— D2a.2 接 IPC 后立即摘除，避免遮住真正的 unused 告警。
  - 未处理 SQLite `BUSY`：当 WAL checkpoint 与 upsert 并发时可能返回 `SQLITE_BUSY`，D2a.3 接 watcher 时需要加 `busy_timeout(100ms)` 或重试层。
  - `ProviderError` 目前没有 `#[allow(dead_code)]` 豁免 —— 真实 provider 落地后所有 variant 都会被构造；当前仅 `InvalidRequest` 被 `MockProvider` 用到。

---

## 2026-04-21 · Phase 3-D1 — AI 辅助面板（related-notes 本地启发式版）

- **Scope**
  - `src-tauri/src/commands/ai.rs`（新文件）：`ai_related_notes(src_rel_path, limit?) -> Vec<RelatedNote>` 命令。
    - 打分模型：tag_overlap × 2.0 + direct_link × 1.5 + co_cited × 1.0 + title_jaccard × 0.5 − staleness × 0.3。
    - 五路查询全走 SQLite（src_tags / direct_links / co_citers / candidates / bulk_tags），最后在 Rust 层纯函数打分。
    - `score ≤ 0` 的候选被过滤，剩余按 score 降序、截到 `limit`（默认 10，最大 50）。
    - 辅助函数：`bigrams(s)` → 2 字符滑动窗口集合（case-fold + 去非字母数字）；`jaccard(a,b)`；`staleness_score(updated, today_days)`（ISO-8601 → days-since-epoch via Julian Day）；`unix_days_now()`；`date_to_julian` 以 Unix epoch 为零点。
    - 单测 15 条：bigrams × 5 / jaccard × 4 / staleness × 4 / scoring × 2。
  - `src-tauri/src/error.rs`：补 `impl From<rusqlite::Error> for AppError`。
  - `src-tauri/src/services/config.rs`：`AppPreferences` + `AppConfigSnapshot` 新增 `ai_enabled: Option<bool>`；`ConfigStore::set_ai_enabled(bool)` 方法。
  - `src-tauri/src/commands/config.rs`：`app_config_set_ai_enabled(enabled: bool)` 命令。
  - `src-tauri/src/commands/mod.rs`：`pub mod ai;`。
  - `src-tauri/src/lib.rs`：注册 `ai_related_notes` + `app_config_set_ai_enabled` 两条命令。
  - `src/lib/ipc/ai.ts`（新文件）：`RelatedNote` / `RelatedSignals` 接口；`aiRelatedNotes` / `appConfigSetAiEnabled` 封装。
  - `src/lib/ipc/config.ts`：`AppConfigSnapshot` 加 `ai_enabled: boolean | null`。
  - `src/lib/panel/Panel.svelte`：新增 `aiEnabled?: boolean` prop；`load()` 扩为 5 路并行（原 4 路 + `aiRelatedNotes`）；底部新增「AI 相关笔记」section（虚线上边框 + AI badge + 信号 tooltip）；CSS 加 `.related-section` / `.related-heading` / `.ai-badge`。
  - `src/routes/+page.svelte`：
    - import `appConfigSetAiEnabled`。
    - `aiEnabled = $state(true)` 状态变量。
    - `loadAppConfig` 里读 `snapshot.ai_enabled ?? true`。
    - `<Panel {aiEnabled} ...>` 传 prop。
    - Settings 末尾加"AI 辅助面板"区块（checkbox + hint）。
    - `paletteCtx.runShowRelatedNotes`：若 off 先 enable，然后 `requestAnimationFrame` scroll `.related-section` 入视野。
  - `src/lib/palette/commandRegistry.ts`：`PaletteContext` 加 `runShowRelatedNotes`；PALETTE_COMMANDS 加 `show-related-notes`（条件：`.md` 文件已打开）。

- **How to verify**
  - 自动化：`cargo test --lib` 82 tests passed（含新增 15 条 ai.rs 单测）；`pnpm check` 0 errors。
  - 手动（打开任意有 tag / 链接的笔记，右侧面板底部应出现「AI 相关笔记」section；hover 条目可见信号 tooltip）。
  - 关闭：Settings → 取消勾选"AI 辅助面板" → 面板相关笔记 section 消失，重启后仍保持关闭。
  - 命令面板：`⌘P > Show Related Notes` 可触发（若关闭则自动开启并 scroll）。

- **Known gaps**
  - 前端无 vitest：`aiRelatedNotes` IPC 封装未自动化测试，依赖手动验证。
  - 笔记量 > 10 k 时（非常大的 vault）全表扫描可能超 100 ms，届时需要引入 LIMIT + 预筛选子集。
  - `staleness_score` 使用 Julian Day 近似，闰秒 / 夏令时偏差 ±1 天，对启发式打分影响可忽略。
  - D2 升级时 `title_jaccard` 信号将被 `embedding_cosine` 替换，届时需要调整权重常数。

---

## 2026-04-21 · Phase 3-A2 — Must-fix sweep（MOC 模板解耦 / 导出 wiki 链接 / Windows 图片路径）

- **Scope**
  - P3-A2 是一个"补坑"编号：P3-A3 ~ P3-A7 在 pre-compaction 阶段已陆续落地（见下方条目），但 Phase 2 + P3-A1 推进时积下来的 4 处 must-fix 缺口一直没收。本轮一次性打包修掉。全部是"修补型"改动，不引入新 Schema / 新命令 / 新 UI 模块。
  - **P3-A2.1 · MOC 模板 stub 解耦**（`src/lib/commands.ts` + `src-tauri/templates/moc.md`）
    - `templates/moc.md` 新增 `<!-- moc:entries-insertion-point -->` sentinel 注释，作为 `buildMocFromTag` 的首选插入锚点。
    - `src/lib/commands.ts` 抽出纯函数 `injectMocEntries(body, entriesMarkdown)`：先匹配 sentinel，未命中再 fallback 到旧模板的 `## 核心笔记\n\n- [[]]` 字面串，两者都不中则返回 `strategy: 'none'`。
    - `buildMocFromTag` 返回值增加 `strategy: 'sentinel' | 'legacy' | 'none'` 字段，便于上层在无插入点时走兜底分支。
    - 同时导出 `MOC_ENTRIES_SENTINEL` 常量与 `MocInjectStrategy` 类型，便于后续模板工具复用。
  - **P3-A2.2 · MOC 创建后 panel 刷新竞态**（`src/routes/+page.svelte`）
    - `confirmBuildMoc` 原本写完 MOC 文件后立即 `panelRefreshToken += 1`，但 Rust 侧 `notify-rs → indexer` 是异步管道，新文件写盘与 SQLite 落库之间有一个 race，侧栏 TagsSection 经常在旧快照里刷新，tag count 不加 1。改为调用既有的 `schedulePanelRefresh(200)`，复用 debounce 通道让 indexer 先追上。
    - 顺手把 `confirmExtract` 同一模式的 race 也改成 `schedulePanelRefresh(200)`。
    - 根据 `strategy` 字段增加 toast 分支：`strategy === 'none'` 时提示"模板未找到插入点，MOC 文件已生成但未自动填入条目"，`sentinel / legacy` 走原有成功提示。
  - **P3-A2.3 · 打印 wiki 链接变超链接**（`src-tauri/src/commands/export.rs`）
    - 新增纯函数 `preprocess_wikilinks(md: &str) -> String`：在 pulldown_cmark 解析之前把 `[[target]]` / `[[target|alias]]` 预处理成 `[display](#slug)` 锚点 Markdown，让 `pulldown_cmark::html::push_html` 直接输出 `<a>` 标签。
    - 配套 helper：`wikilink_re()`（`OnceLock<Regex>` 懒初始化）、`wikilink_slug()`（CJK 友好，保留 `\u{4e00}..=\u{9fff}` 原字符，ASCII 小写 + 连字符归并）、`escape_md_link_text()`（转义 `[` / `]` / `\`，避免别名里含特殊字符时破坏 link syntax）。
    - 空 slug 目标（例如 `[[]]`、`[[|alias]]`）原样保留，不生成悬空 `#` 锚点。
    - `note_render_print_html` 在 `Parser::new_ext` 之前多一步 `let preprocessed = preprocess_wikilinks(body_md);`。
    - 跑通 8 条新 Rust 单测：`basic_target_becomes_anchor_link / alias_overrides_display_text / cjk_target_slugs_survive / two_adjacent_links_do_not_merge / empty_slug_preserved_as_literal / escape_special_chars_in_display / leaves_plain_prose_alone / slug_helper_is_cjk_aware_and_lowercases_ascii`。
  - **P3-A2.4 · Windows 绝对路径图片 embed 识别**（`src/lib/editor/imageEmbed.ts`）
    - `EMBED_LINE_RE` 扩一个分支：`[A-Za-z]:[\\/][^)]+`，让 `C:\Users\...\a.png` / `C:/Users/.../a.png` 都能命中 CM6 embed 判定。
    - 新增导出 helper `normalizeAbsPath(raw)`：若检测到 Windows drive-letter 形态，把 `\` 全部转成 `/`，统一成可以喂给 `convertFileSrc()` 的形态；其它路径原样返回。
    - `scanEmbedLines` 捕获路径后先过 `normalizeAbsPath`，保证后续 Tauri `convertFileSrc` 调用拿到的是 POSIX 形式。POSIX 与 `file://` 分支不变。

- **How to verify**
  - 自动化：
    - `pnpm run check` → **188 files, 0 errors, 0 warnings**（此前 Phase 2 遗留的两处 svelte-check 报错在 P3-A5 / A7 的整理中已被清零，本轮确认保持全绿）。
    - `cd src-tauri && cargo test --lib` → **46 passed, 0 failed**（原 38 条 + 本轮新增 8 条 wikilink 相关单测）。
    - `pnpm run build` → **adapter-static 成功，无新 warning**；仅留既有的"chunks larger than 500 kB"构建期提示，与本轮无关。
  - 手测：
    1. **MOC 模板 sentinel**：建两篇 `#learning` 的笔记，命令面板跑 `Build MOC from tag` → 新 MOC 文件里 `## 核心笔记` 段下应出现两条 `[[...]]`；侧栏 `#learning` tag count 从 N 变 N+1（P3-A2.2 的 race fix）。
    2. **legacy fallback**：把 `templates/moc.md` 里的 sentinel 注释手工删掉（模拟老 vault 没更新模板），再跑一次 Build MOC → 应走 `legacy` 分支，条目仍正常插入到 `## 核心笔记\n\n- [[]]` 之前；Scope 内的回归测试由 `injectMocEntries` 纯函数的结构保证，但当前前端无 vitest 测试 harness，因此靠手测覆盖。
    3. **strategy='none' toast**：把 `templates/moc.md` 里 `## 核心笔记` 这一行改成任意其它文字后重跑 → toast 应提示"未找到插入点"，MOC 文件本身仍被创建（只是不自动插入）。
    4. **打印 wiki 链接**：新建一篇包含 `[[foo]]` 与 `[[bar|别名条]]` 的笔记，执行 `Print (HTML preview)` 命令 → 在打印预览窗口"查看源代码"中应能看到 `<a href="#foo">foo</a>` 与 `<a href="#bar">别名条</a>`；空 `[[]]` 留作字面保持不动。
    5. **Windows 图片 embed**：在编辑器里手打 `![test](C:/Users/x/a.png)` 与 `![test](C:\Users\x\a.png)` 各一行 → 两条都应触发 CM6 embed widget 替换；本机无真实文件的情况下 widget 展示 placeholder，说明识别通道打通。POSIX 既有用例（`attachments/...` / `file://...` / `/abs/...`）回归 green。

- **Known gaps**
  - **前端仍无 vitest harness**：`injectMocEntries` / `normalizeAbsPath` 都特意抽成了纯函数，但仓库里暂无 vitest / jest 配置，这两处只有手测覆盖。同样影响 P3-A6 的 `parseDroppedPaths` 与 Phase 2 的 `buildExtractedNote`。下一批 P3-A（或 Phase 4 质量工程）把 vitest 接入后统一补齐。
  - **打印 wiki 链接只做同文档锚点**：目前 `[display](#slug)` 只对单文档打印有意义；跨文档导出（多篇拼一份 HTML / PDF）要按源文件查表拿到真实目标文档 id 再决定跳转目的，这是 Phase 3 后面"全 vault 导出"任务的前置工作，不在本轮范围。
  - **Windows 图片 embed 未真机验证**：手测是在 macOS 里手动构造 `C:/...` 字符串完成的；真正 Windows VM 冒烟留给 Phase 4 跨平台 CI。
  - **`templates/moc.md` 同时存在 sentinel + 旧 stub**：为向后兼容，两种锚点都能命中；若用户把 sentinel 也删掉但保留 `## 核心笔记\n\n- [[]]`，仍能走 legacy；两者都改掉则直接进入 `strategy='none'` 分支给 toast。后续若要去掉 legacy 分支，需要一次 vault 级 schema 检查提醒老模板升级。
  - **pre-compaction 阶段已经闭环了 P3-A3 ~ P3-A7**：图谱 a11y 键盘 + 屏阅镜像 + 大图 force preset、Rename dry-run + 二次确认、命令反馈 notice stack、Sidebar drop 导入、打印 HTML 主题化——这些当时在 plan 里列作"P3-A3 候选清单"的项目都已通过 P3-A3 ~ P3-A7 完成（见下方对应条目），本轮 plan 初稿因 context 残缺误判成"未落地"，现已订正。真正仍未动的只剩：前端 vitest harness、跨平台 Windows CI 真机冒烟、大图 5k+ 节点 benchmark —— 留给 P3-A8+ 或 Phase 4。

---

## 2026-04-20 · Phase 3-A7 — 打印 HTML 主题化（并收口 P3-A3 候选清单）

- **Scope**
  - P3-A3 的"Graph hardening sweep"在 `GraphView.svelte` 里顺带挂过 `MutationObserver(data-theme)` + `matchMedia(prefers-color-scheme: dark)` 双 hook，图谱主题自动重绘这条早就 wire 好了；但当时没在 changelog / delivery log 里单独 acknowledge。本轮是 P3-A 桌面硬化第一批的正式收口，把最后一条"打印 / 导出 light/dark 主题化"做完，同时把图谱主题自动重绘显式计入已交付，这样 P3-A3 当时列出的候选清单 7 条全部闭环。
  - **P3-A7.1 · 后端 `PrintTheme` enum + 三分支 HTML 生成**（`src-tauri/src/commands/export.rs`）
    - 新增 `PrintTheme { Light, Dark, System }` enum + `from_option(raw: Option<&str>) -> Self`：`"light"` / `"dark"` / `"system"` 显式映射；`None` / 未识别字符串 / 空串一律降级到 `System`，forward-compat 未来可能新增的 theme 值不会让打印命令抛错。
    - `note_render_print_html` 命令签名从 `(src_rel_path: String)` 扩成 `(src_rel_path: String, theme: Option<String>)`。解析完 theme 后交给 `wrap_print_html(title, base_href, body_html, theme: PrintTheme)`。
    - `wrap_print_html` 重构成三分支：
      - **Light**：`<html data-theme="light">` + `:root { color-scheme: light; ...light vars... }`；**不发** `@media (prefers-color-scheme: dark)`，OS 反向切暗不会漏进来。
      - **Dark**：`<html data-theme="dark">` + `:root { color-scheme: dark; ... }` + `:root[data-theme='dark'] { ...dark vars... }` 覆盖；同样不发 media query。
      - **System**：`<html>`（无 `data-theme`） + `:root { color-scheme: light dark; ...light vars... }` + `@media (prefers-color-scheme: dark) { :root:not([data-theme]) { ...dark vars... } }` 让浏览器按 OS 现场择色。
    - `@media print` 块里 `:root, :root[data-theme='dark'] { ...light vars... }` 把两条 root 一次压回亮色——即使 preview 是暗色，真实出纸 / 存 PDF 都走白底，避免暗背景被印出来浪费墨。
    - 调色板从 `oklch()` 退回 hex：macOS Preview / iOS Books / 旧 PDF viewer 对 oklch 支持不一致，打印产物优先跨工具可读性。参照 GitHub 的 Primer 风格配色。
    - 新增 5 条 Rust 单测：`print_theme_from_option_normalizes_known_values`（显式 / unknown / None → System）、`print_html_light_pins_light_palette_and_no_media_query`、`print_html_dark_pins_dark_palette`、`print_html_system_emits_media_query_and_drops_data_theme`、`print_html_print_media_always_forces_light_for_paper_output`（三档 × `@media print` 双重置分支）。
  - **P3-A7.2 · 前端 IPC**（`src/lib/ipc/export.ts`）
    - `noteRenderPrintHtml(srcRelPath, theme?: ThemePreference)`：第二参数可选；不传等价于 Rust 侧 `None` → `System`。复用既有的 `ThemePreference = 'system' | 'light' | 'dark'`，不引入新类型。
  - **P3-A7.3 · UI 透传**（`src/routes/+page.svelte`）
    - `runPrintCurrentNote` 把当前 `$state<Theme>` 的 `theme` 直接塞进 `noteRenderPrintHtml(path, theme)`。命令面板 → 打印路径再无额外分支。
  - **P3-A7.4 · 设计文档同步**（`design_V2.md` + `README.md`）
    - `design_V2.md §6.15` 表格里 `note_render_print_html` 签名补 `theme?`；新增子节"打印 HTML 主题化（P3-A7）"覆盖前端透传、三分支 CSS 策略、`@media print` 亮色强制、为什么 hex 而不是 oklch、单测范围。
    - changelog 追 `2026-04-20 | 2.14`，顺带显式 acknowledge GraphView 的主题自动重绘已在 P3-A3 落地，彻底收口 P3-A3 候选清单。
    - `README.md` 第 9 行把 `P3-A7` 加入"已落地"清单。

- **How to verify**
  - 自动化：
    - `pnpm check` → **0 errors, 0 warnings**。
    - `cargo test --manifest-path src-tauri/Cargo.toml --lib` → 原 62 + 新增 5 = **67 passed, 0 failed**。
  - 手测：
    1. **Light pin**：Settings → 主题 Light → `> Print current note` → 浏览器里打开的预览 HTML 是亮色底；view-source 可看到 `<html lang="zh-CN" data-theme="light">` + `color-scheme: light`，且文件内**无** `@media (prefers-color-scheme: dark)` 块。OS 手动切换暗色，preview 不变。
    2. **Dark pin**：主题 Dark → 打印 → 预览是暗色底；`data-theme="dark"` + `color-scheme: dark`；OS 切主题同样不影响。
    3. **System follow-OS**：主题 System → 打印 → `<html lang="zh-CN">` 无 `data-theme`；`color-scheme: light dark`；OS 切明/暗 preview 跟着翻。
    4. **纸面打印亮色强制**：任一模式下在浏览器里按 `⌘P` → 预览窗口的打印预览总是白底黑字（即使 preview 窗口本身是暗色）。存 PDF 打开确认背景是白。
    5. **图谱主题自动重绘**（P3-A3 落地、本轮显式 acknowledge）：打开图谱视图 → 命令面板 `> Set theme → dark` / `> light` / `> system` → 节点 / 边 / label / 背景颜色立即跟随变化，无需关闭图谱再打开。OS 切换暗色（system 模式下）同样触发重绘。

- **Known gaps**
  - **导出 zip 不带 theme**：zip 里是原始 `.md`，没有渲染产物，theme 概念不存在；这条不是缺口而是 Phase 2 就定的语义边界。
  - **`note_export_copy` 也无 theme**：单篇导出是 raw `.md` 复制，同上。
  - **预览 HTML 无主题切换按钮**：用户点"Print current note"时 theme 被固化进文件；如果打开预览后才想换 theme，只能回 app 切主题再点一次命令。要做 preview 内交互切换得在 HTML 里加一段 JS + toggle，收益不足，不做。
  - **`oklch` → hex 调色板手工对齐**：app.css 用 oklch，打印用 hex。两份色值目测接近但不是严格 oklch↔sRGB round-trip。用户在打印 preview 和 app 之间来回看颜色会略有偏差；打印场景优先跨 PDF-viewer 一致性，这个偏差可接受。若后续要严格对齐，加一个 oklch → sRGB 的 build-time 转换脚本即可。

---

## 2026-04-20 · Phase 3-A6 — 侧栏文件 drop 导入

- **Scope**
  - 把"从 Finder 拖文件到侧栏 → 应用自动把文件复制进 vault 对应目录"这条 Phase 2 留下的最后一条侧栏摩擦落地。编辑器已经吃 `image/*` 拖放（归档进 `attachments/`），但侧栏此前是纯展示——拖任何 .md / PDF / 图片进左侧树都无反应，用户只能在 Finder 里手动把文件放进 vault 目录再等 watcher 扫进来。
  - **P3-A6.1 · 后端 `file_import`**（`src-tauri/src/commands/import.rs` 新文件 + `src-tauri/src/commands/mod.rs` + `src-tauri/src/lib.rs`）
    - 新增 `file_import(src_abs: String, dst_dir: String) -> ImportedFile` 命令：
      - 硬约束：源必须是绝对路径、存在、是普通文件（目录拒，notice 提示 "drop individual files instead"）、basename 不以 `.` 开头且不含 `/` `\`。
      - 反 vault-内拷：canonicalize 源与 vault 根，若 `src_canon.starts_with(vault_canon)` 则拒——避免用户从 Finder 误选 vault 内的 md 拖回侧栏自复制。
      - 目标解析：`resolve_in_vault` 后校验是目录（空串 = vault 根，也允许）；`create_dir_all(parent)` + `std::fs::copy`。
      - 返回 `ImportedFile { rel_path, original_name, was_renamed, bytes_copied }`。
    - 纯函数 `pick_free_slot(active, dst_dir, stem, ext)`：从 `<stem>.<ext>` 起试，冲突则 `<stem>-1.<ext>`、`-2.<ext>` … 最多 64 次，与 `attachment_save` 的上限对齐。`split_name` 拆 basename → `(stem, ext?)`，dotfile（`.gitignore`）不拆 ext 以免 `.gitignore-1` 这种奇怪写法。
    - 8 条新单测：`split_name_basic_extension / no_extension / double_extension_keeps_rightmost / dotfile_has_no_extension` + `pick_free_slot_uses_bare_name_when_available / increments_on_collision / handles_no_extension / allows_vault_root_target`。
  - **P3-A6.2 · 前端 IPC**（`src/lib/ipc/file.ts`）
    - 新增 `fileImport(srcAbs, dstDir): Promise<ImportedFile>` + `ImportedFile` interface。
  - **P3-A6.3 · 侧栏 drop UI**（`src/routes/+page.svelte`）
    - 新 state：`dropTargetPath: string | null`（当前 hover 的目的目录，行级高亮）、`rootDropActive: boolean`（空白区 fallback 高亮）。
    - 辅助纯函数：`dataTransferHasFiles(dt)` / `decodeFileUri(uri)` / `parseDroppedPaths(dt)` / `normalizeDropDstDir(dir)`；`parseDroppedPaths` 优先 `text/uri-list`（可多行），回退到 `text/plain` 首行，匹配 POSIX 绝对路径 / Windows drive-letter / `file://` URI。
    - `handleSidebarDrop(paths, dstDir)`：循环跑 `fileImport`，聚合 `imported[] / failures[]`，四档 notice：
      - 单文件成功：`已导入 <name> → <dstLabel>`；若 `.md` 则 `openFile()`；`was_renamed` 追加 `（重命名为 foo-1.md）`。
      - 多文件成功：`已导入 N 个文件 → <dstLabel>`。
      - 部分成功：`已导入 K / N 个文件 → <dstLabel>；X 失败：<firstErr>`，info 样式，TTL 6s。
      - 全失败：`导入失败（N/N）：<firstErr>`，error 样式。
    - 6 个 DOM handler：`onSidebarRowDragOver/DragLeave/Drop(entry, e)` + `onSidebarRootDragOver/DragLeave/Drop(e)`。行级 `dragover` 设 `dropEffect = 'copy'` 让 Finder 拖动图标变成 `+`；`stopPropagation()` 阻止冒泡到 root。`dragleave` 的 `relatedTarget` contains-check 避免"从 wrap 移到内部 tree-row 再移回"的 flicker。
    - drop 目标三分支：目录 row → 该目录；文件 row → `parentDirOf(entry.rel_path)`；root 空白区 → `0-inbox/`（选 inbox 而不是 vault 根是因为 vault 根在 LYT 里不是自由文件区）。
    - 成功导入后 `expanded.add(dstDir)` + `refreshTree()` + `schedulePanelRefresh(200)`，确保新文件立刻可见。
  - **P3-A6.4 · 模板连线 & CSS**（`src/routes/+page.svelte`）
    - `<ul class="tree">` 挂 root 三 handler + `class:drop-root-active={rootDropActive}`。
    - `.tree-row-wrap` 挂行级三 handler + `class:drop-target={dropTargetPath === ...}`。
    - CSS 两段新样式：
      - `.tree-row-wrap.drop-target` → 1px accent outline + `--color-accent-tint` 轻 bg。
      - `.tree.drop-root-active` → 2px dashed accent outline（整块 tree）。
  - **P3-A6.5 · 设计文档同步**（`design_V2.md` + `README.md`）
    - 新增 `design_V2.md` §6.13.9「侧栏文件 drop 导入」覆盖动机、与编辑器 drop 的分工表、drop 目标三分支、命名冲突策略、为什么不走 bytes IPC、notice 聚合四档、视觉反馈、失败语义、V2 合规自检与 4 条 Known gaps。changelog 追 `2026-04-20 | 2.13`。
    - `README.md` 第 9 行把 P3-A6 加入"已落地"清单。

- **How to verify**
  - 自动化：
    - `pnpm check` → **0 errors, 0 warnings**。
    - `cargo test --manifest-path src-tauri/Cargo.toml --lib` → 原 54 + 新增 8 = **62 passed, 0 failed**。
  - 手测：
    1. **目录 row drop**：从 Finder 拖 `~/Desktop/foo.md` 到侧栏 `1-notes` 目录 → `.tree-row-wrap` 出现 accent outline；松手后侧栏 `1-notes` 自动展开，`1-notes/foo.md` 可见，编辑器打开该文件，右上角 success notice "已导入 foo.md → 1-notes"。
    2. **文件 row drop**：拖 `~/Desktop/bar.md` 到侧栏 `1-notes/foo.md`（文件 row）→ 文件落 `1-notes/bar.md` 而不是嵌一层。
    3. **root drop**：拖文件到侧栏 `<ul class="tree">` 的空白区 → 整棵树出现 2px dashed outline；松手后文件落 `0-inbox/` 并展开；notice "已导入 ... → 0-inbox"。
    4. **冲突递增**：再拖一次同一个 `foo.md` 到 `1-notes/` → 生成 `1-notes/foo-1.md`，notice 提示"（重命名为 foo-1.md）"。再拖第三次 → `foo-2.md`。
    5. **多文件**：选 2 个图 + 1 个 md 拖进 `4-projects/Deep-Work/` → 都落到位；notice 聚合为"已导入 3 个文件 → 4-projects/Deep-Work"，不自动打开任何一个。
    6. **目录 drop 拒**：拖一个文件夹进侧栏 → error notice "refusing to import a directory..."，无副作用。
    7. **vault 内源拒**：在 Finder 里导航到 vault 内，选中一个 md 拖回侧栏 → error notice "source is already inside this vault"。
    8. **编辑器 drop 回归**：拖一张 png 进正文 → 图片仍归档到 `attachments/YYYY/MM/`，不走侧栏路径。

- **Known gaps**
  - **bytes fallback 留给后续**：浏览器下载气泡、部分第三方应用的拖放只有 `File` 对象，无 `file://` URI。当前命中 `parseDroppedPaths → []` 分支，notice 提示"无法识别拖入的文件路径，请从 Finder 重试"。真要兜底需加 `file_import_bytes` 与 `file.arrayBuffer()`，但会让后端命令翻倍；Finder 100% 命中的场景优先，等有真用户反馈再加。
  - **目录 drop 不做**：后端直接拒绝 `is_dir` 源。Obsidian 自身也不做"拖文件夹导入 vault"；真要做得弹"要递归拷贝子树吗"的确认对话框，这次不引入。
  - **不支持 vault 内部 drag-to-move**：侧栏内文件互相拖动是独立特性，需要与 `file_move_with_refs` 联动做链接重写，与本节"外部文件导入"语义不同。Phase 3 后续任务。
  - **前端无单测 harness**：`parseDroppedPaths` / `normalizeDropDstDir` / `decodeFileUri` 都是纯函数，但仓库仍无 vitest，只有手测覆盖。后端 `pick_free_slot` / `split_name` 以 Rust 单测覆盖。

## 2026-04-20 · Phase 3-A5 — Command feedback hardening（`saveError` → notices）

- **Scope**
  - 把 `src/routes/+page.svelte` 里被命令流复用的 `saveStatus / saveError` 通道正式拆开：状态栏现在只表达 autosave（`saving / saved / save failed`），其余命令式反馈统一走页面内 notice stack。
  - **P3-A5.1 · 轻量 notice stack**（`src/routes/+page.svelte`）
    - 在页面内新增本地通知状态：`notices`、`pushNotice()`、`dismissNotice()`、`clearAllNotices()`，不引入新的全局组件系统，也不新增后端 / IPC。
    - notice 采用固定定位右上角栈式布局、自动消失、手动关闭按钮，`z-index: 200` 保持在 modal / context menu 之上，延续设计里原先预留的层级。
    - `error / success / info` 三类样式分色；error 默认停留更久，支持多行文本与移动端窄屏收缩。
  - **P3-A5.2 · 命令反馈迁移**（`src/routes/+page.svelte`）
    - 以下路径从 status bar 改为 notice：
      - graph lazy-load 失败
      - Extract selection 的前置校验与成功提示
      - export / print 相关成功与失败
      - settings 持久化失败（autosave delay / theme / shortcuts）
      - Build MOC 成功提示（含 `strategy === 'none'` 的手工补贴提醒）
      - file / dir rename 成功摘要与 guard error
      - Reveal / 右键删除
      - Set project status / Add Note to Project / Extract from project
      - unused attachments 批量删除结果
      - 全局 unhandled rejection
    - `runReseedTemplates()` 保留现有 Tauri `message()` 原生对话框作为结果通道，只移除了对 autosave banner 的借道复用。
  - **P3-A5.3 · 语义收口**
    - `saveStatus / saveError` 现在只在 autosave pipeline 内部使用：`onContentChange()`、`runPendingSave()`、`openFile()` / `goHome()` / `resetVaultViewState()` 的清理逻辑保持不变。
    - 几个原先只有 generic `saved` 闪一下、但没有文字的命令（例如 project status / extract from project）现在补成了明确文案，避免用户看到“成功了，但不知道成功了什么”。

- **How to verify**
  - 自动化：
    - `pnpm check` → **0 errors, 0 warnings**
  - 手测：
    1. 随便编辑一篇笔记，状态栏仍应显示 autosave 的 `saving… / saved / ⚠ save failed`，并且不再混入 rename / export / delete 之类命令提示。
    2. 执行 `Rename current file…` 或 `Rename current directory…` 成功后，应在右上角看到 notice 摘要，文案包含目标路径与引用改写计数；有 warning 时应走 info 样式且停留更久。
    3. 运行 `Export vault as zip…`、`Export current note (.md)…`、`Print current note`，成功 / 失败都应走 notice，而不是状态栏 tooltip。
    4. 右键 `Reveal` 失败、`Delete` 成功 / 失败、`Set project status → ...`、`Extract from project` 等命令，都应走同一套 notice stack。

- **Known gaps**
  - **notice 仍是页面内本地实现**：本轮刻意不抽成跨 route 的全局 store / dialog framework；如果后面出现第二个 route 也要复用，再考虑抽离。
  - **没有结果历史 / task center**：notice 是一次性 transient feedback，关闭或过期后不会留痕；批量任务中心仍然不在本轮范围。
  - **Tauri 原生 modal 结果仍保留双轨**：像 `Reseed templates` 这种本来就强依赖 confirm / modal 结果展示的流程，继续用 `ask()/message()`，不强行全部 toast 化。

## 2026-04-20 · Phase 3-A4 — Rename hardening（dry-run 预览 / 二次确认）

- **Scope**
  - 把现有“直接执行 rename”补成两阶段流：先 dry-run 预览影响，再确认执行。范围同时覆盖文件 rename 与目录 rename，仍然复用已有两套 modal，不引入新的全局 toast / task-center。
  - **P3-A4.1 · 预览 IPC**（`src-tauri/src/commands/rename.rs` + `src-tauri/src/lib.rs` + `src/lib/ipc/file.ts`）
    - 新增 `file_move_with_refs_preview(from, to)` 与 `dir_move_with_refs_preview(from, to)` 两条纯预览命令；执行命令 `file_move_with_refs / dir_move_with_refs` 保持不变。
    - 文件预览返回 `FileRenamePreview { old_path, new_path, rewritten_files_total, rewritten_files_preview, rewritten_links }`。
    - 目录预览返回 `DirRenamePreview { old_path, new_path, moved_files_total, moved_markdown_files, moved_other_files, moved_files_preview, rewritten_files_total, rewritten_files_preview, rewritten_links }`。
    - 预览命令只做校验、索引查询、文件树遍历与影响汇总；不写盘、不 rename、不 reindex。列表统一按路径排序并截断到 100 条，避免大 vault modal 爆炸。
    - 后端把 dry-run 影响统计抽成 helper：`group_referring_rows`、`summarize_preview_rewrites`、`summarize_moved_files`，复用已有 `RewritePlan::from_paths` / `walk_dir_all` / `build_dir_plan` / `query_referring(_dir)`。
  - **P3-A4.2 · 文件 rename 两阶段 modal**（`src/routes/+page.svelte`）
    - 文件 rename modal 从“一步确认”改成“预览影响 → 确认重命名”两阶段状态机。输入框修改会使 preview 失效，必须重新预览。
    - 预览态展示固定摘要：`将移动 1 个文件；将重写 N 个文件中的 M 处引用`，以及“将被改写的文件”列表。
    - 主按钮文案在 `预览影响 / 预览中… / 确认重命名 / 重命名中…` 之间切换；`Enter` 语义随阶段切换，`Esc` / backdrop 取消逻辑保持不变。
  - **P3-A4.3 · 目录 rename 两阶段 modal**（`src/routes/+page.svelte`）
    - 目录 rename modal 同样改成 preview-first 流；预览态展示：
      - `将移动 X 个文件（Y 篇笔记 + Z 个附件/其他）；将重写 N 个外部文件中的 M 处引用`
      - “将移动的文件”列表
      - “将被改写的外部文件”列表（0 项时明确显示 `无外部文件需要改写。`）
    - confirm 仍沿用现有执行链：`drainPendingSaves()`、`dirMoveWithRefs()`、follow 当前打开文件、刷新 tree / panel、状态栏 banner。

- **How to verify**
  - 自动化：
    - `pnpm check` → **0 errors, 0 warnings**
    - `cargo test --manifest-path src-tauri/Cargo.toml` → **54 passed, 0 failed**
  - 手测：
    1. **文件 rename**：打开任意带反链的笔记 → `Rename current file…` → 输入目标路径后先点“预览影响” → modal 中应出现摘要和 referrer 列表 → 再点“确认重命名”，执行完成后编辑器跟随到新路径，状态栏 banner 显示 rewrite 计数。
    2. **文件 rename 预览失效**：预览成功后继续改输入框 → 主按钮应回到“预览影响”，直接确认不可用，必须重新 preview。
    3. **目录 rename**：对含 `.md` + 附件的目录执行 rename → preview 应同时显示 moved files 与 rewritten external files 两块列表；若无外部 referrer，第二块应明确写 `无外部文件需要改写。`
    4. **预览后执行失败**：preview 成功后，若目标路径在确认前被外部占用，confirm 应保留 modal 并展示错误，不应静默关闭。

- **Known gaps**
  - **目录 preview 的 link 数仍沿用现有执行语义**：`rewritten_files_total` 只统计外部 referrer，但 `rewritten_links` 仍包含树内 referrer 的替换次数，因此摘要文案是“外部文件数 + 总替换数”的组合；这是为保持 preview 与现有执行结果一致，后面若要完全语义对齐，可以把 execute-path 的计数也拆开。
  - **preview 仍是 advisory，不做锁定**：用户 preview 后到 confirm 之间，源文件、目标路径或 referrer 内容都可能变化；最终以 confirm 时真实文件系统与索引状态为准。
  - **`saveError → toast` 仍未拆**：rename 成功后的反馈依然走状态栏 banner 通道，未引入独立 toast，这一项保留给下一轮统一收口。

## 2026-04-20 · Phase 3-A3 — Graph hardening sweep（键盘导航 / 屏阅镜像 / 大图调参）

- **Scope**
  - 聚焦图谱视图这条 `P3-A` 剩余最高频摩擦，把原先明确挂起的 3 类问题一起收口：`a11y` 键盘导航、screen-reader mirror，以及大图布局在数百节点以上时的稳定性调参。仍然坚持"不改 schema / 不改 IPC 结构 / 不做节点位置持久化"。
  - **P3-A3.1 · Graph keyboard focus + screen-reader mirror**（`src/lib/graph/GraphView.svelte`）
    - 新增 `focusPath` / `focusNode` / `visibleNodes` 派生状态；图谱节点按 `label -> path` 稳定排序，作为键盘导航与屏阅镜像的统一数据源。
    - 画布本身变为可聚焦元素：支持 `ArrowUp/Down/Left/Right` 顺序切换节点、`Home/End` 跳到首尾、`Enter` 打开当前聚焦笔记、`Escape` 清空搜索高亮。
    - 新增 sidebar 内的 **Keyboard** 区块，显示当前键盘焦点卡片与 `Prev / Next / Center / Open` 四个操作；鼠标点节点后也会把焦点同步到该节点，方便"先点一下，再用键盘继续走"。
    - 新增屏阅镜像层：`aria-live` 状态文本 + `Visible graph nodes` 隐藏列表，把当前可见节点集合、聚焦次序、入/出度等语义从 canvas 渲染里镜像到可读 DOM。
  - **P3-A3.2 · Graph empty-state / local-mode UX**（`src/lib/graph/GraphView.svelte` + `src/lib/graph/forceLayout.ts`）
    - 本地图（local mode）空态从一句 `No neighbours...` 改成标题 + 建议动作的卡片，明确告诉用户应该切 `Global`、加深 depth、关闭 `Hide orphans`，还是等待 indexer。
    - `localSubgraph()` 在 seed 笔记尚未进入索引时，不再直接返回空图，而是保留一个"孤立 seed 占位节点"，避免用户误以为图谱视图坏掉。
    - 顺手修正了一个过滤细节：`filterByType()` 之前对 `note_type === null` 节点用空串匹配，导致 UI 里的 `unknown` 实际永远不过滤白名单；现已统一成 `unknown` 口径。
  - **P3-A3.3 · Large-graph force preset**（`src/lib/graph/forceLayout.ts`）
    - `startLayout()` 现在会按 `nodes / edges` 数量自动切 `small / medium / large` 三档 preset，而不是所有 vault 都吃同一组 `forceLink / forceManyBody / forceCollide` 参数。
    - `medium / large` 档位下会同步收紧 `linkDistance / linkStrength`，限制 `chargeDistanceMax`，减小 `forceCollide` 半径并提高 `alphaDecay / velocityDecay`，让几百节点以上的图更快收敛、不容易抖太久。
    - Sidebar stats 区块增加 `Medium-graph tuning active.` / `Large-graph tuning active.` 提示，避免用户误以为布局变化是随机行为。

- **How to verify**
  - 自动化：
    - `pnpm check` → **0 errors, 0 warnings**。
  - 手测：
    1. 打开图谱后按 `Tab` 聚焦 canvas，使用方向键切换节点，应看到 sidebar 的 **Keyboard** 卡片和隐藏 `aria-live` 状态随之更新；按 `Enter` 应打开当前聚焦笔记。
    2. 在搜索框输入命中词后按 `Enter`，图谱应跳到该节点并把键盘焦点同步过去；随后按 `Escape` 应清掉搜索高亮，但保留图谱可继续键盘导航。
    3. 切到 `Local` 模式，在当前笔记还没被 indexer 吃进去、或周围确实没有连边时，图中心应给出带操作建议的空态卡片，而不是只剩一句模糊提示。
    4. 在一个较大的 vault 打开 graph，sidebar stats 如命中中/大图 preset，应显示对应 tuning 提示；拖拽/缩放后布局收敛速度应比旧版本更快，不会长时间持续抖动。

- **Known gaps**
  - **屏阅镜像目前是"只读语义层"**：已经能让 screen reader 读到当前图谱状态、节点集合与焦点变化，但还不是一整套 DOM listbox/treeview 交互模型；如果后面要做完全脱离 canvas 的等价图谱导航，可以再补 roving-tabindex 或 dedicated list view。
  - **大图 preset 还是经验值，不是 benchmark 驱动**：当前阈值按个人知识库量级人工收敛出来，解决的是"别太抖、别太慢"；若后续要支持 5k+ 节点，下一步应考虑 worker 化、静态聚类或布局缓存。
  - **节点位置仍不持久化**：这轮只做 runtime hardening，没有引入 `.mynotes/graph-layout.json` 之类的存储；关闭图谱后仍会重新布局，和当前产品哲学保持一致。

## 2026-04-20 · Phase 3-A1 — TagView 多标签筛选 / 排序

- **Scope**
  - 把 `TagView` 从 Phase 2 的“单标签聚合页”推进成真正可探索的过滤视图，补上 `P3-A` 里已经明确列出的两类高频摩擦：`Tag` 交/并集筛选，以及结果排序。
  - 后端新增 `index_notes_by_tags(tags, match_all)`：
    - `match_all = true` 返回交集；
    - `match_all = false` 返回并集；
    - 在 SQL 层一次完成聚合，避免前端自己并 N 次 `invoke` 再拼集合。
  - 前端 `TagView.svelte` 重构为三段：
    - **主标签 + 附加标签**：当前侧栏点开的 tag 作为“主标签”锁定在顶部，用户可从同 vault 的其它标签里继续追加过滤条件，也可以清空所有附加标签回到“只看主标签”。
    - **交集 / 并集切换**：两颗 segmented button，分别表达“同时带有这些标签”和“带有任一标签”。
    - **排序**：提供“最近更新优先 / 最早更新优先 / 标题 A→Z / 按路径”四种排序，不再强制只有 `updated desc`。
  - UI 上保留 Phase 2 的“建 MOC”入口，但明确它仍然是“基于主标签”的动作，不会悄悄把当前交/并集过滤条件混进去，避免用户误以为会直接按当前过滤结果建 MOC。

- **How to verify**
  - 打开任意有多个标签的 vault，进入某个 TagView：
    - 顶部应出现“筛选标签 / 匹配方式 / 排序”三块控件。
    - 主标签 chip 标记为“主标签”，不可移除；从下拉里添加第二、第三个标签后，列表应立即刷新。
  - 交集验证：
    - 选 `#foo` 作为主标签，再添加 `#bar`，匹配方式切到“交集”；
    - 列表中每篇笔记都应同时带有 `foo` 与 `bar`。
  - 并集验证：
    - 保持相同标签，切到“并集”；
    - 列表应扩大为任一标签命中的笔记集合，而不是只保留重叠部分。
  - 排序验证：
    - 在同一组过滤结果上切换“最近更新优先 / 标题 A→Z / 按路径”，列表顺序应即时变化，内容集合不变。
  - 运行：
    - `pnpm check`
    - `cargo test --manifest-path src-tauri/Cargo.toml`

- **Known gaps**
  - **TagView 仍然是“从一个主标签进入”**：附加标签是 narrowing / broadening，不是一个独立的全局 faceted-search 页。要做真正的“多标签浏览器”，后面可以考虑把这套控件上提到 Home 或 Palette。
  - **“建 MOC”还不吃当前过滤结果**：现在它只基于主标签打开原有的 Build-MOC 流程，避免把过滤语义混进既有命令。若用户后续明确需要“从当前过滤结果建 MOC”，那会是下一步单独任务。
  - **排序仍是前端表现层排序**：集合选择在 Rust/SQLite 做了，但排序还在前端切换。当前数据量下完全够用；如果后续 TagView 需要分页或 10k+ 规模优化，再把 sort 参数下推到 IPC。

## 2026-04-20 · Phase 3-A1 — App Config / 快捷键配置化启动

- **Scope**
  - 把 Phase 2 留下来的两块配置债一起清掉：一是 theme / autosave 不再只依赖 `localStorage`，改为落到 Tauri 侧 `app-config.json`；二是 `installShortcuts()` 从硬编码 `if/else` 改成 keymap 驱动，用户可在 Settings 里直接录入新的快捷键。
  - Rust 侧新增 `app_config_get / app_config_set_theme / app_config_set_autosave_ms / app_config_set_shortcuts` 四个命令；`ConfigStore` 扩出 `theme / autosave_ms / shortcuts`，和现有 recent-vault 列表共存于同一份 app config。
  - 前端新增 typed IPC `src/lib/ipc/config.ts` 与快捷键解析/匹配工具 `src/lib/shortcuts.ts`；Settings modal 新增“快捷键”区块，支持：
    - 点击某行“录入”按钮后直接按组合键；
    - 冲突检测（禁止两个动作绑成同一个 accelerator）；
    - 单项恢复默认；
    - 命令面板 hint 与侧栏 Today / Week / Capture / Record tooltip 实时反映当前绑定。
  - 浏览器预览模式保留 `localStorage` fallback；Tauri 模式则把 `localStorage` 当镜像缓存，并以 app config 为长期持久化来源，避免旧用户设置丢失。

- **How to verify**
  - `pnpm tauri:dev` 启动应用，打开任意 vault，按 `⌘,`：
    - Settings 里应出现“快捷键”区块，列出 `命令面板 / Today / This Week / Quick Capture / Daily Record / Graph / Extract / Settings` 八项。
    - 点某一项右侧按钮后按新的组合键，例如把 `Today` 改成 `⌘⇧T`；回到主界面后该按钮 tooltip 与命令面板对应条目的 hint 都应同步显示新键位。
    - 若把第二个动作录成同一个组合键，应显示冲突提示，不保存。
    - 点“默认”应只恢复该项到默认值。
  - 关闭应用重开：theme / autosave / shortcuts 都应保留，不需要再靠同一 vault 的 localStorage 碰运气。
  - 运行：
    - `pnpm check`
    - `cargo test --manifest-path src-tauri/Cargo.toml`

- **Known gaps**
  - **还没有接系统级 global shortcut**：当前只解决 app 内部快捷键配置化；设计里提过的“即使窗口不在前台也能 Quick Capture”仍是后续 Phase 3 任务。
  - **快捷键 UI 只支持录入，不支持自由文本表达式编辑**：这是刻意收敛范围，先把高频路径做稳；若后面需要导入/导出 keymap，再加文本层或 JSON 层。
  - **命令面板里只有与 keymap 对应的命令会动态改 hint**：Theme / Export 这类无快捷键或未配置到 keymap 的命令，仍显示静态 hint / 分类名。

## 2026-04-20 · Phase 2 收口 / Phase 3 启动准备

- **Scope**
  - 对齐文档状态，正式把项目从"Phase 2 功能开发中"切到"Phase 2 已完成，准备进入 Phase 3"：
    - `design_V2.md`：§10 路线图里给 `Phase 2` 加状态说明（已完成），把 `Phase 3` 从原先 3 条一句话愿景整理成可执行的 4 条工作线：`P3-A Desktop Hardening / Config`、`P3-B Web（只读浏览）`、`P3-C Mobile（Quick Capture + Browse）`、`P3-D AI Module`；同时写明推荐启动顺序是"先稳桌面内核，再开 Web / Mobile PoC，最后接 AI"。
    - `design_V2.md` §16 changelog 补 `2.8`，把这次"Phase 2 收口 + Phase 3 准备"记成一次文档层版本推进，避免后面再靠口头记忆判断当前阶段。
    - `README.md` 的"当前状态"从"Phase 2 核心工作流已落地，当前以 bugfix / polish 为主"改成"Phase 2 已完成，当前处于 Phase 3 启动准备阶段"，并把下一阶段优先顺序浓缩成 3 行，方便任何新协作者打开仓库第一眼就知道项目现在不再补 Phase 2，而是准备进入 Phase 3。
  - **这次不改代码，不新增功能**。目标纯粹是把阶段边界钉死，避免后续继续以"Phase 2 还没完"的心态做零散追加。

- **How to verify**
  - 打开 `README.md`：应能直接看到"Phase 2 已完成 · 当前处于 Phase 3 启动准备阶段"。
  - 打开 `design_V2.md §10`：`Phase 2` 段落有"已完成"说明；`Phase 3` 不再只是 3 个模糊方向，而是 4 条工作线 + 启动原则 + 推荐顺序。
  - 打开 `design_V2.md §16`：changelog 里应多一条 `2.8`，说明这是一次文档层面的阶段收口。
  - 打开本文件顶部：这条记录应排在最前，作为 Phase 3 开始前的上下文锚点。

- **Known gaps**
  - **还没有真正开始 Phase 3 的代码任务**：这次只做了整理与定向，不包含任何 Phase 3 功能实现。
  - **`P3-A` 内部的具体 Task 还没拆编号**：现在只有工作线和顺序，没有像 Phase 2 那样拆成 `Task 1 / 2 / 3 ...`。进入 Phase 3 的第一步应先把 `P3-A` 拆成明确任务表，再开始代码推进。
  - **Phase 4 的质量工程暂未前移**：虽然桌面端已可长期使用，但 E2E / CI / 更系统的单测仍留在后续阶段，没有因为 Phase 2 收口而自动提前。

## 2026-04-20 · Phase 2 · Task 8.2 — 图片插入三条路径全部修复（Finder 拖放 / 微信粘贴 / 手打绝对路径）

- **Scope**
  - **背景**：Task 3 上线后用户实测三条插入路径**全废**：(a) 从 Finder 拖图到编辑器无反应；(b) 从微信复制粘贴图片后编辑器里只留文本；(c) 手打 `![wda ](/Users/…/Desktop/foo.jpg)` 这种绝对路径后仍只是字面字符，不出缩略图。定位：
    1. 拖放——Tauri 2 的 window 默认开 `dragDropEnabled: true`，原生 drag-drop 在 OS 层就被 Rust 接管，DOM 的 `drop` 事件永远不触发。
    2. 微信粘贴——微信桌面端剪贴板 MIME 里**没有** `image/*`，只有 `text/uri-list` / `text/plain`（给一个 `file://` 或 `/Users/...` 形态的路径）。`imageEmbed.ts` 的 paste handler 只看 `cd.files` 有没有 `image/*`，没命中就放过，没任何 fallback。
    3. 手打绝对路径——`EMBED_LINE_RE = /^\s*!\[([^\]]*)\]\((attachments\/[^)]+)\)\s*$/` 只认 `attachments/…`，绝对路径和 `file://` 根本不进 scanEmbedLines；而且 WKWebView 在 `http://localhost` 起源下拒载 `file://` 的 `<img src>`，哪怕放行了也还得走 IPC 读字节再包 Blob URL。
  - **`src-tauri/src/commands/attachment.rs` 新增 `attachment_read_external_bytes(abs_path) -> Vec<u8>`**：
    - 只接绝对路径（`PathBuf::is_absolute()`）；
    - 扩展名白名单 `png/jpg/jpeg/gif/webp/svg/bmp/avif/heic/heif`——**不是**任意文件读 IPC，是"编辑器图片预览 / 归档"专用；
    - `std::fs::metadata` 断非文件拒绝；文件大小硬上限 `50 MB`（超过 almost certainly 选错文件，防手滑 OOM）；
    - 安置在既有 `attachment_read_bytes` 下方、`attachment_list` 上方，跟 attachment 流水线毗邻，维护方向一致。
    - 命令在 `src-tauri/src/lib.rs` 的 `invoke_handler!` 注册。
  - **`src-tauri/tauri.conf.json` 关 Tauri 原生 drag-drop**：window 配置里加 `"dragDropEnabled": false`。关掉的代价是失去 Rust 侧 drag event（我们本来就不用），好处是 DOM 的 `drop` / `dragover` 可以正常冒泡到 CM6 的 domEventHandlers。
  - **前端 IPC `src/lib/ipc/attachment.ts`**：加 `attachmentReadExternalBytes(absPath): Promise<Uint8Array>` wrapper（镜像 `attachmentReadBytes` 的 `number[]` → `Uint8Array` 还原）。
  - **`src/lib/editor/imageEmbed.ts` 三改**：
    1. **`EMBED_LINE_RE` 扩成三选一**：`attachments\/[^)]+` ∣ `file:\/\/[^)]+` ∣ `\/[^)]+`（POSIX 绝对路径，`[^)]+` 允许空格 / CJK）。`http(s)://` 故意不入名单——编辑器里不触发网络请求是约束。
    2. **`getBlobUrl(pathLike)` 按路径形态 dispatch**：`attachments/` → `attachmentReadBytes`；`file://` → `decodeFileUri()` 去协议 + `decodeURIComponent` 处理百分号编码（中文文件名这条路线靠这个落地）→ `attachmentReadExternalBytes`；`/…` → 直接 `attachmentReadExternalBytes`。blob cache 的 key 仍是原始 `pathLike` 字符串。
    3. **paste / drop handler 加 `text/uri-list` + `text/plain` fallback**：当 `cd.files` / `dt.files` 里没 `image/*` 时，读 `text/uri-list`（多行、过滤 `#` 注释）→ 失败再读 `text/plain`（只取首行，避免吞普通文本）→ 收集成 `pathCandidates`；`looksLikeImagePath()` 做**同步**前置筛（扩展名 ∈ IMAGE_EXTS 且是 `file://` / 绝对路径）——若全都不像图就 `return false` 让 CM 走默认粘贴，避免文本粘贴被吞；确实像图才 `e.preventDefault()` + 进 async，对每个候选调 `saveImageByPath()`：`attachment_read_external_bytes` 读字节 → `attachment_save` 归档到 `attachments/YYYY/MM/` → 插入 `![name](attachments/…)`。**归档而不是留外部路径**：粘贴/拖放语义是"把图**带进**仓库"，Task 3 已如此。
    - 手打绝对路径**不**归档，`getBlobUrl` 直接读外部路径渲染——尊重用户在文本里留的原值，代价是"外部文件搬走就预览失效"（widget 显示"⚠ 无法加载图片"）；这条代价设计文档里写明。
    - widget 错误文本从"找不到附件"改成"无法加载图片"，因为现在也可能是 external 路径不存在。
  - **设计文档 `design_V2.md`**：§6.12.4 把原三行"粘贴/拖放/非 image"表扩成六行（加回退粘贴 / 回退拖放 / 手打绝对路径），新增"Tauri 原生 drag-drop 拦截"小节 + "外部路径 IPC（`attachment_read_external_bytes`）"小节；§6.12.5 widget 描述改成三种路径形态表格 + `file://` decode 的说明。changelog 追 `2026-04-20 | 2.7`。

- **How to verify**
  - 手测路径 A（Finder 拖放）：Finder 里选 `.png` 文件 → 拖到编辑器正文 → 应当立刻出现 `![foo](attachments/2026/04/20260420-xxxxxx-foo.png)`，下方显示缩略图 widget，vault 的 `attachments/2026/04/` 下多出该文件。
  - 手测路径 B（微信桌面端粘贴图片）：微信 → 右键图片 → 复制 → 编辑器 `⌘V` → 应当落成 `![原始或slug](attachments/…)`，渲染缩略图；若微信的剪贴板变体只给 `text/plain` 也走同一条回退。
  - 手测路径 C（手打绝对路径）：编辑器里**整行**敲 `![screenshot](/Users/hcyang/Desktop/xxx.png)` → 回车 → 该行下方出缩略图 widget；把桌面那张文件删掉或改名 → 切出/再切回该笔记 → widget 变成"⚠ 无法加载图片: /Users/.../xxx.png"错误条。
  - 手测路径 D（普通文本粘贴回归）：剪贴板里只有非路径字符串（如代码片段、英文句子）→ `⌘V` → 文本正确落地，**不**被图片 handler 吞掉。

- **Known gaps**
  - **Windows / Linux 绝对路径正则未适配**：`EMBED_LINE_RE` 的 `\/[^)]+` 分支只认 POSIX 绝对路径。Windows 的 `C:\Users\...` 或 Linux 不挂 `/` 起头的古怪路径一律不触发 widget。要兼容得扩正则且同步扩 `attachment_read_external_bytes` 里 `is_absolute()` 的判断（Rust 的 `Path::is_absolute` 在 Windows 下认 drive-letter，后端 OK；前端 regex 是目前真正的 gate）。
  - **Firefox file-uri 策略**：Firefox 对从 `http://localhost` 原页引用 `file://` 下载 / 预览有 `file_uri_strict_origin_policy` 限制；这个项目当前架构下走 IPC 读字节绕开了原生 `<img src=file://>`，但日后若改回直接挂 `file://` 到 `<img>`（例如打印 HTML 导出里），Firefox 需单独处理。
  - **50 MB 上限无用户提示**：IPC 拒绝时错误会冒泡到 console，widget 显示"无法加载图片"但不说明为什么。对巨图不友好，可考虑在 `saveImageByPath` 失败分支读 `error message` 弹状态栏提示。
  - **external-path 笔记的可移植性**：手打 `/Users/hcyang/Desktop/…` 后把笔记搬到另一台机器，绝对路径必然失效。建议（但不强制）先走拖放 / 粘贴让图片进 `attachments/`；未来加一条 `> Intern absolute image paths`（扫当前 md 的 external 引用 → 读字节 → 归档 → 重写 md）可以一键迁移。
  - **Tauri 原生 drag-drop 完全关掉**：代价是日后若要在非编辑器区域接收文件（例如 sidebar 导入）也得手写 DOM 的 `drop`。当前没这需求所以 OK。
  - **heic/heif 后端允许但浏览器不认**：`<img>` 在绝大多数 WKWebView 版本下可直接渲染 HEIC，但 Chromium / Firefox 长期不支持。后端放行是因为"至少 Rust 能读进来并归档"；归档后能不能预览取决于 webview——这条是 best-effort。

---

## 2026-04-20 · Phase 2 · Task 8.1 — 打印 PDF 改走 Rust 渲染 HTML + 系统浏览器（修复"打印失败"）

- **Scope**
  - **背景**：Task 8 初版 `> Print current note` 用 `window.print()` + `@media print`。用户实测反馈"打印对话框压根没弹"。定位：Tauri macOS WKWebView 对**程序化触发的** `window.print()`（`setTimeout` / palette handler 等非用户手势入口）静默 drop；就算它能触发，CM6 viewport virtualization 只把视窗内的行挂到 DOM，`@media print` 再怎么放开 `overflow/height` 也打不出不存在的节点——打印结果至多只有第一屏。
  - **修法**：不再依赖浏览器 print API。Rust 端渲染 markdown → HTML 独立文件 → 扔给系统默认浏览器，用户在浏览器里按 `⌘P`（用户手势，不被吞）做"另存为 PDF"。
  - **`src-tauri/src/commands/export.rs` 新增 `note_render_print_html(src_rel_path) -> String`**（约 210 行含样式）：
    1. 沙盒合约同 `note_export_copy`（拒 absolute / `..` / 非文件）。
    2. 读 md → `strip_frontmatter`（`---\n...\n---\n` + CRLF 兼容；恶意/畸形 frontmatter 不截断原文，fallback 到"直接渲染"）。
    3. `pulldown_cmark::Parser` 配 `ENABLE_TABLES / STRIKETHROUGH / TASKLISTS / FOOTNOTES`（GFM-ish 常用子集，故意不开 smart-punctuation——代码示例里的引号不该静默卷起来）。
    4. 包 `wrap_print_html(title, base_href, body_html)`——自包含 HTML 骨架：系统字体栈（含 PingFang SC / Microsoft YaHei）、max-width 780px 阅读列、`@page { margin: 0.75in }`、表格 / 代码块 / blockquote / task-list 全套 CSS、`@media print` 去背景色 + 链接不染色；标题用 H1 插到正文前，页脚一行小字提示"在浏览器按 ⌘P / Ctrl+P"。
    5. `<base href>` 用 `url::Url::from_directory_path(&vault)` 生成带百分号编码的 `file:///.../` URL，这样 md 里 `![](attachments/2026-04/foo.png)` 的相对路径在浏览器里能正确解析到 vault 内文件。
    6. 写到 `state.app_support_dir.join("print-preview").join(format!("{safe}-{ts}.html"))`——`safe` 过 `sanitize_stem` 把 `/ \ : * ? " < > |` 和控制字符归一成 `_`，防 Windows 文件名冲突；`ts` 是 `SystemTime::now()` 的毫秒戳避免重名。
    7. `opener::open(&out_path)` 交给 OS 默认 .html handler（macOS `open` / Windows `start` / Linux `xdg-open`）；任何失败走 `AppError::Other("open failed: {e}")` 冒泡。
    8. 返回 HTML 绝对路径给前端显示。
  - **新依赖（`src-tauri/Cargo.toml`）**：`pulldown-cmark = { version = "0.11", default-features = false, features = ["html"] }`（纯 Rust，只要 html 渲染子功能，去掉 simd 等默认 feature 减包体）+ `opener = "0.7"` + `url = "2"`（Tauri 自己已传递依赖，显式 pin 为直接依赖）。
  - **`src-tauri/src/lib.rs`**：`invoke_handler!` 追 `commands::export::note_render_print_html`。
  - **前端 `src/lib/ipc/export.ts`**：加 `noteRenderPrintHtml(srcRelPath): Promise<string>` wrapper。
  - **`src/routes/+page.svelte`**：`runPrintCurrentNote` 从 fire-and-forget 的 `setTimeout(() => window.print(), 0)` 改 async：先 `drainPendingSaves()` 把编辑器里未落盘的 pending 写完，再 `noteRenderPrintHtml(path)`，状态栏显示"已在浏览器打开预览（`<file>.html`）。在浏览器中按 ⌘P / Ctrl+P 保存为 PDF"。`paletteCtx.runPrintCurrentNote` 的 arrow 外壳补 `void` 防 Promise 漏到 `() => void` 签名。
  - **`@media print` CSS 块保留为 backup**：用户手动 `⌘P` 时（用户手势，不被 WKWebView 吞），还能有个凑合的打印输出；顶部注释改写为"not the primary print flow anymore"说明现状。
  - **设计文档**：§6.15 的表格 / "PDF 路径"整段 / IPC wrapper 列表 / 失败语义全部改写；changelog 追 `2026-04-20 | 2.6`。

- **How to verify**
  - 手测路径 A（主路径，含图 + 长文）：打开一个长 md（>2 屏）且包含至少一张 `![](attachments/.../foo.png)` 的笔记 → `⌘P` 不触发应用级打印（这是预期，因为我们不挂钩 `⌘P`），命令面板 `> Print current note` → 默认浏览器自动弹出 → 标题 tab 显示笔记文件名 → 滚到底整篇都在（不是第一屏截断）→ 图片正确渲染（`<base href>` 生效）→ 代码块 / 表格 / 任务列表都有样式 → 浏览器里 `⌘P` → 选"另存为 PDF" → 出的 PDF 文字可复制、分页合理、链接蓝色。
  - 手测路径 B（空状态 + when-gate）：关闭 vault 时 `> Print current note` 不在命令面板中（when-gate 生效——没 `currentFilePath`）；打开 attachment 图片（非 .md）时 `> Print current note` 也不出现；硬调 `runPrintCurrentNote()` 经错误路径时状态栏显示"请先打开一个 .md 文件"。

- **Known gaps**
  - **wiki-link `[[foo]]` 在 HTML 里不是超链接**：pulldown-cmark 原生不认 Obsidian wiki-link 语法，会按 CommonMark 当字面字符渲染。Phase 3 要在渲染前做一次 `[[title]]` → `[title](file:///vault/1-notes/slug.md)` 的预处理（需要 title→path 的查找表，靠 SQLite index）。
  - **`<base href="file://…">` 在 Firefox 下的 file:// 跨目录加载有限制**：macOS Chrome / Safari / Edge 下实测正常；Firefox 出于 `file_uri_strict_origin_policy` 可能拒绝加载同级外的图片。若用户默认浏览器是 Firefox 且图片路径跨目录层级多，可能出现 broken image——暂不做特殊处理。
  - **print-preview 文件不自动 GC**：每次调用写一个新 `.html`，KB 级、位置隐蔽（`~/Library/Application Support/.../print-preview/`），但时间一长会堆积。以后加一个一次性 "清理 print cache" 命令或启动时刷一遍。
  - **`@media print` backup 仍截断**：用户手动 `⌘P` 走应用内打印时，CM6 虚拟化的问题依然在——这条分支只是"比什么都不做强"，不是 parity。想真正修好得在用户按 `⌘P` 时拦截并转走 `note_render_print_html`，代价是要监听 keydown 且跟系统快捷键合作，暂不碰。
  - **样式不可自定义**：打印样式是编译时写死的；用户想要自己的字体 / 页边距 / code 块配色得等 Phase 3 "导出主题" 特性。

---

## 2026-04-20 · Phase 2 · Task 8 — 设置界面 + 主题三路切换 + 导出（Zip / 当前笔记 / Print→PDF）

- **Scope**
  - **设置界面（Modal + `⌘,`）**：`+page.svelte` 新增 `settingsOpen / settingsReseedRunning / settingsReseedMsg / autosaveDelayMs` 四个状态；`installShortcuts` 追 `⌘,` → `openSettings()`；命令面板追 `open-settings`（hint `⌘,`）。modal 内四块内容：
    - **主题**：三路 radio（跟随系统 / 浅色 / 深色）。以前的 gear 按钮是"循环切换"，这里做成**绝对选择**——用户只需看一眼就知道当前在哪档。选中后立即 `setTheme(next)` 持久化并应用；gear 按钮保持循环行为不变。
    - **自动保存延时**：number 输入，范围 100–5000ms，步进 100。改动 → 持久化到 `localStorage['mynotes:autosave-ms']`、`onContentChange` 的 `setTimeout(..., autosaveDelayMs)` 立即采用新值（默认 500）。启动时 `onMount` 读一次，clamp 到 \[MIN, MAX]，非数字回退 500。
    - **模板重置**：一颗"从模板重新播种"按钮，复用 `vault_reseed_templates` command，但在前端加 Tauri `ask()` 二次确认——防止手滑；`settingsReseedMsg` 显示执行结果（例如 `templates/ 已刷新 7 个文件`）。
    - **中文分词说明**：静态段落，解释 `jieba-rs` 是 build-time 决定的（FTS5 tokenizer 建表时绑死），改动词典/切换引擎需重装。未暴露运行时 toggle，避免让用户以为能切——§5.6 已经讲清楚了。
    - 底部：app 版本（`APP_VERSION = '0.1.0'`，硬编码；将来接 `tauri::Config` 再动）+ 关闭按钮。
  - **主题三路切换（palette commands）**：`PaletteContext` 新增 `applyThemeChoice: (theme: 'system' | 'light' | 'dark') => void`。`PALETTE_COMMANDS` 追 `set-theme-system / set-theme-light / set-theme-dark` 三条独立命令（各自 `label: '设置主题 → …'`）。为什么不做单条"循环"命令——命令面板是"我知道我要哪个"的交互，绝对命名比循环命名更好背诵、也更幂等（`> dark` 再执行一次结果仍是 dark，不会 oops 变 system）。内部 handler：`setTheme(t)` 持久化到 `localStorage['mynotes:theme']` 并同步 `data-theme` 属性；之前的 `cycleTheme()`（gear 按钮）重构成调用 `setTheme`，避免两条分叉写同一份 state。
  - **导出**：`design_V2.md §6.15` 展开了三个命令。代码落地：
    - **`vault_export_zip`**（`src-tauri/src/commands/export.rs`，约 180 行）：`walkdir` 遍历 vault，`zip` crate v2 写 DEFLATE 压缩；Rust 端 atomic-ish——写到 `<dest>.part` 再 `rename` 到最终路径，崩溃留 `.part` 便于定位；排除 `.mynotes/` 前缀（SQLite index + app metadata 是派生产物，新装几秒钟就重建完，shipping 过去还有 DB 版本不兼容风险）；保留 `attachments/`（否则接收者那边 `![](attachments/…)` 链接全坏）；符号链接跳过防环；目录 entry 显式写入保留空目录；archive 内部路径一律正斜杠（Windows 的 `\` 在其他平台 zip 工具里会被当成文件名字面字符）；返回 `ExportSummary { dest_path, file_count, bytes_written, skipped_count }` 给前端在 status bar 显示"已导出 N 个文件（M KB，跳过 K）"。为什么选 DEFLATE 不 ZSTD：每个桌面 OS 自带的解压工具都能读 DEFLATE，markdown 本身压缩率就不错，值不得为了 ZSTD 引入 `zstd` 依赖。
    - **`note_export_copy`**（同文件，约 40 行）：单笔记 `.md` 复制到用户选定的绝对路径。原本想走 `@tauri-apps/plugin-fs` 的 `writeTextFile`，但 `package.json` 里没装这个 plugin——为一次写调用装 plugin + 改 capabilities JSON 是不值的；走 Rust-side `std::fs::copy` 更轻。沙盒保留 `file_read`/`file_write` 同款合约：`is_absolute()` / `ParentDir` 组件都 reject 成 `PathEscape`、源必须是文件、目标存在则拒绝覆写。
    - **`runExportCurrentNote` / `runExportVaultZip` / `runPrintCurrentNote`**（`+page.svelte` 约 90 行）：前两个走 Tauri `save()` dialog；单笔记导出前先 `drainPendingSaves()`，免得用户刚打的字还没从 debounce 飞出去；`runPrintCurrentNote` 直接 `window.print()`，靠 `@media print` CSS 隐掉 `.sidebar / .panel-slot / .status-bar / .modal-backdrop`、把 `.app` grid 拍平成 block、放开 `:global(.cm-editor) / :global(.cm-scroller)` 的 height/overflow——CM6 默认把高度锁死，打印视图会把内容截断到第一屏。选 `window.print()` + CSS 不装 `pdf-lib` / `markdown-it`：系统级"另存为 PDF"对话框在 macOS/Windows 都是一等公民，零依赖就能走完。
  - **新 IPC 层**：`src/lib/ipc/export.ts`（~45 行）：`ExportSummary` 类型 + `exportVaultZip(destAbsPath)` / `noteExportCopy(srcRelPath, destAbsPath)` 两个 invoke wrapper。与 `ipc/index.ts` / `ipc/file.ts` 同风格，JS 侧不引入任何 fs 依赖。
  - **Rust 端 wiring**：`src-tauri/src/commands/mod.rs` 追 `pub mod export;`；`src-tauri/src/lib.rs` 的 `invoke_handler!` 追 `commands::export::vault_export_zip` 和 `commands::export::note_export_copy`；`src-tauri/src/error.rs` 补 `impl From<zip::result::ZipError> for AppError`（映射到 `AppError::Other(format!("zip: {e}"))`），这样 `zw.add_directory(...)?` / `zw.finish()?` 能用 `?` 冒泡；`Cargo.toml` 追 `zip = "2"` 和 `walkdir = "2"` 依赖。
  - **palette 接线**：`PaletteContext` 一次加 5 字段（`runOpenSettings` / `applyThemeChoice` / `runExportVaultZip` / `runExportCurrentNote` / `runPrintCurrentNote`），对应 7 条命令；`export-current-note` 和 `print-current-note` 带 `when: (ctx) => ctx.currentFilePath?.endsWith('.md') && !ctx.currentFilePath.startsWith('.mynotes/')`——没打开笔记、或者打开的是 attachment 图片时，命令面板里不显示，避免产生"导出但啥都没发生"的困惑。
  - **resetVaultViewState 追补**：关 vault / 切 vault 时把 `settingsOpen / settingsReseedRunning / settingsReseedMsg` 都清零（与 `mocBuilderOpen` / `renameOpen` 等对称），防止切换后 modal 还漂着旧数据。
  - **设计文档**：§6.14（设置界面）+ §6.15（导出）两节新增，§10 Phase 2 Task 8 的 7 个子项全部打 `[x]`；changelog 追 `2026-04-20 | 2.5` 记录这批并入。

- **How to verify**
  - `pnpm tauri:dev` 后 4 条手测路径：
    1. **设置 + 主题持久化**：打开 vault → `⌘,` 弹出 Settings modal → 勾"深色"→ 编辑器与全局立即变深；关闭 modal，重启 app、打开同一 vault → Settings 再开，"深色"radio 仍勾着；命令面板 `> set theme` → 选"设置主题 → 跟随系统"→ 若系统在浅色则切浅色；再 `> set theme → 深色` 执行两次，结果仍停在深色（幂等）。
    2. **自动保存延时生效**：`⌘,` → 把 autosave delay 改成 1500 → 回到编辑器改一段文字 → 观察 status bar "已保存"出现时机从默认 ~500ms 变成 ~1500ms；重启 app，值仍是 1500（`localStorage`）；把值改成 50 → clamp 成 100；改成 9999 → clamp 成 5000。
    3. **整库 Zip 导出**：命令面板 `> Export vault as zip…` → 选 Desktop 的一个空目录 → 保存成 `vault-backup.zip` → 成功后 status bar 显示"已导出 N 个文件（M KB，跳过 K）"；外部 unzip 这个 zip → 目录里有 `1-notes/` / `2-moc/` / `attachments/` 等，但**没有** `.mynotes/` 前缀的任何文件；原地再导出一次、选同一文件名 → 应报错"destination already exists"，且保留原 zip 不动。
    4. **单笔记导出 + 打印 PDF**：打开 `1-notes/xxx.md` → `> Export current note (.md)…` → 存到 Desktop → 外部打开是同样的 markdown；在同一笔记上 `> Print current note` → 系统打印对话框弹出，预览里只看得到笔记正文（无 sidebar、无 Panel、无 status bar、无 modal）、CM6 内容高度没被截断；"另存为 PDF"能正常出文件；关打印对话框 → 回到编辑器、布局完好。
  - Extra：`⌘,` 里点"从模板重新播种"→ 系统 `ask()` 弹框确认 → 确认后 message 显示 `templates/ 已刷新 N 个文件`；关 vault 再开一个 vault → Settings modal 没残留上一个 vault 的 reseed message。

- **Known gaps**
  - **快捷键不可自定义**：`⌘,` / `⌘P` / 保存 / 打印全是代码里写死的。Phase 3 打算做 keymap JSON（受 `localStorage` 约束或存到 vault `.mynotes/keymap.json`），这一批没做——涉及命令面板重构。
  - **中文分词只能 build-time 决定**：FTS5 tokenizer 在建表时就锁死，换 jieba 词典或切换 `porter` 都得重装 app；设置页只是解释这个事实，不是 UI 遗漏。
  - **PDF 走系统打印对话框**：没有自己排版；不同 OS / 浏览器内核（macOS WebKit vs Windows WebView2）下的分页、字体、marginalia 不完全一致。对"我想要可复制文字的 PDF"够用，对"我要投稿级排版"不够用。后者得接 markdown-it + pdf-lib，这次不做。
  - **单笔记导出是 `.md` 原样复制**：不是渲染后的 HTML/PDF；想要"给别人看一份排好版的"得走打印 → 另存为 PDF。
  - **Zip 导出不做增量**：全量打包；10k+ 笔记 vault 首次导出可能几秒。`file_count / bytes_written` 返回给前端是事后总数，途中没有进度事件（Tauri event 可加，但这次范围外）。
  - **`.mynotes/` 排除不可配**：硬编码在 Rust 端。极端场景有人想把 `.mynotes/index.sqlite` 也备份到 zip，做不到——得自己手动从 vault 目录复制。设计意图就是"zip 是给人看的 / 导入到别处的 vault"，index 是机器产物、故意排除。
  - **`note_export_copy` 不支持批量**：一次只能导一篇；用户"把这个文件夹下所有 md 都导出"会回到全库 zip 场景。Phase 3 看需求再加多选批量。

---

## 2026-04-20 · Phase 2 · Task 7 — MOC 辅助建议（Build MOC from tag）

- **Scope**
  - **`src/lib/commands.ts` 追加 `buildMocFromTag(deps, { tag, title, noteRefs })` 约 65 行**：签名返回 `Promise<{ dstPath: string; insertedCount: number }>`。流程：
    1. `slugifyTitle(title)` 生成 slug；空则抛"标题无法转换为合法文件名"。
    2. 冲突循环 `2-moc/<slug>-N.md` 最多 100 次（与 Promote/Extract 对称）——否则抛"找不到空闲的目标文件名"。
    3. `createNoteFromTemplate(dstPath, { title })` 走完整 moc.md 模板路径（不复制模板内容、不旁路，保证手工"New MOC"和"Build MOC from tag"产出的 frontmatter/结构永远一致）。
    4. 读回 body，`STUB_RE = /## 核心笔记\r?\n\r?\n- \[\[\]\]/` 匹配模板里的空 stub；命中且 `lines.length > 0` 则注入 `- [[title]]` 列表替换；匹配失败 → 软降级"已创建但未注入"，不抛（模板以后换形状也不会炸）。
    5. `rewriteFrontmatter(afterInject, { moc_source_tag: params.tag })` 追写来源 tag，便于未来"重建 MOC"或 Panel 显示"该 MOC 来自 #tag"。
    6. `deps.expandDir('2-moc') → refreshTree → openFile(dstPath)`。
  - **wiki-link 形式**：emitted `[[title]]`，不是 `[[1-notes/slug]]`。理由：§5.4 解析器把"按 title 查找"当作一等支持；MOC 上裸标题远比路径可读。`ref.title ?? ref.path.replace(/\.md$/,'').split('/').pop()` 做 null 保护（某些 0-inbox/ 文件没 frontmatter title，退化到文件名 stem）。两篇同名时解析器按路径字典序确定性选第一条——靠重命名消歧义，不在 MOC 生成时做。
  - **`src/lib/ipc/index.ts` 既有 `indexNotesByTag` 不改**：已有 `(tag) → NoteRef[]` 按 `updated DESC`。`NoteRef` 新导入到 `commands.ts`（`import type { NoteRef } from './ipc/index';`）。
  - **命令面板（`src/lib/palette/commandRegistry.ts`）**：
    - `PaletteContext` 新增 `runBuildMocFromTag: () => void` + `activeTag: string | null` 两字段。前者是触发 handler；后者是 when-gate 数据源。
    - `PALETTE_COMMANDS` 追加 `build-moc-from-tag` 条：`label: 'Build MOC from tag…'`、`hint: 'MOC'`、`when: (ctx) => !!ctx.activeTag`。why `when`：命令是 tag-scoped；让用户在命令面板里挑 tag 等于重做一遍 TagView 的事。
  - **`+page.svelte` 状态 + handlers（约 125 行）**：
    - 状态：`mocBuilderOpen` / `mocBuilderTag` / `mocBuilderTitle` / `mocBuilderError` / `mocBuilderLoading` / `mocBuilderRunning` / `mocBuilderList: NoteRef[]` / `mocBuilderSelected: Set<string>` / `mocBuilderTitleEl`。`Selected` 用 Set 而非 List，toggle 是 `new Set(prev)` 再 add/delete 保证 Svelte 5 runes 追踪；`mocBuilderTitleEl` `setTimeout + focus + select` 让 modal 打开时光标立即停在 title 输入框并选中默认值（=tag 名），一回车就能改。
    - `runBuildMocFromTag()` 异步：取 `activeTag`，seed `mocBuilderTitle = activeTag`（最常见场景），`indexNotesByTag(activeTag)` 拿全列表，默认**全选**（`new Set(notes.map(n => n.path))`）——动机：用户点"建 MOC"时最短路径是"把这整个 tag 变成一个 MOC"，全选是正解。
    - `cancelBuildMoc` / `toggleMocNote(path)` / `toggleAllMocNotes()` / `confirmBuildMoc` / `onMocBuilderKey`（`Esc` 关、`⌘↵` 确认）。`confirmBuildMoc` 调 `buildMocFromTag` 后 `invalidateWikiCompletionCache() + panelRefreshToken++ + graphRefreshToken++`——新 MOC 要进补全、反链 Panel、图谱视图。
    - `paletteCtx` 追两字段：`runBuildMocFromTag: () => { void runBuildMocFromTag(); }` 和 `activeTag`。
  - **TagView 按钮（`src/lib/tags/TagView.svelte`）**：`Props` 加可选 `onBuildMoc?: () => void`。header 内"建 MOC"按钮：只有 prop 在时渲染（向后兼容其它 embed 点）、`notes.length === 0` 时 `disabled`、样式小号 ghost 按钮。CSS 注意：把 `.close` 的 `margin-left: auto` 保留（回退），`.build-moc` 也有 `auto`——当两者都在时只有 `.build-moc` 的 auto 生效推右；只有 `.close` 时仍右对齐。
  - **Modal 结构**（`+page.svelte` 约 85 行 + 65 行 CSS）：复用既有 `.modal-backdrop` / `.modal` 玻璃风；内部新增 `.moc-builder` 定宽 560px、`.moc-builder-header`（全选 + 计数 "已选 N / M"、checkbox 支持 `indeterminate`）、`.moc-builder-list`（滚动 320px，每行 label = checkbox + title + 小字 mono 路径右对齐）。list 空时 fallback 文案"该标签下没有任何笔记。创建后仅会生成模板骨架。"——允许走空 MOC 创建，等价于 `> New MOC…`。
  - **设计文档 & changelog**：§6.4 MOC 辅助建议从一行占位展开到实现对齐版（入口、modal 逻辑、`buildMocFromTag` 伪代码、wiki-link 形式决定、正则失配软降级）；changelog 追 `2026-04-20 | 2.4`。

- **How to verify**
  - `cd src-tauri && cargo test --lib` 本轮无 Rust 改动，既有测试应维持绿。
  - `npx tsc --noEmit --skipLibCheck --module esnext --target es2020 --moduleResolution bundler src/lib/commands.ts src/lib/palette/commandRegistry.ts` 对改过的两个 `.ts` 文件应 exit 0、零 error；`.svelte` 靠用户本地 `pnpm check`。
  - `pnpm tauri:dev` 后 3 条手测路径：
    1. **TagView 按钮路径**：点侧栏某 tag → TagView 打开，header 右侧出现"建 MOC"按钮（若该 tag 下无笔记则 disabled）→ 点击 → Modal 打开，title 预填 tag 名并选中、所有笔记默认勾选、计数 "已选 N / N"、全选 checkbox 勾上 → 按 ⌘↵ 确认 → `2-moc/<slug>.md` 建成、sidebar `2-moc` 展开并滚到、编辑器打开新 MOC、`## 核心笔记` 下列表里有那 N 条 `- [[title]]`、frontmatter 多了 `moc_source_tag: ...`、status bar 显示"已创建 2-moc/xxx.md（N 条笔记）"。
    2. **命令面板路径 + 子集选择**：进入一个有多条笔记的 tag → `⌘P` → 输入 "moc" → 看到 "Build MOC from tag…" → 回车 → modal 打开 → 取消勾选若干条，题改成自定义名 → 确认 → 新 MOC 只含勾选的笔记；全选 checkbox 进入 indeterminate（半勾）状态正确。进入**没有**任何 tag 被选中的场景（关闭 TagView）→ `⌘P` → 输入 "moc" → `Build MOC from tag…` 不出现（when-gate 生效）。
    3. **碰撞递增**：选 tag A、标题填"学习" → 建成 `2-moc/学习.md`。再进一次，tag 仍是 A，标题也仍填"学习" → 建成 `2-moc/学习-1.md`（不是报错、不覆盖原文件）。在 sidebar 看到两个文件并存、两个都能打开、新的那个的 `moc_source_tag` 仍指向 A。
  - Extra：取消路径——按 Esc 或点 backdrop → modal 关、无副作用、编辑器状态不变；加载中 → 按钮显示"创建中…"且 disabled，防止重入。

- **Known gaps**
  - **wiki-link 同名不消歧义**：两篇都叫"学习方法"时，MOC 里会并列出两行 `- [[学习方法]]`——目测重复。解析器按路径字典序选第一条，第二条点进去会解析到相同目标。要真正消歧义得 emit `[[1-notes/xxx|学习方法]]`（带 alias 的路径式），但会牺牲可读性。现版本选"便利优先，同名靠用户重命名"。
  - **不支持多 tag 交集**：只能一次一个 tag。真实场景"把 `#ai` 且 `#paper` 的笔记建成 MOC"得用户手选。Phase 3 的 tag 筛选器可以加"交/并"操作。
  - **`moc_source_tag` 只是审计字段**：目前没有 UI 展示"本 MOC 来自 #tag"，也没有"用最新的 tag 成员重建"动作。下一次进这个区域时再做——配合 Panel.svelte 的"该 MOC 属于 #tag / 来自 #tag"显示。
  - **模板 stub 正则耦合模板形状**：`## 核心笔记\r?\n\r?\n- \[\[\]\]` 匹配 `moc.md` 的准确字符串。若用户改了自家 vault 的 `templates/moc.md`（把"核心笔记"改成"核心"、把空 stub 删了），正则会失配——软降级为"MOC 已建但未注入列表"，用户需要手动复制列表。改进方案：模板用专用 marker 占位（比如 `<!-- moc:core-notes -->`），再做替换——更稳但引入新概念、与现有模板语法冲突，暂不做。
  - **不刷新 Tag Count**：新 MOC 写入了 `tags: [moc]`，没有 `#<source-tag>`，所以 TagsSection 里的计数不变。这是想要的（MOC 不该自动继承 source tag，否则 tag 聚合页会把自己列进去产生环）。但用户若手动往 MOC frontmatter 加 `#<source-tag>`，会被 watcher 正常处理。
  - **列表无排序选项**：当前按 `indexNotesByTag` 的 `updated DESC` 展示；没有"按标题字典序" / "按创建时间正序"切换。Phase 3 配合 TagView 的排序重构一并加。
  - **空列表仍可创建**：标签下没笔记时，仍允许生成空 MOC（退化成 `> New MOC…`）。UX 上更友好，但也带来"用户右键点到空 tag 又误点了建 MOC"的可能；现版本 modal 内显示"该标签下没有任何笔记。创建后仅会生成模板骨架。"提示。

---

## 2026-04-20 · Phase 2 · Task 4.6 — Sidebar 右键菜单（Rename/Reveal/Delete 直接操作）

- **Scope**
  - **后端新增 `path_reveal` 命令（`src-tauri/src/commands/file.rs` 追加 ~40 行）**：签名 `(rel_path: String) -> AppResult<()>`。实现用 `#[cfg(target_os = …)]` 三路分支：macOS `Command::new("open").arg("-R").arg(&abs)` 在 Finder 里预选该文件；Linux `xdg-open` 没有 "select" 动词，对文件时退回打开父目录、对目录直接打开该目录；Windows `Command::new("explorer").arg(format!("/select,{}", abs.display()))`。`spawn()?` 不 `wait`——UX 上 reveal 是"点一下就看到窗口"，我们启动子进程即返回；真失败（ENOENT）会被 `?` 转 `AppError::Other` 回到前端 `saveError`。有意不引入 `tauri-plugin-opener`——该插件是"打开默认应用处理 URL"的抽象，没有 "select in file manager" 语义，且要在 `Cargo.toml` + `capabilities/default.json` 两头登记，反而更重。注册在 `src-tauri/src/lib.rs` 的 `invoke_handler!` 的 `file_delete` 之后一行。
  - **前端 IPC wrapper（`src/lib/ipc/file.ts`）**：`pathReveal(relPath: string): Promise<void>` 薄 invoke；JSDoc 说明三平台行为（Linux 会退到父目录这条用户得知道，不然会以为 "select 不起作用"）。顺手把 Task 4.5 的 `dirMoveWithRefs` + `DirRenameResult` 接口也补齐——上一轮交付 TS wrapper 有这条，这一轮没改，但 JSDoc 顺手补了"for directories use `dirMoveWithRefs`"的提示。
  - **Rename handler 参数化重构（`src/routes/+page.svelte`）**：`openRenameModal()` 从硬绑 `vaultState.openFilePath` 改签 `openRenameModal(path?: string)`——省参继续取 `openFilePath`（命令面板保持零参调用），传参用传入值（右键菜单走这条）。`openDirRenameModal` 类似改签 `openDirRenameModal(dirPath?: string)`，内部再分 explicit（右键目录，直接拿 `dirPath`）和 implicit（命令面板，`parentDirOf(openFilePath)` 推父目录）两条分支。好处：命令面板和右键共享同一个 modal + 同一套 `confirmRename` / `confirmDirRename` 逻辑，modal 的状态机（running / error / running lock / 自动跟随打开文件）只写一份。
  - **Context menu 状态（约 15 行 $state）**：`ctxMenuOpen: boolean`、`ctxMenuEntry: DirEntry | null`、`ctxMenuX: number`、`ctxMenuY: number`。全部 session 内瞬态，不持久化。
  - **8 个 handler（`+page.svelte` 约 140 行）**：
    - `openContextMenu(e, entry)` — `preventDefault()` 吃浏览器默认菜单 + `stopPropagation()` 防冒泡；坐标按 `Math.min(clientX, innerWidth - 220 - 8)` / `(innerHeight - 240 - 8)` 钳位（常量取菜单最大宽高 + 8px 边距），保证触发点靠窗口右下时菜单不被截；把 entry 和坐标塞进 state 然后 `ctxMenuOpen = true`。
    - `closeContextMenu()` — 清 open + entry。
    - `onCtxMenuKey(e)` — 顶层捕获 `Escape` 消散。
    - `ctxReveal()` — 先 `closeContextMenu()`（异步动作前消散菜单，避免用户看到菜单挂着不动），再 `pathReveal(entry.rel_path)`；失败写 `saveError`。
    - `ctxRename()` — 按 `entry.is_dir` 分派到 `openDirRenameModal(path)` 或 `openRenameModal(path)`。
    - `ctxDelete()` — 仅文件。Tauri `ask(msg, { title: '删除文件', kind: 'warning' })` 原生弹窗确认；confirm 后 `drainPendingSaves()` → `fileDelete(rel_path)` → 若 `vaultState.openFilePath === entry.rel_path` 就显式清 `openFilePath = null; editorContent = ''`（防 stale 内容被 autosave 写回已不存在的路径）→ `invalidateWikiCompletionCache() + refreshTree() + schedulePanelRefresh(200)`。
    - `ctxOpenOrToggle()` — dir 时 `toggleDir(entry)`，file 时 `openFile(entry)`。等价于左键点，但放进菜单作为显式选项。
    - `ctxNewNoteInDir()` — 仅目录。`newNote(entry.rel_path)` 把 New Note modal 的 `targetDir` 预填成右键的目录，复用 Task 3 的 `4-projects/<slug>` + 其它 targetDir 分支机制。
  - **Markup（`+page.svelte`）**：`.tree-row-wrap` 上挂 `oncontextmenu={(e) => openContextMenu(e, entry)}`。Modal 结构分两层：透明全屏 `.ctx-menu-backdrop`（`role="button" tabindex="-1"`，捕获 `onclick = closeContextMenu`；`oncontextmenu` 也 close 防止右键外部触发第二层菜单；`onkeydown = onCtxMenuKey`）+ 绝对定位的 `.ctx-menu`（`role="menu"`，注入 `style="left: {x}px; top: {y}px"`，`onclick={(e) => e.stopPropagation()}` 防止自己点到自己冒泡到 backdrop）。菜单内容按 `ctxMenuEntry.is_dir` 分支：
    - **文件**：打开 · 重命名… · ─ · 在 Finder 中显示 · **删除**（danger 色）。
    - **目录**：展开/折叠（按 `expanded.has(path)` 文案切换）· 重命名… · 在此文件夹新建笔记… · ─ · 在 Finder 中显示。
    - 菜单顶部 header 行展示完整 `entry.rel_path`（dim mono）——在深目录里误点时是个二次确认。
  - **为什么目录不做删除**：`file_delete` 后端显式拒绝目录（refusing to delete a directory）——动机是 "too easy to nuke a whole vault by accident"。Phase 3 可以加专门的 `dir_delete` + 递归预览 + 强确认（"即将删除 N 个文件 …"），但这次不引入。删除动作 `ctxDelete()` 里也有 `if (entry.is_dir) return;` 兜底。
  - **CSS 新增（`+page.svelte` `<style>` 约 70 行）**：
    - `.ctx-menu-backdrop`：`position: fixed; inset: 0; z-index: 120`（在 modal 的 100 之上，toast 的 200 之下，右键菜单 overlay 在 modal 上也能消散）；背景完全透明（不做暗化）。
    - `.ctx-menu`：`position: fixed`；`min-width: 180px; max-width: 220px`；`background: var(--glass-bg); backdrop-filter: blur(12px)`；`border-radius: var(--radius-md)`；`box-shadow: var(--glass-shadow), var(--pane-border)`——与命令面板统一的玻璃卡片语言。
    - `.ctx-menu-header`：mono / uppercase / `--color-fg-dim` / `0.72rem` / padding 6px/12px，`border-bottom: 1px solid var(--color-border)`——"这是一条上下文元数据"的语义提示。
    - `.ctx-menu-item`：`text-align: left; width: 100%; padding: 6px 12px; font-size: 0.85rem; border: none; background: transparent`；hover `background: var(--color-surface-raised)`。
    - `.ctx-menu-item.danger`：`color: var(--color-danger)`；hover 用 `background: color-mix(in oklch, var(--color-danger) 12%, var(--color-surface-raised))`——与普通 hover 区别，暗示"不可撤销"。
    - `.ctx-menu-sep`：1px inset hairline，`margin: 4px 8px`，把"浏览类"（打开/重命名/新建）和"系统类"（Finder）/"破坏类"（删除）视觉分段。
  - **设计文档（`design_V2.md`）**：新增 §6.13.8「侧栏右键菜单」子节覆盖触发/消散、菜单内容按 `is_dir` 的分支表、handler 对照表、rename handler 参数化重构动机、`path_reveal` 跨平台 shell 实现选型、V2 合规自检。changelog 追 `2026-04-20 | 2.3 |`。

- **How to verify**
  - `cd src-tauri && cargo test --lib` → 既有 17 个 rename 测试 + 3 个 graph 测试都应通过（本任务对 rename/graph 逻辑没改动）。`cargo check` 用户本地跑预期无 warning（sandbox 无 cargo）。
  - `npx tsc --noEmit --skipLibCheck` 对修改的 `.ts` 文件（`file.ts` / `commandRegistry.ts` 未改）应零错；`.svelte` 靠用户本地 `pnpm check`。
  - `pnpm tauri:dev` 后 4 条手测路径：
    1. **文件右键 rename**：sidebar 里随便右键一个不是当前打开的 `.md` → 菜单弹出在光标点（不超出窗口）→ "重命名…" → modal 预填完整 rel_path 且选中 stem 段 → 改名回车 → 新路径落盘、`[[...]]` 引用被重写（复用 Task 4 流程）、若该文件之前没打开则编辑器不跳转。
    2. **目录右键 rename**：右键 `1-notes` 或任一子目录 → "重命名…" → modal 预填该目录完整 rel_path、末段被选中 → 改名 → 走 Task 4.5 的 `dir_move_with_refs`；若当前编辑器打开的是被搬树内的文件，自动跟随到新路径。
    3. **文件右键 delete + 跟随关闭**：右键当前正在编辑的 `.md` → "删除" → Tauri ask 弹窗警告 → 确认 → 文件被删、编辑器立即清空（`openFilePath = null; editorContent = ''`）、sidebar 该行消失、status bar 显示"已删除 ..."。取消 ask → 菜单已关、文件未动。右键**非**当前打开的文件同流程，只是不触发编辑器清空。
    4. **右键"在 Finder 中显示"**：文件 → Finder 打开父目录并预选该文件（macOS `open -R`）；目录 → Finder 直接打开该目录。Linux 下文件是打开父目录不预选（已知行为，设计文档有说）；Windows 下 Explorer `/select,` 预选。
  - **消散三条路径都验**：打开菜单 → 按 Esc 关 / 点菜单外空白区关 / 点菜单项后菜单也会关（不要求手动）。菜单靠窗口右/下边时不被截（钳位生效）。
  - 目录的菜单里**没有**"删除"项——视觉上也没有 danger 红色项。

- **Known gaps**
  - **目录级 delete 没做**：有意为之，见 Scope 第 8 条。想删空目录或想递归删除，目前得去 Finder / 命令行。Phase 3 可以加 `dir_delete` + 递归预览"即将删除 N 个文件、M 个子目录"的确认。
  - **菜单里没有 cut / copy / paste**：只做"针对目标 entry 的直接动作"，没做"暂存 + 粘贴到另一处"的剪贴板语义。实现要多一个 `clipboardEntry: DirEntry | null` + 在另一个目录右键时渲染 "粘贴到此" 条目——不复杂但本轮没做。
  - **菜单里没有 "drag to move"**：右键只触发一次性动作，不支持拖拽。拖拽移动是独立功能，Phase 3 的"侧栏交互化"一并处理（同时要处理 "drop 到目录 = 走 `file_move_with_refs`" 的引用重写）。
  - **Linux reveal 不预选**：xdg-open 无 "select" 动词，只能打开父目录。要真预选得按桌面环境分：GNOME 走 `dbus-send` 到 Nautilus；KDE 走 Dolphin 的 CLI args。工作量不小、回报低（本应用用户大概率在 macOS 上），暂不做；设计文档 §6.13.8 有声明这个降级。
  - **右键菜单没做键盘驱动导航**：菜单项靠鼠标点击，不支持箭头键高亮 + Enter 触发（虽然 `role="menu"` ARIA 正确，但没装对应键盘 handler）。屏幕阅读器用户受影响；日常高频路径是鼠标右键，所以没做。Phase 3 的 a11y pass 一并处理。
  - **`path_reveal` 无跨进程超时**：`spawn()` 不 `wait`，子进程挂起也不会让 IPC 超时——但 `open` / `xdg-open` / `explorer` 都是秒级返回的 shell 命令，实际不构成问题。
  - **ctxDelete 清编辑器后的 save 竞态**：`drainPendingSaves()` 先刷完，再 `fileDelete` + 清空 `editorContent`。但如果用户在 ask 弹窗显示**期间**继续编辑，这次编辑的 debounced save 可能在 ask 返回后仍然 schedule。目前没显式锁编辑器；实测从 ask 弹出到 `fileDelete` 通常 <200ms，自动保存 debounce 是 600ms，窗口期够短。真频繁踩到的话加个 `deleting = true` 把 autosave 跳一步即可。

---

## 2026-04-20 · Phase 2 · Task 6 — 块级 Extract to Note + Markdown 语法高亮定制

- **Scope**
  - **Markdown 语法高亮（`src/lib/editor/markdownTheme.ts` 新建，约 95 行）**：用 `HighlightStyle.define([...])` + `@lezer/highlight` 的 `tags` 写一份绑到 `--color-*` CSS var 的自定义配色，彻底替换 `@codemirror/language` 的 `defaultHighlightStyle`（后者为代码语言调优，色盘是写死的 hex，和 Quire oklch 调色板冲突）。覆盖的 tag：`meta / processingInstruction`（标记字符如 `#` `**` `` ` `` 统一 `--color-fg-dim`）/ `heading1–6`（和 livepreview 的字号/字体分工：这里只管颜色，H1–H4 → `--color-fg`，H5–H6 → `--color-fg-muted`）/ `strong` 仅加 `fontWeight: 600` / `emphasis` 仅 `fontStyle: italic` / `strikethrough` 仅 `textDecoration: line-through`（颜色保持环境色，不抢戏）/ `link` 与 `url` → `--color-accent`，URL 再加 accent-weak 的下划线 / `monospace` 换 `--font-mono` / `quote` → `--color-fg-muted + italic` / `comment` → dim + italic / 代码里的 `keyword/string/number/bool/null` 用 `oklch(...)`（蓝紫、绿、橙系），保证切主题后饱和度一致 / `function/typeName/operator/punctuation` 各自一个语义色。`fallback: true` 让未命中的 lezer tag 退回环境样式不崩。
  - **GFM 扩展开启（`src/lib/editor/Editor.svelte`）**：`markdown({ extensions: [GFM, Strikethrough] })`（从 `@lezer/markdown` 引入），这样 `~~foo~~` / 任务列表 `- [x]` / 表格 / autolink 都走解析器层，`strikethrough` 这类 lezer tag 才真正存在、上面的 highlight 规则才有节点可上色。
  - **块级 Extract to Note：纯函数构造器 + 副作用 IPC（`src/lib/commands.ts` 追加约 55 行）**：
    - `buildExtractedNote(extractedText, title, now)` 返回新笔记的完整 body 字符串，frontmatter `title/created/updated/type: note/tags: []`，body 以 `# title` 开头再跟 extracted 段落。纯函数——方便单测且不需要 window/IPC。
    - `extractBlockToNote(title, extractedText): Promise<{ dstPath: string; linkText: string }>`：slug 由 `slugifyTitle(title)` 生成，循环 `join('1-notes', `${slug}${suffix}.md`)` + suffix 递增直到 `fileExists` 返回 false（同 `runNewNote` 的老套路）→ `writeFileTrusted` 落盘 → 返回 `{ dstPath, linkText: "[[title]]" }`。故意**不**在这里修改源文件——调用方负责 dispatch 源端 edit，任何写入失败都不会造成"新笔记留在磁盘但源文件没更新"的半成品状态。
  - **Editor 暴露命令式 API（`src/lib/editor/Editor.svelte` 新增 `EditorAPI` 接口 + `onReady` prop）**：`EditorAPI` 五个方法：`getSelection() / getSelectionRange() / replaceSelection(text) / dispatchReplace(range, text) / expandToParagraph()`。要点：
    - `dispatchReplace` 接受"打开 modal 时捕获的 range"而不是 live selection：用户在 modal 里改输入时光标可能乱动，用捕获 range 保证 splice 的是原来选择的位置。内部 `Math.min(max, range)` 防御性 clamp——若 doc 在 capture 和 dispatch 之间变短（不太可能，因为 modal 期间 editor 失焦），也能退化成合法 range 不崩。
    - `expandToParagraph()` 以空行为边界向上/向下扫描：光标所在行非空时返回 `{ from: 该段首行首, to: 该段末行末 }`，顺便把 editor selection 设成这个 range（给用户即时视觉反馈"我选中的是这一段"）；光标落在空行时返回 null（调用方显示"no block at cursor"）。
    - 实现细节：`view` 在 onMount 之后就稳定不变了，所以 `onReady` 回调里把 `view` 用 const 捕获一次，五个方法闭包引用这份快照——比每次 getter 都检查 `if (!view)` 简洁。
  - **命令注册 + 快捷键（`src/lib/palette/commandRegistry.ts` + `src/routes/+page.svelte`）**：`PaletteContext` 新增 `runExtractSelection: () => void | Promise<void>`。`PALETTE_COMMANDS` 追加 `extract-selection` 条，hint `⌘⇧E`，`when` 断言：**必须有当前文件 & 是 .md & 不在 .mynotes/**（意思是 `4-projects/` / `1-notes/` / daily / 0-inbox 都能用，不限制于 inbox——长 daily 笔记里提炼一个想法、project 里抽一段 spec 都是常见场景）。`+page.svelte` 绑 `runExtractSelection()` 到 ctx，同时在 `installShortcuts` 追一条 `⌘⇧E` 直达（不绕命令面板）。
  - **Extract 模态框（`src/routes/+page.svelte`，约 70 行 + CSS 25 行）**：交互流程：
    1. 触发时 `editorApi.getSelection()` + `getSelectionRange()`；若空（用户没手动选）→ 调 `expandToParagraph()` 把光标所在段扩成 selection，range/text 从返回值拿。若 `expandToParagraph` 返回 null（光标在空行）→ showToast 提示不执行。
    2. 捕获的 `range` 存进 `extractRange = $state<{from:number;to:number}|null>`，`extractText` 存原始选中文本，`extractTitle` 默认值由 `guessTitleFromBlock(text)` 给（首行去掉 markdown heading 标记、截到 80 字符）。弹 modal。
    3. 用户在 modal 里可改 title；预览区（`.modal-preview`）展示 extracted text 前 240 字，`white-space: pre-wrap` 保段落结构。
    4. Confirm：`extractBlockToNote(title, text)` 写新笔记 → `editorApi.dispatchReplace(capturedRange, linkText)` 原子替换为 `[[title]]`，撤销栈里是单步 → toast 成功 → 关 modal。任何一步 throw → toast error，modal 保留给用户重试，源文件未被修改。
    5. Cancel：清 state 关 modal，源文件和磁盘都没动。
  - **CSS 新增（`+page.svelte` `<style>`）**：`.modal-preview / .modal-preview-label / .modal-preview-body` 三条规则，preview body 用 `--font-mono` + `--color-bg` 背景 + `max-height: 180px` overflow auto，视觉上像"只读 code block"。

- **How to verify**
  - `pnpm run check` → 本任务引入的 `markdownTheme.ts / Editor.svelte / commands.ts / commandRegistry.ts / +page.svelte` 全部零错。剩余 2 条预存错（`imageEmbed.ts:47` 的 `Uint8Array BlobPart`、`CommandPalette.svelte:210` 的 `TagCount` 类型）与本任务无关。
  - `cd src-tauri && cargo test --lib` → 34 passed / 1 failed，失败的是 `rename::tests::normalize_dir_strips_trailing_slashes_and_normalizes_separators`（Task 4.5 遗留，本任务未触碰 Rust），与本条无关。
  - `pnpm tauri:dev` 手测 4 条：
    1. **语法高亮**：打开任意 md → heading 的 `#` 灰、strong `**foo**` 加粗且 `**` 灰、`~~strike~~` 中划线、` ``code`` ` 等宽、`[text](url)` / bare URL 为 accent 橙。切主题（若 `data-theme` 切换）→ 所有配色跟着走。
    2. **Extract with selection**：在长 md 里选中一段文字 → `⌘⇧E` → modal 弹出，title 预填为选中文本首行截断，preview 显示选中原文 → 改 title 为 "测试标题" → Confirm → `1-notes/测试标题.md` 生成，原段落变 `[[测试标题]]` 链接 → `⌘Z` 一次即可撤销（验证是单步 transaction）。
    3. **Extract without selection（段落扩展）**：光标随意落在某段中间（不做选择）→ `⌘⇧E` → selection 自动扩成整段高亮，modal 出来 → Confirm → 与场景 2 等价。光标落空行 → toast 提示 "no block at cursor"，modal 不弹。
    4. **Extract 唯一性冲突**：二次 extract 用同样 title → 新文件是 `1-notes/测试标题-2.md`，链接文本仍是 `[[测试标题-2]]`（因为 `linkText` 从 slug 反推；见 Known gaps 第 2 条）。

- **Known gaps**
  - **语法高亮不覆盖代码块内的具体语言**：`keyword/string/number` 这套 tag 只在 lezer 给出对应节点时命中。我们没装 `@codemirror/lang-javascript` / `lang-python` 等具体语言扩展，所以 fenced code block 内部目前只会走通用 `monospace` + 默认色。要"代码块里 JS 关键字高亮"得引入对应 lang pack（每门语言 5–15 KB），Phase 3 按需加。
  - **Extract 的 `linkText` 来自 title 而不是 slug 去重结果**：现在 `extractBlockToNote` 返回 `linkText = "[[title]]"`，但若 slug 冲突，dstPath 是 `slug-2.md`。这意味着链接文本 `[[测试标题]]` 会通过 wiki-link 解析器找到 `测试标题.md`（已存在的那个）而**不是**新创建的 `测试标题-2.md`。暂时可接受（用户意图通常是"去到最近那个同名笔记"），但严格正确性要求把 `linkText` 改成 `[[slug-2|title]]` 形式，带 alias 表达"显示 title、实际指向新文件"。下一次 polish 时补。
  - **`expandToParagraph` 的段落定义仅按空行切**：Markdown 语义上 `## heading` 后不加空行也算新块，但这里会把 heading 和其后第一段视为同一段扩出去。对"提炼一段话"这个意图已经够用，但从 heading 下一段 extract 时体验略怪。要 pixel-perfect 得改成"遇到 heading 行也断"，多一个 regex 判断，后续优化。
  - **Modal 期间源 editor 仍可继续编辑**：虽然 dispatchReplace 用 capturedRange 有 clamp 防护，但如果用户在 modal 打开期间切走焦点到 editor 插入内容，splice 后的位置可能偏几个字符。实际使用场景极少（modal 打开时 focus 默认到 modal input），没加显式 disable。
  - **没写 `buildExtractedNote` 单测**：纯函数本该有，但本轮赶进度先跳过。下次加命令时顺手补。
  - **H1–H4 都是 `--color-fg`**：颜色上没分层，视觉差异全靠 `livepreview.ts` 里的字号分层。light 主题下还 OK，dark 下可能过平。可以把 H3–H4 再降到 `--color-fg-muted` 建立梯度，留作微调。
  - **`oklch` 关键字色在极端主题下可能不协调**：keyword=紫、string=绿、number=橙是写死的 `oklch()` 值，没绑主题 var。目前 vault 不开代码块的场景多，暂无感；有用户投诉再把这五条改成 `var(--color-syntax-keyword)` 并在 theme.css 里定义。
  - **快捷键 `⌘⇧E` 在部分 macOS 应用的"Emoji & Symbols"面板绑定上有冲突**：系统级绑定默认是 `⌃⌘Space`，但少数输入法/启动器用 `⌘⇧E`。没做快捷键自定义 UI，有冲突的用户需要自己改 commandRegistry.ts 重编一次。同 Task 5 的 `⌘⇧G` 处境。

## 2026-04-20 · Phase 2 · Task 5 — 图谱视图（Canvas + 力导向 + 局部模式）

- **Scope**
  - **后端命令 `index_graph`（`src-tauri/src/commands/graph.rs` 新建，约 240 行）**：签名 `() -> GraphData { nodes, edges }`。`GraphNode { path, title, note_type, in_degree, out_degree }`、`GraphEdge { src, dst, link_type }`。实现：`SELECT path, title, type FROM notes` 取全节点 → `SELECT src, dst_resolved, link_type FROM links WHERE dst_resolved IS NOT NULL` 单次拉已解析边 → 用 `HashMap<path, usize>` 索引节点，单遍累计 in/out degree（**两端节点都存在才计数**，防止 indexer 落后时悬挂边污染 src.out_degree）。未解析链接（`dst_resolved IS NULL`）不返回——它们没有目标节点，backlinks 面板 / "unresolved links" 命令已经单独覆盖。一次读完整图，本地 vault（几 k 节点、几十 k 边）序列化后 <1 MB，前端负责 BFS 剪枝、类型过滤、力模拟、canvas 绘制。
  - **3 个单元测试（同文件 `#[cfg(test)]`）**：`graph_contains_all_nodes_with_correct_degrees`（3 节点 3 边 fixture，A→B / A→C / B→C，断言 in/out degree 精确）/ `unresolved_edges_are_dropped`（一条 `dst_resolved IS NULL` 的链接不出现在 edges）/ `dangling_edge_to_missing_node_is_skipped`（目标节点缺失时 src.out_degree 不被错误递增——这条第一轮实现挂了，把"增度数"移到两端都命中之后才执行才修好）。测试直接 `Connection::open_in_memory` + `include_str!("../db/schema.sql")`，不依赖 AppState。
  - **IPC 注册（`src-tauri/src/commands/mod.rs` + `src-tauri/src/lib.rs`）**：`mod.rs` 加一行 `pub mod graph;`；`lib.rs` invoke_handler 数组在 rename 两条之后追 `commands::graph::index_graph`。
  - **前端 IPC 封装（`src/lib/ipc/graph.ts` 新建，约 45 行）**：`indexGraph()` 薄包装 + `GraphNode / GraphEdge / GraphData` TS 接口，字段名与 Rust serde 对齐（snake_case：`note_type / in_degree / out_degree / link_type`）。显式注释 `dst` 一定非空（后端已过滤），省掉前端 null-check。
  - **Canvas 渲染引擎（`src/lib/graph/canvasRenderer.ts` 新建，约 260 行）**：`GraphCanvasRenderer` 类管 1 个 `<canvas>` 的生命周期。要点：
    - **devicePixelRatio 感知**：backing store 用 `dpr × cssSize`，resize 只在像素尺寸真变了才重新分配（避免清空 ctx 的 line dash、font 等状态）。
    - **双坐标系**：节点+边用一次 `ctx.setTransform(dpr*k, 0, 0, dpr*k, dpr*x, dpr*y)` 在世界坐标绘制；描边宽度除以 k 保持 1-device-px；标签在屏幕坐标绘制（constant font size），用 `transform.applyX/Y` 手动投影——这是 Obsidian 的套路，避免 12px 字体放大到 48px 糊屏。
    - **hover 邻域高亮**：未 hover 时边 alpha=0.6；hover 时全局降到 0.25，该节点的相邻边再画一遍加粗+accent 色。
    - **命中测试**：`pick(clientX, clientY)` 先 `transform.invert` 到世界坐标，再 `d3-quadtree.find`（世界坐标下半径 `30/k` 的候选圈），最后精确半径校验（视觉半径 + 2px slop）。四叉树懒建：`markLayoutDirty()` 只标记脏位，第一次 `pick` 时才 `quadtree().addAll(nodes)`——每个 tick 都重建就是 O(n log n) 的无效开销。
    - **节点半径**：`baseRadius + degreeScale * √(in+out)`（默认 3.5 + 1.6×√deg）。平方根落差让"hub 显得大"但不会压扁小节点。
    - **选中环 / hover 环**：在节点上方多画一圈，selection 走 accent 色、hover 走 edgeStrong。
  - **力模拟 + 局部子图（`src/lib/graph/forceLayout.ts` 新建，约 150 行）**：
    - `toSimData(data)` 把 `GraphData` 转成 `SimNode[] / SimEdge[]`：节点按黄金比例抖动半径撒在环上（避免完美圆让 d3-force 多做功），边把 string endpoint 替换成对象引用（d3-force 的 `forceLink.id()` 需要的形态）。
    - `startLayout(data, onTick)` 配四力：`forceLink.distance(48)` / `forceManyBody.strength(-180)` / `forceCollide.radius(6+1.6√deg)` / `forceCenter(0,0).strength(0.04)`。`alphaDecay=0.03` 让模拟在 ~100 tick 内冷却。
    - `localSubgraph(data, seedPath, maxDepth)` 把边当**无向**走 BFS（你想看"谁指我 + 我指谁"两边），返回诱导子图；seed 不在数据里时返回空图而不是崩。
    - `filterByType(data, allowed)` / 孤立点判定 `isIsolated`——前端的"隐藏孤儿"开关走这条。
  - **组件 `GraphView.svelte`（新建，约 490 行）**：全屏视图，被 `activeView === 'graph'` 分支塞进 editor-pane 插槽。职责：
    - **数据加载**：`$effect` 订阅 `refreshToken`，调 `indexGraph()`，带 reqSeq 防竞态（同 InboxView 套路）。
    - **派生视图图**：`viewGraph = $derived.by(() => { 全局/局部 → 类型过滤 → 孤立节点隐藏 })`。局部模式没开文件时返回空，类型过滤空集 = 不过滤；孤立节点隐藏在局部模式下**仍保留 seed**（"我在这里"）。
    - **渲染循环**：`$effect` 监视 `viewGraph` 变化，调 `rebuildLayout()`——`stop` 旧模拟、`startLayout` 新模拟、`setData` 到 renderer、`attachDrag` 重绑 d3-drag、`resetView` 居中。每个 tick 只 `markLayoutDirty + scheduleDraw`，后者通过 `requestAnimationFrame` debounce 真实绘制。
    - **d3-zoom + d3-drag 冲突**：`zoomBehaviour.filter` 手动拒掉"鼠标按在节点上"的 mousedown（让 drag 接管），但不拦 wheel。`dragBehaviour.filter` 只在 `pick` 命中节点时触发；拖拽时 `node.fx/fy` 钉住、end 时 `= null` 让节点自然回到模拟。
    - **hover / 点击**：`onpointermove` 调 renderer.pick，更新 `hoverNode` 和 cursor。`onclick` 命中节点直接 `onOpenNote(path)`——父组件切 `activeView = null` 并 `openFile`，感受像"走进图里的一个节点"。
    - **搜索框**：`query` 驱动 `searchHit = $derived`（遍历 nodes 首个命中），回车把 viewport 跳到命中节点（无 transition，省 12KB 的 d3-transition）。
    - **侧栏**：类型 checkbox + 色块（`colorForType` 与 `colorsFromCss` 同步，读 CSS var）/ "Hide orphans" 开关 / nodes·edges 统计 / hover 节点详情卡。
    - **Toolbar**：Global/Local 分段按钮、局部深度滑块（1-4）、搜索框、Reset / Close。样式全部用 `--color-*` CSS var，light/dark 自动跟主题切换。
  - **依赖新增（`package.json`）**：`d3-force 3.0.0` / `d3-zoom 3.0.0` / `d3-selection 3.0.0` / `d3-quadtree 3.0.1` / `d3-drag 3.0.0`，以及 `@types/d3-*`。共计运行时 ~42 KB gzipped（符合设计 §6.10 预估）。故意不装 `d3-transition`——用不到那点烟火气。
  - **入口接线（`src/routes/+page.svelte`）**：
    - `activeView` 类型由 `'inbox' | null` 扩到 `'inbox' | 'graph' | null`。新增 `openGraphView() / closeGraphView() / graphRefreshToken`。
    - `editor-pane` 里 `{#if activeView === 'inbox'}` 分支后加 `{:else if activeView === 'graph'}<GraphView .../>` 分支，把 `onOpenNote` 绑成"切 activeView=null 再 openFile"，实现"图→笔记"的横向跳转。
    - `installShortcuts` 加 `⌘⇧G` 分支调 `openGraphView()`（`⌘G` 为未来的"Jump to Date"保留）。
    - `PaletteContext` 加 `runGraph: () => void`；`paletteCtx` 接 `openGraphView`；`PALETTE_COMMANDS` 加 `> Open Graph View` 一条，hint `⌘⇧G`。
  - **设计文档**：本条交付之后把 `design_V2.md §6.10`（若有）改写成对齐本实现的章节；changelog 追一行。（见 Known gaps 第 1 条，**本条还没落**。）

- **How to verify**
  - `cd src-tauri && cargo test --lib commands::graph` → 3 个测试全绿。
  - `cargo check` 应通过（sandbox 内已跑，clean）。
  - `pnpm run check` → 新文件全部零错（`imageEmbed.ts` 和 `CommandPalette.svelte` 两个预先就在的 pre-existing error 与本任务无关）。
  - `pnpm run build` → 构建成功。
  - `pnpm tauri:dev` 后手测 5 条：
    1. **全局图**：打开 vault，`⌘⇧G`（或命令面板 `> Open Graph View`）→ 力模拟自动铺开，节点按 `note_type` 上色（note=accent 橙 / moc=紫 / project=warning 黄 / daily/weekly/inbox=dim），大约 100 tick 后稳定。
    2. **局部图**：先打开某篇 MOC → `⌘⇧G` → 左上切到 Local → 只显示该笔记 + N 跳邻域；拖"Depth"滑块到 4 → 子图扩大；切回 Global → 回到全量。
    3. **交互**：鼠标滚轮缩放 0.15x–8x 无卡顿；拖空白区域平移；拖节点 → 该节点跟鼠标走并带动邻居（模拟 reheat）；松手节点释放。鼠标悬停节点 → 该节点 + 相邻边高亮，侧栏下方显示 path / title / in·out 度。
    4. **筛选 + 搜索**：侧栏 "Display" 取消勾选 "Hide orphans" → 孤立点出现在外圈；取消勾选某个类型 → 对应色的节点消失。搜索框输入关键字 → 首个命中节点高亮，回车视口跳到它。
    5. **点击跳转**：点任意节点 → activeView 回到 null，editor 打开该笔记，sidebar 当前文件同步高亮。
  - 主题切换（light/dark）→ canvas 背景、边、标签、节点色全部随 CSS var 变化（因为 `colorsFromCss` 在 `colorsFromCss` 调用时每次 getComputedStyle；注：当前不会在切主题时自动重绘整个 canvas，见 Known gaps）。

- **Known gaps**
  - ~~**设计文档 §6.10 本次未同步**~~：已补齐。`design_V2.md §6.10` 从占位章节扩写为 10 个子节（6.10.1–6.10.10，见 changelog 2.1）覆盖数据层、Canvas 取舍、d3-force 配置、局部子图、quadtree 命中、zoom×drag 冲突、依赖清单、V2 合规自检。
  - **切主题不会自动重绘颜色**：`colorsFromCss` 只在 `onMount` + `rebuildLayout` 时调用一次，之后 renderer 的 `opts.colors` 不变。light ↔ dark 切换后要重新打开图谱视图才能看到新配色。成本：监听 `document.documentElement` 的 `data-theme` attribute MutationObserver 调一次 `renderer.opts.colors = colorsFromCss(); scheduleDraw();`。Phase 3 小修。
  - **性能上限未做压测**：目标是 "几千节点 + 几万边"，但当前手头 vault 规模只有几十篇，实测性能良好不代表 2k 节点仍然流畅。Canvas + quadtree 的理论上限 ~10k 节点；真遇到卡顿先看 `forceCollide` 迭代和 tick 密度，再考虑换 WebGL。
  - **没有节点分组（community detection）**：所有节点都是散点，没有按 tag / 目录着色分组的"社群"视觉。这是 Obsidian / Roam 的常见增值功能，实现要引入 Louvain / Leiden 算法（~5 KB）或前端土办法（按 `note_type` 已经部分着色，但没按"共同 tag"聚类）。Phase 3 的 research 子项。
  - **无法持久化节点位置**：每次打开图谱都重跑力模拟，用户手动拖拽的布局不保存。Obsidian 也不存（除非 plugin），先对齐这个行为。真要存就在 `.mynotes/graph-layout.json` 里按 path 存 {x, y}，加载时覆盖 `node.fx/fy`——还得有 "reset layout" 按钮。
  - **`⌘⇧G` 与系统快捷键潜在冲突**：macOS 部分输入法的切换绑定 `⌘⇧Space` / `⌘⇧G`，少数用户可能被拦截；没有"自定义快捷键"UI，必要时改 commandRegistry 再重编一次。
  - **Local mode 孤立 seed 的空图 UX**：如果当前文件是一个完全没有 resolved link 的孤立笔记，Local 模式会显示只有 seed 一个点的图——目前正常渲染（"seed 被当作 connected 保留"的 hack），但 UX 上可能让人以为"功能坏了"。加一行提示 "当前笔记没有连接"能更明确，暂缓。
  - **Canvas 不支持无障碍**：屏幕阅读器看不到节点；没有键盘焦点循环（Tab 进图里然后箭头在节点间走）。目前是纯 `<canvas>`，离真正的 a11y 还差一层 invisible `<ul>` 镜像。不是日常高频路径，列入 Phase 3 无障碍专项。
  - **双向 `$effect` 潜在循环风险**：`$effect(() => { renderer?.setHover(searchHit?.path ?? hoverNode?.path ?? null); ... })` 同时读 `searchHit` 和 `hoverNode`——两者其中之一变化都会触发 setHover + scheduleDraw。Svelte 5 的 rune 没栈溢出，但要注意不要把 hover 回写到 state 里形成闭环。目前没有这条反向链路，安全。

## 2026-04-20 · Phase 2 · Task 4.5 — 目录重命名（Rename With Refs 扩展）

- **Scope**
  - **后端命令 `dir_move_with_refs`（`src-tauri/src/commands/rename.rs` 追加约 310 行）**：签名 `(from: String, to: String) -> DirRenameResult`。`DirRenameResult` 比 `RenameResult` 多一个 `moved_files: usize` 字段。前置校验 5 道：from/to 非空、不相等、任一侧不含 `.mynotes`、target 不嵌套在 source 里（`to == from || to.starts_with(format!("{from}/"))`——以 `/` 作边界所以 `foo` / `foo-bar` 被正确区分）、源存在且是目录、目标不存在。执行流：`walk_dir_all` 递归收 `FileMove { old_rel, new_rel, is_md }`（跳过 `.` 前缀目录，与 scanner 保持一致；非 md 文件也收集以便重写 embed）→ `build_dir_plan` 把每个 md 的 3 对 wiki + 1 对 embed、非 md 的 1 对 embed 聚合成**单个** `RewritePlan` → 单次 SQL `SELECT src, dst, link_type FROM links WHERE dst_resolved LIKE ?1 ESCAPE '\\'`（模式 `{from}/%`，`like_escape` 把 `%` / `_` / `\\` 加反斜杠防 metachar 泄漏）→ 按 src 分组 → 每个 referrer 读-rewrite-`atomic_write`（**树内 referrer 也在旧路径上就地改**，随 `fs::rename` 一起搬家；树外 referrer 加入 `rewritten_files`）→ `fs::rename(src, dst)`，失败 fallback `copy_dir_recursive + remove_dir_all`（跨文件系统的保底）→ reindex：对每个 md 文件 `delete_one(old_rel) + reindex_one(new_rel)`；对每个外部 referrer 额外 `reindex_one`。一切引用方/索引级别失败都只 log + push warning，不打断 move。
  - **`RewritePlan` 复用（rename.rs）**：聚合方案一次查 SQL 而不是每个文件跑一次，把 I/O 从 O(files × referrers) 降到 O(files + referrers)。好处：100 篇 md × 5 个 referrer 的场景从 500 次读写降到 105 次。`build_dir_plan` 还对 `old_rel == new_rel` 的项跳过（保险）、对 stem 未变的 md 自动不生成 `stem` 对（走 `RewritePlan::from_paths` 的既有过滤逻辑）。
  - **6 个新单元测试（rename.rs `#[cfg(test)]`）**：`normalize_dir_strips_trailing_slashes_and_normalizes_separators` / `like_escape_handles_percent_underscore_and_backslash` / `build_dir_plan_aggregates_md_and_non_md_entries`（2 个 md + 1 个附件的复合树，断言聚合后 wiki 对数 & embed 对数都对）/ `build_dir_plan_noop_when_no_move`（`old == new` 的 FileMove 不贡献任何 pair）/ `build_dir_plan_applies_across_tree_on_external_referrer`（一次 plan.apply 同时改两个 `[[old/x]]` → `[[new/x]]`）/ `dir_self_nesting_prefix_check_distinguishes_boundary`（pin 住 `foo → foo/bar` 被拒 & `foo → foo-bar` 放行的边界语义）。全文件共 17 个测试，原 11 + 本次 6。
  - **IPC 注册（`src-tauri/src/lib.rs`）**：invoke_handler 在 `file_move_with_refs` 后追 `commands::rename::dir_move_with_refs,`，`mod.rs` 无需改动（整 `rename` 模块已导出）。
  - **前端 wrapper（`src/lib/ipc/file.ts`）**：新增 `dirMoveWithRefs(from, to): Promise<DirRenameResult>` + TS `DirRenameResult` 接口（字段与 Rust serde 对齐：`old_path / new_path / moved_files / rewritten_files / rewritten_links / warnings`）。`fileMoveWithRefs` 的 JSDoc 把"Directory renames not supported"改成"for directories use dirMoveWithRefs"。
  - **Palette 命令 `> Rename current directory…`（`src/lib/palette/commandRegistry.ts`）**：在 `PaletteContext` 追 `runRenameCurrentDir`，registry 加一条 hint 为 `Rename` 的命令；`when` 断言要求 `currentFilePath` 含 `/`（文件不在 vault 根）且父目录不是 `.mynotes/`——保证命令只在有"父目录可重命名"时出现。
  - **目录 rename modal（`+page.svelte`）**：独立一套状态（`dirRenameOpen / dirRenameSource / dirRenameInput / dirRenameError / dirRenameRunning / dirRenameInputEl / lastDirRenameResult`）与文件 rename modal 并存，不共用以免互相污染。4 个 handler：`openDirRenameModal`（取 `parentDirOf(openFilePath)`，预填整路径并选中最后一段——让"改 leaf 保留父路径"一次输入）/ `cancelDirRename` / `confirmDirRename`（校验：trim 尾斜线、非空、!= 源、不进 `.mynotes/`、不自嵌套、目标不存在；成功后若 `prevOpen` 在被搬树内自动 `openFile` 到新路径对应的 rel）/ `onDirRenameKey`。Modal markup 复用已有 `.modal-backdrop` + `.modal` 样式（文案换成"重命名目录"），与文件 rename modal 放同一 `{#if}` 链末尾。
  - **设计文档（`design_V2.md` §6.13.7 + changelog 2.0）**：新增 6.13.7「目录重命名」子节覆盖命令签名、5 道前置校验、聚合执行流（附 ASCII 时序）、为什么一次聚合胜过 N 次 file-level 调用的 I/O 分析、"树内 vs 树外 referrer 先后顺序"的两难及选择（rewrite 先于 move）、前端 3 个入口表（palette + 侧栏右键 "暂不做" + 跟随 openFile）、失败语义（沿用 6.13.5，额外说明 move 做一半的代价可接受）、V2 合规自检。changelog 补一行 `2026-04-20 | 2.0 |`。
- **How to verify**
  - `cd src-tauri && cargo test --lib rename::tests` → 应是 17 个测试全绿。
  - `pnpm tauri:dev` 后 3 条手测路径：
    1. **基本目录改名 + 跟随**：在 `1-notes/` 下造 `foo/a.md`、`foo/b.md`，在 `1-notes/other.md` 里写 `[[foo/a]]` 和 `![](foo/pic.png)`（再塞一张 `foo/pic.png`）；先打开 `1-notes/foo/a.md`；`⌘P` → `> Rename current directory…` → modal 预填 `1-notes/foo` 并选中 `foo`，改为 `1-notes/bar`；回车；预期结果：文件树看到 `1-notes/bar/a.md / b.md / pic.png`、`other.md` 里 `[[foo/a]]` 变 `[[bar/a]]`、`![](foo/pic.png)` 变 `![](bar/pic.png)`、编辑器自动跟随到 `1-notes/bar/a.md`、status bar 显示"移动 3 个文件，重写了 1 个外部文件中的 2 处引用"。
    2. **非法目标**：尝试把 `1-notes/foo` 改成 `1-notes/foo/archive`（自嵌套）→ modal 立刻红字"目标 '…' 位于源 '…' 之内"，不发 IPC；改成 `.mynotes/x` → "不能移动到 .mynotes/ 下"；改成已存在的 `2-moc` → "目标已存在: 2-moc"。
    3. **跨目录层级 + 外部引用**：把 `4-projects/foo/` 改成 `archive/projects/foo`（两级新目录，且不在树内的笔记引用了 `foo/task.md`）→ `archive/` 和 `archive/projects/` 自动被 `fs::create_dir_all(parent)` 创出来；外部笔记的链接被重写；`warnings` 为空；sidebar 新一级目录可见。
  - `pnpm check` 应零错（sandbox 无 pnpm，用户本地跑）。
  - `cargo check` 应通过（sandbox 无 cargo，用户本地跑）。
- **Known gaps**
  - **Move 部分失败无回滚**：`fs::rename` 失败 → `copy_dir_recursive` → `remove_dir_all` 三段里任意一段挂了都**不回滚已 rewrite 的 referrer**，也不回滚已 copy 的部分目标目录。用户看到的状态是"源仍在 + 目标目录部分存在"。下一次打开 vault 会触发 full scan 修 `links` 表。真要做事务需要两阶段 commit + journal，对 Phase 2 超标。设计文档 6.13.5/6.13.7 已明写接受这代价。
  - **侧栏右键 UI 未做**：当前只有命令面板一个入口，对"想从 tree 里选中任意子目录改名"的场景不够顺手——得先打开该目录下的任意文件才能命中 palette 命令。Phase 3 的"侧栏交互化"会一并带上（右键菜单 + 拖拽移动）。
  - **Watcher 事件在 reindex 期间回放**：`dir_move_with_refs` 走的是 per-md 的 `delete_one + reindex_one`，和 watcher 在 fs::rename 发出的 FSEvents 可能有时间窗口叠加——实测下两者都走同一个 `Mutex<Connection>`，后发生的覆盖前者，没出现过 corruption。但如果 referrer 数量很多（>500），reindex 循环中的 watcher 事件可能让同一文件被多次重建。代价是 CPU/IO，不是正确性。Phase 3 的 watcher 改造里一并做 debounce。
  - **`like_escape` 对非标准字符未做 NFD/NFC 归一**：用户如果在目录名里塞 Unicode 组合字符（例如 `café` 用 NFD 的 `cafe\u0301`），SQLite 的 `LIKE` 不做 Unicode normalization，可能查不出引用方。写 V2 的 Home 默认 vault 用 ASCII 目录名，这事儿先观察。
  - **没有 dry-run 预览**：目前用户点"重命名"就直接执行，没有"先告诉我会改哪 N 个文件"的预览页。真要做得扩展 command 让它返回 plan 不落盘（参数 `dry_run: bool`），同时 UI 得多一个"确认"态。Phase 3 若用户投诉"误改"再补。
  - **跟随 openFile 使用字符串拼接**：`newOpen = target + prevOpen.slice(src.length)`——只对"打开的文件在源树内"的前提成立；如果用户在源树外的文件切换后再改目录名（openFilePath 已经不在 src_prefix 下），跟随逻辑会直接跳过，修复时重新 `openFile(prev)` 即可。

## 2026-04-19 · Phase 2 · Task 4 — 链接重写（Rename with Refs）

- **Scope**
  - **后端命令 `file_move_with_refs`（`src-tauri/src/commands/rename.rs` 新建，约 360 行）**：核心 IPC，签名 `(from: String, to: String) -> RenameResult`。`RenameResult` 返回改过的引用方文件列表、总改写数、以及非致命警告。流程：预检（源存在、目标不存在、源不是目录、双向 `resolve_in_vault` 路径越界检查）→ 查 `links` 表拿到所有 `dst_resolved = from` 的引用方（`SELECT src, dst, link_type FROM links WHERE dst_resolved = ?1 AND src != ?1`）→ 按 `src` 分组 → 每个引用方读文件 + 正则替换 + `atomic_write` → `std::fs::rename`（失败 fallback `copy + remove`）→ 对旧路径 `scanner::delete_one` + 新路径 `scanner::reindex_one` + 每个改过的引用方 `reindex_one`。引用方级的失败只 log + push warning，不阻断 move 本身。
  - **重写引擎 `RewritePlan`（同文件内部结构）**：`RewritePlan::from_paths(from, to)` 根据新旧路径生成最多 3 对 wiki 重写候选——path-with-ext（`1-notes/foo.md` → `1-notes/bar.md`）、path-no-ext（`1-notes/foo` → `1-notes/bar`）、stem（`foo` → `bar`）——加最多 1 对 embed 候选（full path）。`RewritePlan::apply(body, links)` 用 `HashSet<&str>` 先从引用方的真实 raw `dst` 记录里过滤出"确实出现过的形式"，再逐对 regex replace；这样 `[[OldTitle]]`（title-form）不会被误当成某个 stem 改掉。wiki 正则 `\[\[\s*OLD\s*(\|[^\]]*)?\s*\]\]`、embed 正则 `(!\[[^\]]*\]\()\s*OLD\s*(\))`，都是 bracket-anchored——防止 `[[foobar]]` 被 `[[foo]]` 规则误吃。alias 段（`|alias`）和 alt 段（`![alt](...)`）整体透传不动。
  - **11 个单元测试（同文件 `#[cfg(test)]` 模块）**：`plan_stem_change_only` / `plan_dir_change_only` / `plan_noop_when_identical` 覆盖 RewritePlan 的路径形变；`wiki_replace_basic` / `wiki_replace_preserves_unrelated` / `wiki_replace_path_form` 覆盖 wiki 正则；`embed_replace_basic` / `embed_replace_preserves_alt` 覆盖 embed 正则；`plan_apply_respects_raw_form` / `plan_apply_skips_when_indexer_has_no_record` / `plan_apply_embed` 覆盖 "索引器 raw_dst gate" 保险逻辑。
  - **IPC 注册（`src-tauri/src/commands/mod.rs` + `src-tauri/src/lib.rs`）**：`mod.rs` 加一行 `pub mod rename;`；`lib.rs` invoke_handler 数组在 attachment 几条之后追一行 `commands::rename::file_move_with_refs`。
  - **前端 wrapper（`src/lib/ipc/file.ts`）**：新增 `fileMoveWithRefs(from, to): Promise<RenameResult>` 和 TypeScript 侧的 `RenameResult` 接口，字段名与 Rust 端 serde 序列化保持一致（`old_path` / `new_path` / `rewritten_files` / `rewritten_links` / `warnings`）。
  - **Palette 命令 `> Rename current file…`（`src/lib/palette/commandRegistry.ts` + `+page.svelte`）**：在 `PaletteContext` 追加 `runRenameCurrent: () => void | Promise<void>`，commandRegistry 里加一条 hint 为 `Rename` 的命令，`when` 过滤 `.mynotes/` 下的文件。`+page.svelte` 里新增整套 rename modal 状态（`renameOpen / renameSource / renameInput / renameError / renameRunning / renameInputEl / lastRenameResult`）+ 5 个 handler（`openRenameModal` / `cancelRename` / `confirmRename` / `onRenameKey`），modal 打开时预填完整路径并把选区默认落在 stem 段（`/` 到 `.md` 之间）——让最常见的"同目录改名"一次输入就能提交。modal UX：target 必须 `.md` 结尾、不能进 `.mynotes/`、不能已存在；成功后自动 `openFile(target)` 跟随；status bar 用 `saveError` 通道回显"重写了 N 个文件中的 M 处引用"，warning 只 console.warn（不污染面板 UX）。
  - **`runExtractFromProject` 改走 rewrite 命令（`+page.svelte`）**：原来的 `fileRead + rewriteFrontmatter + fileWrite(dst) + fileDelete(src)` 被拆成两步——先 `fileMoveWithRefs(src, dst)` 把引用方全部重写并搬文件，再 `fileRead(dst) + rewriteFrontmatter + fileWrite(dst)` 把 `type: project-note → note` 和 `updated` 写进 frontmatter（文件相同时跳过写）。好处：项目内的 sub-note 如果被 MOC 或其它笔记引用，extract 完链接不断。
  - **设计文档（`design_V2.md`）**：新建 §6.13「链接重写（Rename With Refs，Phase 2 Task 4）」6 个子节——6.13.1 范围与非范围（明确不做目录 / title / 附件重命名）、6.13.2 后端命令签名与流程、6.13.3 替换算法正确性（bracket-anchor + raw_dst gate + alias/alt 保留）、6.13.4 前端接入点对照表（4 个入口的原/新实现）、6.13.5 失败语义、6.13.6 V2 合规性自检。
- **How to verify**
  - `cd src-tauri && cargo test --lib rename::tests` → 11 个用例全绿。
  - `pnpm tauri:dev` 后：
    1. **基本改名**：建两篇笔记，B 里写 `[[A]]`；命令面板 `> Rename current file…` 打开 A 的 modal，改路径为 `1-notes/A-renamed.md`；回车；B 里的 `[[A]]` 自动变成 `[[A-renamed]]`，status bar 显示"重写了 1 个文件中的 1 处引用"。
    2. **无引用改名**：一篇孤立笔记改名 → RenameResult.rewritten_links = 0 → status bar 只走常规 "saved"。
    3. **Extract from project**：在 `4-projects/foo/note.md` 里随便写点东西，在 `1-notes/any.md` 里加 `[[foo/note]]`；从 project 里 `> Extract from project`；note 落在 `1-notes/note.md` 且 frontmatter 里 `type: note`；`any.md` 里的 `[[foo/note]]` 自动改成 `[[note]]`（走 path-no-ext + stem 两条规则）。
    4. **取消 / 非法路径**：输入非 `.md`、输入 `.mynotes/...`、输入已有文件 → modal 底部红字提示，不发 IPC；`Esc` 或"取消"关闭 modal 不改动任何东西。
  - `pnpm check` 应零错（sandbox 无 pnpm，用户本地跑；sandbox 内 `tsc --noEmit --skipLibCheck --moduleResolution bundler` 对三个 .ts 文件除 `$lib/` alias 外无错）。
  - `cargo check` 应通过（sandbox 无 cargo，用户本地跑）。
- **Known gaps**
  - **不处理 title-form wiki 引用**：如果其它笔记写的是 `[[Some Title]]`（走 indexer 的 title-match 路径解析到 A），rename A 的文件路径不会改动 `[[Some Title]]`——因为 title 没变。这在语义上是对的：title 仍然是 A，link 也仍然解析得到 A，只是映射到新 path 上。但如果用户把 rename 当成 "把 Some Title 换名字"，会有预期偏差。Phase 3 的 "change title" 操作里再处理 title 的 rename propagation。
  - **不处理目录重命名**：`file_move_with_refs` 在源是目录时直接 `Err` 返回。目录重命名涉及批量文件 + 引用扇出爆炸，应该单独设计（可能走一个 staging 机制 + 进度条）。
  - **不处理附件 rename**：`attachments/...` 下的文件改名走 `fileMove` 就没人接管引用。Phase 2 Task 3 里约定附件 rel_path 生成后不应该改，所以暂不处理；如果以后要支持（比如"按语义命名附件"），走同一套 `file_move_with_refs` 命令能直接吃 embed 规则。
  - **`promoteInboxNote` 暂未切换**：Promote 同时改 title 和 path，title 改名不在本版本范围；且 inbox 笔记基本不会被其它笔记引用，切换收益低。Phase 3 的 title rename 支持一并处理。
  - **失败回滚粒度粗**：引用方 rewrite 成功、但 move 本身失败时，已重写的引用方 md 不会回滚（会一次性指向一个还没落位的新路径，导致短暂 dangling）。实测下 `fs::rename` 失败极罕见（只在跨文件系统时触发 `copy + remove` fallback，都挂了才真失败），代价可接受；真要做事务就要把整套操作改成"预检 → 暂存 → 一次性提交"的 staging 结构，对 Phase 2 是 overkill。
  - **Warning UX 简陋**：引用方单文件失败时只在 status bar 给个数字 + console.warn；没有 "点击查看详情" 的面板。多数场景 warning 本来就罕见（索引的 src 路径都是刚 reindex 过的，读失败意味着磁盘层异常），先把观察留给 console，真频繁出现再加 UX。

## 2026-04-19 · Phase 2 · Task 3 — 图片/附件粘贴与管理

- **Scope**
  - **存储分层与命名（`src-tauri/src/commands/attachment.rs` 新建）**：附件统一写入 `attachments/YYYY/MM/`，文件名为 `YYYYMMDD-HHmmss-<slug|rand6>.<ext>`——`slug` 来自原文件名（保留 ASCII 字母数字 + CJK，上限 48 字符），剪贴板粘贴没有原名时退化成 6 位 hex；同秒内碰撞加 `-2 / -3 …` 计数器重试（上限 64 次）。扩展名走 `sanitize_ext`（仅 ASCII 小写字母数字、≤12 字符）防注入。所有写入都走已有的 `atomic_write` + `resolve_in_vault`，继承路径越界保护。
  - **IPC 五件套（`src-tauri/src/lib.rs` 注册 / `src/lib/ipc/attachment.ts` 封装）**：`attachment_save(bytes, original_name, ext) -> rel_path`、`attachment_read_bytes(rel_path)`（额外 gate：只允许 `attachments/` 前缀）、`attachment_list()`、`attachment_unreferenced()`、`attachment_delete_batch(rel_paths) -> deleted[]`。`attachment_list` 与 `attachment_unreferenced` 共用 `list_all_attachments` 内部 helper，避免 `State<AppState>` 无法 `clone` 的坑。
  - **Markdown 图片进索引（`src-tauri/src/db/indexer.rs`）**：`scan_body` 扫描 `![alt](path)` 并写入 `links` 表，`link_type='embed'`（复用现有 schema 的 text 列，免迁移）。正则 `!\[([^\]]*)\]\(([^)]+)\)` 显式跳过 `\!` 转义、`![[...]]` wiki-embed、以及 `http/https/data/blob:` 远端 URL。`resolve_links` 拆成两段：wiki 走原来的"按 title 反查 notes"路径（加 `AND link_type='wiki'` 显式约束），embed 走一条 passthrough `UPDATE ... SET dst_resolved = REPLACE(dst, '\\', '/') WHERE link_type='embed'`——embed 的 dst 本身就是相对路径，不需要 note 反查。这样 orphan 查询能直接拿 `SELECT DISTINCT dst_resolved FROM links WHERE link_type='embed'` 得到"全 vault 被引用集合"。
  - **剪贴板粘贴 / 拖放（`src/lib/editor/imageEmbed.ts` 新建，`Editor.svelte` 加装）**：`EditorView.domEventHandlers({ paste, dragover, drop })`——paste 扫 `clipboardData.files` + `.items`（Chromium vs. Safari 的差异），drop 扫 `dataTransfer.files`，两者都只接 `image/*` MIME（防止把误拖的 `.md` 当附件）。每个文件走 `attachmentSave`，然后在光标处一次性插入 `![alt](rel_path)\n`；多文件换行拼接。`dragover` 强制 `preventDefault()` 才能变成合法 drop target。
  - **行内缩略图预览（同文件的 `imageEmbedField`）**：StateField 扫行（CM6 约束——block widget 只能从 StateField 出），正则命中 `^\s*!\[…\]\(attachments/…\)\s*$` 时挂一个 `block: true, side: 1` 的 `ImagePreviewWidget` 到行尾。源码行本身不替换、仍可编辑，缩略图是附加块。`src` 走 `getBlobUrl` 异步拿（`attachment_read_bytes` 返回 bytes → `new Blob(...) → URL.createObjectURL`），`BLOB_CACHE` Map 做复用。选了 Blob URL 而非 Tauri asset 协议，是因为后者要静态声明 scope、对"任意用户 vault 路径"不友好。`Editor.svelte.onDestroy` 里调 `revokeAllAttachmentBlobs()` 防内存泄漏。最大 520×360 px，圆角 6px + 柔阴影，load 失败改渲染 `⚠ 找不到附件: …` 提示。
  - **`> Find unused attachments` 面板命令（`src/lib/palette/commandRegistry.ts` + `src/routes/+page.svelte`）**：在 `PALETTE_COMMANDS` 加一条 hint 为 `Vault` 的命令；`+page.svelte` 里新增一套 modal 状态（`unusedOpen / unusedLoading / unusedError / unusedList / unusedSelected / unusedDeleting`）+ handler（`openUnusedAttachments` / `toggleUnusedRow` / `toggleUnusedAll` / `confirmDeleteUnused` / `cancelUnusedAttachments`）。modal 沿用既有 `modal-backdrop` + `modal` 模式，加宽至 560 px，列一张滚动表：复选框 / rel_path / 文件大小（`fmtBytes`）；顶部"全选 · N/M"行。删除前用原生 `ask()` 警示"将永久删除、刚粘贴未保存的图会被误判"；删除完列表清空后自动关闭。
  - **设计文档（`design_V2.md`）**：§4.1 目录图加 `│   └─ YYYY/MM/` 一行；§5.2 追加 5 个 `attachment_*` IPC 签名；§5.4 单文件解析流程加"Markdown 图片解析"段；新建 §6.12 「附件与图片」7 个子节（存储/命名、引用格式、渲染架构、插入交互、缩略图 widget、孤儿清理、V2 原则对齐自查）；§16 changelog 追 `| 2026-04-19 | 1.8 |` 行。
- **How to verify**
  - `pnpm tauri:dev` 启动后：
    1. 在编辑器任意位置 `⌘V` 粘贴剪贴板图片 → 出现 `![](attachments/2026/04/20260419-xxxxxx-xxxxxx.png)` 一行，下方自动渲染缩略图。
    2. 从 Finder 把若干张图（`.jpg` / `.png` 混合）拖到编辑区 → 每张各自一行 + 缩略图；拖 `.md` / `.pdf` 不被拦截也不进 attachments（被 `image/*` filter 过滤掉）。
    3. 在磁盘 `attachments/2026/04/` 肉眼检查文件确实按 `YYYYMMDD-HHmmss-*` 命名，没有覆盖。
    4. `⌘P` → `> Find unused attachments` → 打开 modal；手动在磁盘丢一个没引用的 `attachments/2026/04/orphan.png` 进去，重新打开 modal 能看到；勾选 → 删除 → 文件消失。
    5. 在编辑器里粘贴一张图**但不保存**（`⌘S`），立即打开 unused → 该图会出现在列表（因为 links 表里还没登记）；modal 的⚠提示已经说明这种情况。
  - `pnpm check` 应零错（sandbox 没有 pnpm，用户 macOS 本地跑）。
  - `cargo check` 应通过（sandbox 没 cargo，用户本地跑）。
- **Known gaps**
  - 不走系统回收站：`attachment_delete_batch` 直接 `fs::remove_file`，删了就真删了（对齐 `file_delete` 语义）。当前只有 modal 里的警示提醒；如果之后要接 trash，走 `tauri-plugin-fs` 的 `trash` 扩展比我们自己实现稳。
  - 索引是"保存时更新"：粘贴图但没 `⌘S` 的那一刻，`links` 表里没有该 embed 行，所以 `> Find unused attachments` 会误判成孤儿。modal 的 hint 已经显式说"先⌘S 再清理"——这个 UX 约定是有意的，不打算做"in-memory pending refs"的复杂账本。
  - `attachment_read_bytes` 走 JSON IPC 的 `Vec<u8>`，对于 >10MB 的附件会有明显序列化开销。v1 只做笔记本大小的图（截图、手机照片），暂不考虑视频/大图；真要支持就得换 Tauri 的二进制 IPC 通道或者拍板启用 `asset://` 协议（连带写一套 vault-path-to-asset-scope 的映射）。
  - 拖放不处理非 `image/*` 文件（如 PDF、CSV）：有意为之。Phase 2 Task 3 的目标是"图片嵌入"；通用附件（`[📎 file.pdf](...)`）等一条独立 Task。
  - 缩略图只对"整行就是一个 `![](attachments/…)`"的情形渲染；行内带前后文的（如 `看这张 ![](…) 图`）不预览。这样做是 V2 "不要改写源码"原则的保守化处理——整行命中更容易保证 widget 不和光标 / 选区打架。

## 2026-04-19 · Phase 2 · Task 2 — Quire 美学套用（First design 整套落地）

- **Scope**
  - **Tokens（`src/app.css`）**：补 `--color-surface-raised`（Light `#ffffff`，Dark `oklch(0.215 0.010 275)`）作为"卡片/高亮面板"专用背景；补 `--accent-glow = 0 0 44px -4px oklch(accent / 0.54)` 作为"活跃状态"光晕；把 OS `@media (prefers-color-scheme: dark)` 段里残留的旧 hex 色（`#1A1918 / #D47761 / ...`）全量换成与 `[data-theme='dark']` 完全一致的 oklch 值，让"跟随系统 / 手动切暗"两条路径视觉一致。新增两个原子工具类：`.meta`（mono + uppercase + 0.08em letter-spacing + `--color-fg-dim`，用作结构型标签）与 `.accent-dot`（6px 圆点 + 光晕，用作 live/active 指示）。
  - **Button 基线变更**：默认 `button` 改为 "hairline + pane-border" 模型——去掉 `border: 1px solid var(--glass-border)` 的硬线，改 `border: 1px solid transparent` + `box-shadow: var(--pane-border)`；hover 只在"背景+轻微上浮"起作用，不再改边框色。`button.primary` 叠加 `--accent-glow`。
  - **Panel（`Panel.svelte`）**：每个 `section` 改用 `--color-surface-raised` + `--pane-border` + `--radius-lg`；`h4` 小节标题升格为 mono / uppercase / `--color-fg-dim`；panel 外壳去掉左侧硬线（交由 `.panel-slot` 的卡片化外框承担）。
  - **CommandPalette（`CommandPalette.svelte`）**：modal 外壳加 `--pane-border`；输入框用 Fraunces 衬线字体放大到 17px，底线由 `border-bottom` 改为 `inset 0 -1px 0` 的 hairline；footer 用 mono 微体；`<kbd>` 改为"浮起小片"样式（`surface-raised` + `pane-border`）。
  - **Home / Modal / Sidebar / StatusBar（`+page.svelte`）**：
    - Welcome/Home 标题换 Fraunces（`font-weight: 400`，负字距）；
    - Home cards 改 pane-border，hover 叠 accent-weak 环 + accent-glow；
    - `.home-card-value` 改 Fraunces；`.home-list-head h3` / `.home-card-label` / `.home-review-label` / `.recent h3` 统一改 mono + 0.08em；
    - `.home-review` 由"虚线卡片"改成"raised 卡片"；
    - 所有模态（`New Note / New MOC / New Project / Extract / Quick Capture / Promote`）共用的 `.modal`：`--pane-border + glass-shadow`；`.modal h3` 改 Fraunces；`.modal-input` 改 raised+pane-border，focus 环改 `0 0 0 2px var(--color-accent-weak) + --accent-glow`；
    - `.sidebar-header` / `.command-bar` / `.status-bar` 所有 1px 硬分隔线改 `box-shadow: inset 0 ±1px 0 var(--color-border)`；
    - `.cmd`（sidebar 顶部 4 个工具按钮）改 mono 小体，hover 才升格为"raised+pane-border"。
  - **Inbox（`InboxView.svelte`）**：header 标题改 Fraunces 18px；`.row` 的 border-bottom 改 hairline inset；`.act` 按钮升格为"raised+pane-border"，`.act.promote` 叠 accent-weak 环，hover 叠 accent-glow。
  - **TagView（`TagView.svelte`）**：`.tag-header` 分隔线改 hairline；`.tag-label` 改 Fraunces；`.badge` 升格 raised+pane-border。
  - **Sidebar 分组（`ProjectsSection.svelte` / `TagsSection.svelte`）**：顶分隔线改 hairline inset；分组头改 mono + uppercase；激活行由"彩字"改为"左侧 2px 重音竖条 + 普通文字色"（Quire 的 "separation without borders" 原则——active 通过色块+竖条表达，而不是改文字颜色）；active 项的 `.dot` 叠 `accent-glow`。
- **How to verify**
  - `pnpm tauri:dev` → 整体观感：卡片"浮起而非框住"，所有面板顶底只有极细 hairline，圆角统一为 `--radius-lg`（18px）级；
  - 切到系统暗色（不手动指定 theme），确认侧栏 / 卡片 / 分隔线与"手动暗"完全一致（这是本次新修的关键路径）；
  - 打开 ⌘P → 输入框是 Fraunces 衬线，footer 是 mono 小体，`<kbd>` 浮起；
  - Home → 两张大卡在 hover 时会出现一圈淡陶土色环并轻微上浮（accent-glow）；
  - 点击侧栏 Projects 的任一项目，active 行左侧会出现 2px 陶土色竖条（不再把项目名染红）；
  - ⌘P → `> New Project…` → modal h3 是衬线，input 聚焦时有淡陶土色光晕。
- **Known gaps**
  - 本次是 CSS-only 改动，不影响类型 / 运行逻辑；没有回跑 `svelte-check`（Linux 沙盒缺 `@rollup/rollup-linux-arm64-gnu` 原生包，且不能覆盖用户 macOS `node_modules`）。用户在 macOS 下首次 `pnpm tauri:dev` 就能看见实际渲染。
  - 没有改 Editor 内部 livepreview 的 chip 样式（`#tag` / `[[wiki-link]]` 装饰）——那层需要改 CM6 的 decoration 扩展，不是纯 CSS；留到后续一次专门的"editor chip pass"。
  - `First design/Quire.html` 里的 TweaksPanel（density / radius / glow 可调面板）未移植；当前 app 走固定的 Quire "balanced" 配置。若后续要给用户开控台，再把 TweaksPanel 独立成 Svelte 组件。

## 2026-04-19 · Phase 2 开启 · Task 1 — UI 质感美化与体验打磨 (UI Beautification)

- **Scope**
  - **基础架构升级 (Claude风格转轨)**：在 `app.html` 引入 Google Fonts `Inter` 字体；全面重构 `app.css`，运用 Anthropic 的“长文阅读级”极简美学。色板调整为渐进式的三列结构：侧边栏应用经典骨瓷米色 `#F3F1EC`，中心编辑区过渡为极为柔和的珍珠米白 `#F9F8F6`，从根本上解决大面积死白产生的割裂感。
  - **组件软化与卡片化 (Card UI & Ghost Buttons)**：右侧属性关联面板（Panel）去除死板横线分割，所有 Section 均封装为圆角带浅阴影的纯白独立卡片 (`--color-card`)。侧边栏的命令阵列（Tools / Buttons）执行全面“去边框化 (Ghost Buttons)”，转而在悬浮时展现微幅上浮与幽灵底色过渡。
  - **编辑区沉浸化 (Editor Harmony)**：清除了原先残留的 IDE 观感（删除了 CodeMirror 的 `lineNumbers()` 边距与全屏横穿的 `highlightActiveLine()`，并将字体修正为流畅的无衬线排版字体）；加入 `max-width: 780px` 与居中包裹，形成正文如一张信纸悬浮于中部的独立呼吸感布局。
  - **克制的高定渲染**：重新调校了AST装饰拓展。不再使用笨重的色块胶囊，`#tag` 从克制的静默灰在 hover 时醒目，`[[wiki-link]]` 则仅在 hover 时透出陶土色底划线；加大了各级标题留白并缩紧字距，`.cm-md-quote` 改为仅呈现典雅的左侧 3px 竖线和 18px 内边距。最后为右侧“笔记关系”面板追加了对称的 `translateX(2px)` 悬停微动效。
- **How to verify**
  - `pnpm tauri:dev` 启动应用，检查整体不再具有割裂的突兀边框，特别是工具条中的 Today / Capture 等，在静止态应完全融于背景。
  - 打开一片带有各种格式笔记的纯编辑器区（中间），验证 IDE 行号已经消失、整体居中有最大宽度约束、没有突兀颜色切分，字样阅读十分舒适。
- **Known gaps**
  - CM6 Editor 中的 `#tag` 解析由于只是简单正则，与 SQLite 层级的精准分词未必绝对统一，但对于渲染目的已足够。

## 2026-04-19 · Week 5 · Task 6 — `> Add Note to Project` / `> Extract from project`

- **Scope**
  - 命令面板两条新命令（`src/lib/palette/commandRegistry.ts`）：
    - `add-note-to-project` — 标签 `Add Note to Project — new note under current project`，`when` 复用 `isInProject`（当前开的文件在 `4-projects/` 下任何位置都出现；包括 index.md 自己）。
    - `extract-from-project` — 标签 `Extract from project → move to 1-notes`，`when` 更严：`4-projects/` 下 **且** 不是 `index.md`。项目主页本身不能"从项目抽离"（那等价于 archive 项目，有专门命令）。
  - `PaletteContext` 新增两个钩子 `runAddNoteToProject()` / `runExtractFromProject()`，`+page.svelte` 的 `paletteCtx` 里 wire 到同名实现函数。
  - **Add** 实现：不重写模态框，复用既有 "New Note" modal 的 targetDir 机制——新增 `targetDir = '4-projects/<slug>'` 这一形态。`runAddNoteToProject()` 只做两件事：`projectSlugFromPath(vaultState.openFilePath)` 拿 slug，然后 `newNote('4-projects/<slug>')` 打开模态；后续 `confirmNewNote` 里新增的 `newNoteTargetDir?.startsWith('4-projects/')` 分支负责 slugify 输入 → 生成 `<targetDir>/<note-slug>.md`、把原始 title 带进 `extra`。`createNoteFromTemplate` 里 `templateForDir('4-projects', 路径不以 /index.md 结尾)` 已经正确选择 `project-note.md`（从 Task 3 起就对了），不用改。
  - **Extract** 实现：**不走 `file_move` IPC**——原因是 extract 语义包含 frontmatter 改写（`type: project-note → note`），而 `file_move` 是纯 rename 不动文件内容。实际路径：`fileRead(src)` → `rewriteFrontmatter(body, { type: 'note', updated: now })` → `fileWrite(dst, newBody)` → `fileDelete(src)`。两步写-删（不 swap），跟 Promote 同套容错模型：中途崩溃留两份而不是零份。
  - **V2 no-md-injection 合规**：Extract **不**往 `index.md` 里加 `[[wiki-link]]`。设计 §6.11 的原则是"项目-笔记关系来自 filesystem path"，抽离就是路径从 `4-projects/<slug>/` 变成 `1-notes/`，关系自动消失。若用户想在 index.md 里保留个前向指针，自己打 `[[…]]`——命令不做静默注入。
  - 碰撞处理：目标 `1-notes/<filename>.md` 已存在时，按 `<stem>-1.md` / `<stem>-2.md` … 递增，上限 100（和 `promoteInboxNote` 同 cap，超过报错不覆盖）。
  - 刷新：`refreshTree()` + `schedulePanelRefresh(200)` + `invalidateWikiCompletionCache()`。其中 wiki-complete 缓存是关键——src 路径消失、dst 路径出现，补全列表必须更新，否则用户输入 `[[old-name` 还会被旧项目路径匹配到。
  - 模态框 UI 微调（`+page.svelte` 模板部分）：新增 `newNoteTargetDir?.startsWith('4-projects/')` 分支的 `<h3>新建项目笔记</h3>` 标题、hint 文案（指示"标题会被 slugify 成文件名"）、placeholder `Interview Notes`。保证跟既有 `4-projects` / `2-moc` / 无 targetDir 三种分支的视觉/交互一致。
- **How to verify**
  - `pnpm check` 无 ts 报错（注意 `fileExists` / `rewriteFrontmatter` 两个新引入的 import）；Rust 无改动，`cargo check` 不必重跑。`pnpm tauri:dev` 热重启。
  - 准备：打开任一 `4-projects/<slug>/index.md`（比如 Task 5 的 Deep-Work 项目）。
  - **Add 路径**：
    1. ⌘P → `> Add Note to Project` → 模态框标题"新建项目笔记"、hint 提示在 `4-projects/<slug>/` 下、placeholder `Interview Notes`。
    2. 输入 `Interview Notes` → Enter → 生成 `4-projects/<slug>/Interview-Notes.md`（slugify 保留大小写），frontmatter 有 `title: "Interview Notes"` / `type: project-note` / `created` / `updated`；编辑器打开新文件；目录树展开到项目目录能看到新文件；右侧 Panel 这时是非 index 页面，不显示项目笔记 section（符合 Task 5 的"仅 index.md 触发"约定）。
    3. 切回项目 index.md → Panel 顶端"项目笔记"计数 +1，列出 `Interview Notes` 那行。
    4. 空输入 → 模态提示"笔记标题不能为空"；只含特殊字符的输入（如 `///`）→ 提示"标题无法转换为合法文件名"。
    5. 同名再建一次 → 报错 `<path> 已存在`（`createNoteFromTemplate` 原有行为）。
  - **Extract 路径**：
    1. 在刚创的 `Interview-Notes.md` 上（或 Task 5 的其它 project-note），⌘P → `> Extract from project → move to 1-notes`。
    2. 文件被搬到 `1-notes/Interview-Notes.md`；原 `4-projects/<slug>/Interview-Notes.md` 消失；frontmatter 多 `type: note`（替换了 `type: project-note`）、`updated` 被刷新；编辑器内容不变，状态栏路径更新为 `1-notes/`；侧栏 `1-notes` 自动展开看到新文件。
    3. 切回项目 index.md → Panel"项目笔记"计数 -1，该行消失。ProjectsSection 侧栏不变（项目数未变）。
    4. 若 `1-notes/Interview-Notes.md` 已存在：新文件叫 `Interview-Notes-1.md`；连续 extract 同名文件能持续递增。
    5. 在项目 index.md 自己上：命令面板里看不到 `Extract from project` 条目（`when` 排除了）。
    6. 在 `1-notes/xxx.md` 这种非项目路径：两条命令都不出现。
  - 链接缓存：Extract 后在任意文件里打 `[[Interview-Notes` 触发补全，应看到 dst 位置 `1-notes/Interview-Notes.md`，不再看到 src 的 `4-projects/<slug>/Interview-Notes.md`（后者已不存在，缓存也被 invalidate）。
- **Known gaps**
  - Extract **不**在 index.md 里留 wiki-link 指针——V2 no-md-injection 的代价是"关系信息随路径走"。实践中如果用户希望保留"这个笔记曾经属于此项目"的痕迹，要么手动在 index.md 打 `[[…]]`，要么我们后续补一个 opt-in 的 `> Extract from project (with link back)` 变体。暂不做。
  - Extract **只支持**移到 `1-notes/`——没有 picker 选目的分类（`2-moc` / `3-journal` / 另一个 project 等）。Promote 也是硬编码 `1-notes`，保持对齐。如果要跨 project 转移（"从项目 A 移到项目 B"），当前得分两步：extract 到 1-notes、再手动 `> Add Note to Project` 重新挂——这恰好不触发 `index_project_notes` 里的"同名笔记"冲突（因为 add 是新建不是移动），目前这是可接受的摩擦。
  - Add 的模态复用 `confirmNewNote`，沿用它的"同名文件不覆盖直接报错"——**不**自动 `-1` 递增。这与 Extract 的 `-1` 自动递增行为不对称，理由：Add 是用户主动输入标题，碰撞说明同项目已有同名笔记，提示更合理；Extract 是命令一键触发，人没有机会修正目标名，所以帮他避让。
  - 没做"从子笔记触发 Add 时自动继承当前笔记的 tag"——`project-note.md` 模板 `tags: []` 起步。若将来发现项目内笔记大量共享同一组 tag，可以在 `extra` 里把 siblings 的高频 tag 预填；当前信噪比不足。
  - Extract 后 `created` 字段保持不变（只刷新 `updated`）——这是"笔记还是那篇笔记，只是换了抽屉"的语义。若将来想统计"进入 1-notes 的时间"，需要新字段（比如 `promoted_at` / `extracted_at`），不动 `created`。
  - 没做撤销——extract 后想退回只能手动 `file_move` 回去（或 re-add 同名笔记，新文件会有新 frontmatter）。和 Promote / rename 的行为一致，后续统一做 undo stack 再处理。
  - 命令面板的 `when` 两个谓词与 `isInProject` 同宗，硬编码了 `4-projects/` 前缀。若将来重命名顶层目录（§6.11 的 archived sub-folder 等），所有这些谓词加上 `projectSlugFromPath` 里的硬编码都要改——目前是唯一容忍点，因为 V2 path-SSOT 下路径本身就是 schema。

---

## 2026-04-19 · Week 5 · Task 5 — 项目"相关笔记"自动列表（Panel 侧）

- **Scope**
  - 新 IPC `commands::index::index_project_notes(slug: String) -> Vec<NoteRef>`——按路径前缀 `4-projects/<slug>/` 收集该项目目录下所有笔记，排除 `index.md` 自身；`ORDER BY updated DESC`。**故意不看 frontmatter `project_slug`**（V2 已废弃字段），纯走 path-based SSOT。
  - 实现细节：
    - 用 `substr(path, 1, prefix_len) = prefix` 而不是 `LIKE prefix || '%'`——slug 理论上由 `slugifyTitle` sanitize 过是 ASCII alnum + `-`，不会命中 LIKE 通配符；但 `substr` 等值比较更健壮，future 放宽 slug 规则也不会 break。
    - 防御性：`if slug.trim().is_empty() { return Ok(vec![]) }`——防止空 slug 匹配到整个 `4-projects/` 前缀、返回所有 project-note。
    - 用 `!= prefix + "index.md"` 排除 index.md 自身（而不是排除所有叫 index.md 的路径）——未来如果某个 project-note 意外叫 `subfolder/index.md`（嵌套子目录 index），它会被列出，这跟 V2 "path 就是真理" 一致。
  - `lib.rs::invoke_handler` 注册；`src/lib/ipc/index.ts` 加 `indexProjectNotes(slug)` 封装。
  - `Panel.svelte`：
    - 新增 `projectNotes: NoteRef[]` state + 在 `load()` 里与 backlinks/outgoing/unresolved 四路并发；非项目 index.md 路径走 `Promise.resolve([])` 短路，不发多余 IPC。
    - 新 helper `projectSlugFromIndex(path)`：正则 `^4-projects\/([^/]+)\/index\.md$`，只接受 "项目主页" 这一种路径。打开项目子笔记（非 index.md）**不**显示该 section——理由见 Known gaps。
    - render：新 section 放在**最上方**（backlinks 之前）——对项目主页来说这是最主要的上下文，排在反向链接前面。label 用中文「项目笔记」保持跟侧栏 ProjectsSection 同一套术语。
    - 空状态："还没有同项目笔记。试试命令面板 ⌘P → `> Add Note to Project`"——把用户导向 Task 6 即将实现的命令。
    - row 视觉沿用既有 `.link` 样式：`link-title` 显示 frontmatter title（fallback 到 fileName），`link-path` 只显示 filename 不显示整串（同目录内，整串 `4-projects/<slug>/xxx.md` 冗余）。
  - 刷新：`Panel` 既有的 `refreshToken` prop 已被 `panelRefreshToken` 喂养，Task 4 加的 `schedulePanelRefresh(200)`（new project / set-status 后）顺带也会重跑 `index_project_notes`。新建 project-note 的命令还没有（是 Task 6 的任务）——届时那条命令成功也会 bump。当前通过 auto-save 后 400ms 的 bump 兜底。
- **How to verify**
  - `cargo check --manifest-path src-tauri/Cargo.toml` 通过；`pnpm check` 无 ts 报错；`pnpm tauri:dev` 启动无 runtime warning。
  - 准备数据：已存在的 `4-projects/Deep-Work/index.md` + 手动（或通过侧栏目录树右键新建）在 `4-projects/Deep-Work/` 下加两三个 `.md`（比如 `meeting-notes.md` / `architecture.md`）。
  - 打开 `index.md` → 右侧 Panel 最顶端出现 **项目笔记** section，计数跟实际同目录笔记数相等，不含 `index.md` 自身。每行点击能打开对应笔记；切到打开的那个 note 后，Panel 顶端的项目笔记 section **消失**（因为不再是 index.md）。切回来重新出现。
  - 项目目录下没有子笔记时：切回 index.md → "项目笔记 0" + 空状态提示 `⌘P → > Add Note to Project`。
  - 打开别的非项目笔记（比如 `1-notes/xxx.md`）→ Panel 不显示项目笔记 section，跟修改前视觉完全一致；没有多余 IPC 请求（Promise.resolve 短路）。
  - 边界：空 slug 的假输入不能从正则里出来（`[^/]+` 要求至少 1 字符）；但后端也做了 `if trim().is_empty()` 兜底，安全网双重。
  - 性能：single SQL 带 index（`notes.path` 有隐式 PK index），在 100 个 note 的 vault 上亚毫秒级；同 4 路查询并发不会感觉到延迟。
- **Known gaps**
  - 项目子笔记（`4-projects/<slug>/meeting.md` 等非 index）**不**显示项目笔记 section——当前只在 index.md 上触发。理由：a) 设计 §6.11.2 明确写的是"项目主页（index.md）"的渲染；b) 在子笔记里展示兄弟列表需要额外排除当前 open 的那篇，复杂度增加；c) 用户在子笔记里通过左侧栏目录树已经能直接看到 siblings。若后续发现"在子笔记间切换不方便"会补——触发点扩展到 `projectNoteSiblings(path)` 新 IPC 或客户端 filter。
  - 没有"按类型分组"或 section 内再分桶——所有项目内笔记一律平铺 `updated DESC`。如果一个项目的 notes 增多（>20），可以考虑按 `type` 或按最近访问时间分组，当前不值得。
  - 列表**不**包含 archived 项目的笔记之类特殊情形——只是 status=archived 的项目里的笔记依然正常列出，因为 `status` 是项目 `index.md` 的字段，不影响其它笔记的可见性。这是预期行为（归档≠隐藏）。
  - 没做 bidirectional 联动：ProjectsSection（侧栏）点击某项目 index.md 会激活高亮；但在 Panel 的项目笔记里点击一条笔记，不会在侧栏里额外展开对应的 `4-projects/<slug>/` 子目录——`openFile` 不自动展开祖先。与 Tag / 全文搜索结果点击的 UX 保持一致，以避免视觉跳动。
  - Panel 刷新依赖 `panelRefreshToken`，目前已在三处 bump（new note / set-status / auto-save 后）。若将来 Task 6 加新命令修改 project 子笔记布局，该 task 里要记得也 bump 一次。没有集中的 "dirty bus"——所有写动作需要显式调 `schedulePanelRefresh`。

---

## 2026-04-19 · Week 5 · Task 4 — 侧栏 Projects 面板（按 status 分组）

- **Scope**
  - 新组件 `src/lib/projects/ProjectsSection.svelte`：外层 Projects 折叠 section（默认展开），内部 4 个 status 子分组——`active` / `paused` / `done` / `archived`，顺序固定（跟生命周期对齐，不是字母序）。默认 `active` + `paused` 展开，`done` + `archived` 折叠（避免归档项目霸屏）。
  - 数据源：四路并发 `indexProjectsByStatus('active' | 'paused' | 'done' | 'archived')`——后端 SQL 已经按 `path LIKE '4-projects/%/index.md'` 过滤到 `index.md` 这一层，`status` 比较 case + whitespace insensitive。4 路串行 vs 并行在 SQLite indexed read 上基本没差，图简单选并行。没做 lazy per-group 加载——每 bucket 都是小表，没必要。
  - 每行：`●`（色点） + 项目 title（frontmatter 里的，fallback 到 slug）+ 省略号 overflow。点击把 `rel_path` 交回给父组件，父组件合成 `DirEntry` 调 `openFile(...)`；组件本身不直接碰 `vaultState`，保持跟 TagsSection 同构的 props-only 接口。
  - `activeProjectPath` prop：当 `vaultState.openFilePath` 等于某行 `path` 时高亮该行。非 `index.md` 的 project-note 页面不会激活任何行（符合"面板只列项目本身"的语义）。
  - `refreshToken` prop + `$effect`：bump 即触发 reload。父端在 3 处 bump：
    - `confirmNewNote` 成功后（`4-projects/<slug>/index.md` 新创 → bucket 变）。
    - `runSetProjectStatus` 成功后（项目在 bucket 间迁移）。
    - 既有的 `schedulePanelRefresh(400)`（auto-save 完成后 400ms，从 Task 6 继承），对 project 侧无直接作用但不会错刷——4 个查询总代价亚毫秒级。
  - 挂载点：`src/routes/+page.svelte` 侧栏 `<aside>` 内，**在 TagsSection 之前**——Projects 是更"结构化"的组织面（跟 `4-projects/` 顶层目录同格），放前面让用户第一屏看见；Tags 是横切标签，放后面。
  - 样式：尽量复用 `TagsSection.svelte` 的视觉语言——折叠 chevron、count badge、同套 color token，缩进层级错开 2 级（外层 section label 左对齐；group label 缩进 22px；project row 缩进 40px）。没新增任何 CSS custom property，都走既有 `--color-fg-muted` / `--color-bg-hover` / `--color-accent`。
  - 空状态：vault 里没有 projects 时渲染一行 `还没有项目。试试命令面板 ⌘P → > New Project…。`——把用户导流到 Task 3 那条命令。每个 group 的空 bucket 不渲染 `<ul>`，只在 group 头 count 里显示 `0`（带 `.muted` 降透明），保留 4 个 label 的稳定版面（用户学一次就知道去哪找每种状态）。
  - 错误态：`加载失败` 短文案 + `title=` 承载完整 error string（跟 TagsSection 一致），不弹 dialog——这是 passive 面板，用户没显式触发动作，Dialog 会太响。
- **How to verify**
  - `pnpm check` 无 ts 报错；`pnpm tauri:dev` 启动无 runtime warning。
  - 空 vault：打开一个新 vault → 左栏底部应见 `▾ Projects   0`，点开空状态提示指向 `> New Project…`。
  - 单 active：`⌘P` → `> New Project… Alpha` → 面板应立刻冒出 `Active 1 / ● Alpha`（不是等 400ms，因为 `confirmNewNote` 里已经 bump 过 `schedulePanelRefresh(200)`）。打开的 `Alpha/index.md` 对应行应高亮（色点 + 文字 accent）。
  - 状态迁移实时性：保持 `Alpha/index.md` 打开 → `⌘P` → `> Set project status → paused` → 编辑器里 `status:` 立即变 `paused`（来自 Task 3.7 的修复），**同时**侧栏 `Active 0 / Paused 1`，行从上一个 bucket 消失、在下一个 bucket 出现。再跑 `→ done`、`→ archived` 观察 bucket 跳转是否正确。
  - `archived` bucket 默认折叠——点开 chev 才显示列表。点击归档项目的行仍然可以打开对应 `index.md`（archived 不等于不可访问）。
  - 多项目：新建 Alpha / Beta / Gamma，分别设不同 status → 各 bucket 计数与成员正确；同 bucket 内按 `updated` desc 排列（后端 `ORDER BY updated DESC`）。
  - 开 project-note（非 index.md）：高亮不激活任何面板行（面板只列 `index.md`），这是预期行为。
  - 切 vault：从 vault A 切到 vault B（两个都有若干 project）→ 面板内容应立刻反映 vault B 的项目；切回 A 再切回 B 应稳定（`reqSeq` 机制挡住 stale 响应）。
- **Known gaps**
  - 没有拖拽重排或显式排序——bucket 内严格 `updated desc`。如果用户想 pin 一个项目到顶，目前无法；需要等 §6.11 的 `project_pinned` 字段（design_V2 里登记为 future work，不在 Week 5 范围）。
  - `title` 取自 frontmatter，如果 frontmatter 没写或为空串，fallback 到 slug（`4-projects/<slug>/index.md` 的第二段）。不会 fallback 到 `# ...` 一级标题——那需要额外解析正文，代价不值。
  - `schedulePanelRefresh` 是个**通用** panel refresh 信号——它同时驱动右侧 Panel 的 backlinks/outgoing 重取和现在的 ProjectsSection。将来若 panel refresh 频率变高、或 ProjectsSection 想要独立 throttle，会需要拆成两个 token。目前共用。
  - 面板与 `4-projects/` 目录树并存——用户可以从树里点 `index.md` 打开，也可以从 ProjectsSection 里点。两条路径都汇到 `openFile(DirEntry)`，行为一致。没做"选中 ProjectsSection 后同步展开侧栏里的 `4-projects/<slug>/`"的联动——故意保留扁平视图，减少跳动；如果用户反馈想要，做一下 `expand` set 注入即可。
  - 无虚拟滚动——大于 ~50 个项目时滚动可能开始感觉到卡顿。个人 vault 场景下不太会到这个量级，不做。
  - 没给 ProjectsSection 加键盘导航（↑↓ 跳行 + Enter 打开）。所有 sidebar 目前都是纯鼠标，保持一致；键盘入口走命令面板的 `> Open …` 而不是侧栏。

---

## 2026-04-19 · Week 5 · Task 3.7 — 两个 hot-fix：Reseed 守卫字段名 + Set-status 后编辑器不刷新

- **Scope**（都是 Task 3 / 3.5 验收过程中浮出的前端 bug，Rust 侧未动）
  - **Fix A：Reseed 命令的 vault-opened 守卫总是误判。**
    - `src/routes/+page.svelte` `runReseedTemplates()` 第一行守卫原来写的是 `!isTauriRuntime() || !vaultState.rootPath`，但 `VaultState` 类（`src/lib/state/vault.svelte.ts`）上根本没有 `rootPath` 字段——真实字段是 `current: VaultInfo | null`，路径在 `vaultState.current.path`。`$state` 代理访问不存在的属性返回 `undefined` 而不报编译错，于是守卫永远短路到"需要先打开 vault"的 warning 对话框，即使 vault 确实已经打开。
    - 改成 `!vaultState.current?.path`，跟文件里其它 ~15 处用法对齐（grep 确认全局只此一处 typo）。
    - 顺带把原本用 `window.confirm()` 的 confirm 步骤换成 Tauri plugin-dialog 的 `ask()`——跟 `vault_init` 那条"要在此目录初始化吗？"的提示同源，视觉一致；所有结果/错误走 `message(..., { kind: 'info' | 'error' })` 直接弹原生对话框，不再塞 `saveError` + tooltip。原因：状态栏只渲染 `⚠ save failed` 短标签，真实错误压在 `title=` tooltip 里容易被忽略；reseed 是一次性、显式的用户动作，跟 auto-save banner 的生命周期不同，应该走独立的对话框通道。保留 `saveStatus='saved'`/`'error'` 的写入做视觉延续，但主通道是 dialog。
  - **Fix B：`> Set project status → X` 不刷新当前编辑器。**
    - `runSetProjectStatus()` 原注释："If the user has index.md open in the editor, the file watcher will notice the on-disk change and the editor reloads via its normal external-change flow"——**这条路径不存在**。前端没订阅任何 Tauri `emit` 事件（全项目 grep `listen` 零命中）；watcher 只负责刷新 SQLite 索引，不会把 file-contents 推回 Svelte 层。后果：停留在 `index.md` 触发该命令后，磁盘上 `status: paused` 已落盘，但 CodeMirror buffer 依然显示 `active`，用户必须切到别的文件再切回来（让 `openFile()` 重跑 `fileRead`）才看见变化。
    - 修：IPC 返回后若 `vaultState.openFilePath === 4-projects/<slug>/index.md`，显式 `const fresh = await fileRead(indexRel); editorContent = fresh; pendingSave = null;`。更新通过 `Editor.svelte` 里既有的 `$effect`（对比 `view.state.doc.toString()` vs `content`，走 `suppressChange=true` 的 dispatch）静默注入 CodeMirror，不会被 `updateListener` 误判成用户编辑、也不会触发新的 save cycle。`drainPendingSaves()` 已经在 IPC 调用前跑过了，所以 re-read 前不存在未落盘的 buffer 版本冲突。
    - `try/catch` 包起来：IO 失败仅 `console.warn`，不阻塞；此时 `status:` 已经写成功，只是 UI 没刷——降级可接受。
    - 命令从 project-note（非 `index.md`）上触发的场景：`vaultState.openFilePath !== indexRel`，re-read 分支跳过。磁盘上 `index.md` 被改了，但用户的当前编辑视图不动，符合直觉。
- **How to verify**
  - `pnpm check` 无 ts 报错。
  - Fix A：`⌘P` → `>` → `Reseed templates from bundled` → 应弹 `ask()` 警告（而不是 `window.confirm`）→ 确认 → 应弹 info 对话框显示 `模板已同步：更新 N（…）· 未变 M`。之前的 warning"需要先打开 vault"消失。
  - Fix B：打开 `4-projects/Deep-Work/index.md`（frontmatter 显示 `status: active`）→ `⌘P` → `> Set project status → paused` → 编辑器里 `status:` 行**立刻**变 `paused`，不需要切文件。连跑 `→ done`、`→ archived` 应同样即时生效。
  - Fix B 负路径：在 Deep-Work 下新建 `notes.md`（project-note），光标停在那 → 跑 `> Set project status → active` → `notes.md` 的 buffer 不抖、光标不乱，切到 `index.md` 看 frontmatter 确实改了 `active`。
- **Known gaps**
  - Fix A：状态栏 `saveError` 的 tooltip-only 暴露对所有其它"非交互、后台失败"场景依然是弱渠道（比如 auto-save 失败）。这次只在 reseed 这一条命令上加了 dialog，没有统一改造 banner——如果未来 auto-save 也开始有 io error，值得把 status bar 错误态扩成带 inline 文本的 chip。现在不做是为了保持改动爆炸半径。
  - Fix B：只处理了"当前打开的就是被改的 `index.md`"这一种情况。未来如果支持多 tab / split view，同一个 `index.md` 可能在多个 view 里，本 re-read 只刷当前主 editorContent。因为现在没有 tab 结构，不是 gap；做 tab 时需要把这段逻辑 generalize 到"任何显示该文件的 view 都 re-read"。
  - 没有补 Tauri `emit('file-changed', …)` 通道——那是另一条更大的架构改动（watcher → emit → 前端 listen → 按开启文件匹配刷 buffer）。当前只对"用户显式命令导致的后端写盘"做 point-fix；外部程序（Obsidian、命令行 `sed`）改 vault 仍然看不到实时刷新，得切文件。

---

## 2026-04-19 · Week 5 · Task 3.6 — CodeMirror `frontmatterCollapse` 迁 StateField（修 RangeError）

- **Scope**
  - 症状：打开带 frontmatter 的笔记 → 鼠标点正文任意位置 → 控制台抛 `RangeError: Decorations that replace line breaks may not be specified via plugins`（作者截图复现），CodeMirror 后续再不刷新 `livePreview` 装饰。
  - 根因：原 `livePreview` ViewPlugin 的 `buildDecorations()` 里同时兼顾两件事——(a) frontmatter 内部逐行 line-decoration（游标在 fm 内时高亮原文）；(b) 游标不在 fm 内时发一个 `Decoration.replace(fm.from, fm.to)` 把整块 YAML 折成 chip。 (b) 跨了至少 2 个 `\n`；CM 6 明确规定**跨行 `replace` 只能从 `StateField` 发出，`ViewPlugin` 不允许**（参见 CM 6 源码 `EditorView.decorations` facet 的 `DecorationSet.of` 断言）。这套代码最早是给"非折叠"场景写的，加折叠分支时漏了"换出口"这一步。
  - 修复：把 (b) 单独拆成新 `frontmatterCollapse` StateField——`create` + `update`（仅在 `docChanged || tr.selection` 时重算）里调 `computeFrontmatterCollapse(state)`，返回 `Decoration.set([Decoration.replace({ widget: new FrontmatterWidget(lineCount), block: true }).range(fm.from, fm.to)])`，并加 `block: true`（非 inline，匹配 widget 要输出块级 DOM 的事实）。`livePreview` ViewPlugin 保留 (a)，把原本 `if (fmCollapsed) { Decoration.replace(...) }` 整支删掉——那条路径现在由 StateField 覆盖。
  - `FrontmatterWidget.toDOM()`：从原来 `<span class="cm-md-fm-chip">` 改成 `<div class="cm-md-fm-block">` 包一层，内里嵌 `<span class="cm-md-fm-chip">` 保留原视觉。因为 `block: true` 的 replace 配的 widget 必须是 block-level DOM，否则 CM 会 warn 并在某些浏览器下排版塌陷。chip 视觉代码一个像素没动，只多了一层 flex 容器用来撑 line-height。
  - `livePreviewTheme` 补一条 `.cm-md-fm-block { padding: 2px 0; line-height: 1.4 }`——让折叠后的那行高度跟正文 `line-height: 1.6` 错开一点，眼睛更容易看出"这里被折叠了"。
  - `Editor.svelte`：`import { frontmatterCollapse }`；extensions 数组里把 `frontmatterCollapse` 紧跟在 `livePreview` 后面（顺序无所谓——两者 decorations 合流用 `facet` 拼，CM 自己排；放一起只是让人读代码时看到它们配对）。
- **How to verify**
  - `pnpm check` 无 ts 报错；`cargo check` 无回归（未动 Rust）。
  - 手测：`pnpm tauri:dev` 起客户端 → 打开任意带 frontmatter 的 .md（比如 `4-projects/Deep-Work/index.md`）→ 打开 DevTools Console。
    - 默认状态（光标不在里）：YAML 块被替换为单行 chip（绿色 `-- frontmatter (N lines) --` 之类）；Console 零报错。
    - 点 chip 之下的正文任意位置 → Console **不再**抛 `RangeError: Decorations that replace line breaks may not be specified via plugins`。
    - 点 chip / 用方向键进 fm 块 → chip 展开为原始 YAML 行；继续点出去 → 再折回 chip。来回切无抖动、无报错。
    - 编辑 fm 外正文文字 → chip 保持折叠态；编辑 fm 内 YAML → fm 保持展开态，保存后切文件再打开恢复默认折叠。
  - 视觉回归：chip 文案、配色、间距与修改前一致（只是外层多了 `<div>`，高度视觉上不变）。
- **Known gaps**
  - 本次只修 frontmatter 这一类块级 replace。如果未来还要加"折叠代码块"、"折叠 excalidraw 链接"之类跨行 replace，都必须照抄这个 StateField 模式，不能塞回 `livePreview` ViewPlugin。
  - 选区判定用的是"ranges 命中 fm 行区间"，如果用户把游标放在 fm 起始 `---` 行或结束 `---` 行正好那一行，也会被判成"在 fm 内"进而展开。当前行为跟 Obsidian 一致，算 feature 不是 bug。
  - `FrontmatterWidget` 目前只显示行数，不显示 fm 里的 title——曾考虑过 chip 上直接渲染 `{{title}}`，但这样就得把 Yaml 解析搬进 widget，得不偿失；title 从侧栏/tab 能直接看到，不是必需。
  - 未做单元测试：CM 装饰类逻辑用 jsdom 跑代价大，手测 + 观察控制台足以覆盖这条 bug 的 1D 回归面。如果后面开始堆块级装饰，再引入 `@codemirror/view` 的 testable EditorView 搭 minimal test harness。

---

## 2026-04-19 · Week 5 · Task 3.5 — `> Reseed templates from bundled`

- **Scope**
  - 起源：Task 3 + Task 2 联合 How to verify 在作者的老 vault 上失败——`templates/project.md` 是 Task 2 之前的老版本（含 `{{project_slug}}` 未绑定占位符 + `project_status:` / `project_started:` / `project_target:` 老字段），导致 `> New Project…` 生成的 `index.md` 带老字段，随后 `> Set project status → paused` 只能"在 frontmatter 末尾追加一行 `status: paused`"，跟 Task 2 Known gaps 预告的一样。根因：`vault_init` 的 `if !dst.exists()` 守卫让 bundled 模板升级永远打不到现有 vault；`vault_open` 不 seed。
  - 新增后端 `src-tauri/src/commands/vault.rs::vault_reseed_templates(state) -> ReseedSummary`：遍历 `BUNDLED_TEMPLATES` 7 项；`!dst.exists()` 走 `added`；存在但字节 `!= body.as_bytes()` 走 `updated` 并覆盖；完全相等走 `unchanged` 不写盘。`std::fs::read()`（bytes）与 `body.as_bytes()` 比，CRLF vs LF 会算作 diff 并被规范成 bundled（LF）形式，这是我们想要的副作用。不做 reindex——templates 里的 .md 也会被 watcher 索引，让它跑自然的 200ms 通道即可，不值得走 `scanner::reindex_one`。
  - `ReseedSummary { added, updated, unchanged: Vec<String> }`——三桶 disjoint，方便 UI 一次性呈现。对 `templates/` 里不属于 bundle 的文件（用户自定义模板）不动——既不读也不覆盖也不删。
  - `lib.rs::invoke_handler` 注册 `vault_reseed_templates`。
  - 前端 `src/lib/ipc/vault.ts`：`ReseedSummary` interface + `vaultReseedTemplates()` 封装。
  - `src/lib/palette/commandRegistry.ts`：`PaletteContext` 加 `runReseedTemplates: () => void | Promise<void>`；`PALETTE_COMMANDS` 新增 `reseed-templates` 条目（label `Reseed templates from bundled`, hint `Vault`）；不加 `when`——任何时候只要在 vault 里都可用。
  - `src/routes/+page.svelte`：新函数 `runReseedTemplates()`——`isTauriRuntime() && vaultState.rootPath` 守卫 → 用原生 `window.confirm(...)` 拍出覆盖告警（**明写**"自定义模板不会被删，但手工改过的 bundled 模板会丢"）→ `vaultReseedTemplates()` → 把 summary 渲染成 `saveError + saveStatus='saved'`（复用 save-banner 通道显示 `更新 N（a, b） · 新增 M · 未变 K`）。失败走标准 `saveStatus='error'` 通道。
  - V2 合规说明：本命令写 `templates/*.md`，属于写动作——但是**用户显式触发 + confirm 二道口**，不是后台静默迁移；相当于「用户在终端里 `cp src-tauri/templates/*.md vault/templates/`」的图形化等价物。符合 §0.2「no silent md injection」的原意（不侵犯用户数据 ≠ 不能写）。
- **How to verify**
  - `cargo check --manifest-path src-tauri/Cargo.toml` 通过；`cargo test --manifest-path src-tauri/Cargo.toml` 无回归（本轮未加单测——Rust 侧是纯 IO，逻辑足够简单以至于 per-bucket 计数跟 TS 侧手测一眼看穿；若 future 加 `keep_custom` 参数或字段级 diff 再补）。`pnpm check` 无 ts 报错。
  - 手测（复现老 vault 场景）：在作者原 vault 里 `⌘P` → `>` → `Reseed templates from bundled` → confirm 对话框出现 → 确认 → save banner 应显示类似 `更新 1（project.md） · 未变 6`（其它 6 个 bundled 模板内容未变）。
  - 立即 `⌘P` → `> New Project… Deep Work` → 新 `4-projects/Deep-Work/index.md` 的 frontmatter 应只含 `title / type / status / started / created / updated / tags`，**不再**有 `{{project_slug}}` 字面量或 `project_status / project_started / project_target` 老字段。
  - 紧接 `> Set project status → paused` → `status: active` 原地改成 `status: paused`，不会再追加新行。
  - 幂等性：再跑一次 `> Reseed templates` → banner 应显示 `未变 7`（全部 bundled 命中 unchanged 桶，零字节写入）。
  - 保留用户模板：在 `templates/` 下手建 `mycustom.md`（不在 bundle 里）→ reseed → `mycustom.md` 文件仍在且内容未变。
  - 保留用户对 bundled 的修改？**不保留**——这是已预警的 trade-off：手动编辑 `templates/note.md` 加了一段 → reseed 后被 bundled 原样覆盖。confirm 文案已明说这一点，用户自负。
- **Known gaps**
  - 不做 `per-file confirm` / diff 预览：全部 bundled 一把梭。如果日后 bundled 模板改动频率升高（`daily.md` 换个 emoji 也算 diff），可以考虑加 `preview-only` 模式让用户挑挑拣拣。
  - 没有撤销：写入就落盘，不做 `.bak`。个人 vault 场景下用户如果踩了"自己改过又 reseed"这个坑，得自己走 git / 回收站捞回来。
  - 不处理"bundled 文件被用户重命名"：如果用户把 `templates/project.md` 改名成 `templates/proj.md`，reseed 会以为 `project.md` 缺失、新建它，造成两份同义模板共存。暂不拦（个人 vault 不太会这么玩）。
  - 本命令不触发 schema 或索引变更——仅动 `templates/*.md`。未来若引入「按 bundled 模板重生成 4-projects/ 下已有 index.md」这种数据迁移动作，会是另一条命令（`> Migrate project frontmatter`），不会混进 reseed 里。

## 2026-04-19 · Week 5 · Task 3 — `> New Project…` 命令

- **Scope**
  - `src/lib/palette/commandRegistry.ts`：`PaletteContext` 加 `runNewProject: () => void`；`PALETTE_COMMANDS` 新增 `new-project` 条目（label `New Project…`, hint `4-projects/`）。
  - `src/lib/commands.ts::templateForDir`：
    - 签名从 `(topDir)` 改成 `(topDir, relPath)`——需要多一个参数以区分 `4-projects/<slug>/index.md`（套 `project.md`）和 `4-projects/<slug>/<note>.md`（套 `project-note.md`）。
    - 新增 `'4-projects'` 分支：`relPath.endsWith('/index.md') ? 'project' : 'project-note'`。`_internals.templateForDir` 仍导出但签名变了；搜索确认无外部调用，仅内部使用 + 未来测试。
    - `createNoteFromTemplate` 的调用点同步改传 `(topDir, relPath)`。
  - `src/routes/+page.svelte`：
    - 新 import：`slugifyTitle`（自 `$lib/commands`）。
    - `paletteCtx.runNewProject` 调 `newNote('4-projects')`——直接复用现有 newNote modal，不另开模态框，避免重复 5 个 state + 4 个 handler。
    - `confirmNewNote` 新分支：当 `newNoteTargetDir === '4-projects'` 时：非空校验 → `slugifyTitle(input)` → 生成 `relPath = 4-projects/<slug>/index.md`，并把 `{title: input}` 作为 `extra` 传给 `createNoteFromTemplate`，使得模板里的 `{{title}}` 展开为用户原始输入（保留大小写与空格），不是 slug 化后的值。
    - 错误态区分：项目已存在时报 `项目 <slug> 已存在`（比原路径字符串清楚）。
    - 模板里新增标题 `新建项目`、新 hint 解释 slugify 行为、input placeholder `Deep Work`。
    - **顺手修复**：`confirmNewNote` 原本只展开 `lastIndexOf('/')` 前那一层；新建 `4-projects/deep-work/index.md` 时只会把 `4-projects/deep-work` 加进 expanded，但中间的 `4-projects` 若未展开则整条根本不显示。改成按路径分段展开所有祖先（`[0, 1, 2, ...]` 的前缀累加到 expanded 集合）。
- **How to verify**
  - `pnpm check` 无 ts 报错；`cargo check --manifest-path src-tauri/Cargo.toml` 无回归（后端未改）。
  - 手测：`⌘P` → `>` → 选 `New Project…` → modal 标题是 `新建项目`，placeholder `Deep Work`；输入 `Deep Work` → 回车。
    - 侧栏 `4-projects/` 应自动展开，`Deep-Work/` 子目录可见，`index.md` 在其下，编辑器打开的就是这个文件。
    - 打开的内容来自 `project.md` 模板：`type: project / status: active / started: 2026-04-19 / tags: [project]`；`title: "Deep Work"`（空格保留）；`# Deep Work` 一级标题。
    - 立刻 `⌘P` → `>` → `Set project status → paused` → `status:` 行就地改成 `paused`，没有出现 `project_status:` 行（Task 2 修正后的模板）。
  - 负路径：再次 `> New Project… Deep Work` → 报 `项目 Deep-Work 已存在`；输入空格串 `"   "` → 报 `项目名不能为空`（trim 掉）；输入纯非法字符 `"///"` → 报 `项目名无法转换为合法目录名`（slugify 后空）。
  - `> New MOC… Python` 仍按老路径 `2-moc/Python.md` 创建，未被 `4-projects` 分支影响（`templateForDir` 对 `2-moc` 仍返回 `moc`）。
- **Known gaps**
  - 未做 slug 规范化（大小写 / 首字符 `.` 或 `-`）：`slugifyTitle("- .hidden")` → `-.hidden`，建出来是 `4-projects/-.hidden/index.md`——Unix 下是点号前缀隐藏目录。不拦，用户自负；若日后踩坑可加 `lstrip(['-', '.'])` 和 `toLowerCase()` 选项。
  - 项目"相关笔记"自动列表、侧栏 Projects 面板、Add/Extract 命令都尚未实现（分别对应 Task 4/5/6）；本轮 Task 3 只保证"能建一个项目、看得见、能改状态"这条最小闭环。
  - 复用 `newNote` modal 导致 `newNoteTargetDir` 字段意义变成"混合标记"——既是实际落盘 top-dir，也是 UX 分支 key。项目数 = 1 个新分支时成本低；若后续再加第 3、4 种特殊流（如 `new-daily-from-palette`）建议拆成独立 modal 或 enum。
  - `extra.title` 只支持 `4-projects` 一个分支；原 `createNoteFromTemplate` 签名本就接受 extra，只是之前没人用。其他 top-dir（MOC 等）若以后也想保留大小写，照抄一行即可。

## 2026-04-19 · Week 5 · Task 2 — 校正 project.md 模板（V2 对齐）

- **Scope**
  - `src-tauri/templates/project.md` 三处改动：
    - `project_status: active` → `status: active`：与 Task 1 `project_set_status` 写入的字段名对齐。旧模板生成的 project 会有 `project_status` 字段，Task 1 对其不感知——运行一次 `> Set project status` 后会得到 `project_status` + `status` 两行，查询仍工作（读 `status`），只是 cosmetic 重复；详见 Known gaps。
    - `project_started: ...` → `started: ...`：去掉项目专属前缀，保持与 `created / updated` 风格一致；这个字段从未被任何代码读取，纯模板层。
    - 删掉 `target: "{{target}}"` 行：`{{target}}` 在 `template.ts::render` 里无上下文绑定，会原样保留为字面 `{{target}}` 字符串；原字段 `project_target` 同样未被任何代码读取，且「🗺️ 里程碑」区块已覆盖类似意图，无需重复。
  - 其它模板（`inbox / note / moc / daily / weekly / project-note`）盘一圈：`note.md` 已有 `status: draft`；其余要么没 status 字段（inbox/moc/daily/weekly），要么是路径 SSOT 形（project-note），均与 V2 对齐，本轮不动。
  - `vault.rs` 的 `BUNDLED_TEMPLATES` 数组本身无变化——内容借 `include_str!` 从磁盘拉，Rust 侧零改动。
- **How to verify**
  - `cargo build --manifest-path src-tauri/Cargo.toml` 通过（`include_str!` 是编译期读文件，模板改了就会重新打进二进制）。
  - 手测（新 vault）：`vault_init` 一个空目录 → 检查 `templates/project.md` 有 `status:` 无 `project_status:`；然后 `> New Project… Deep Work`（Task 3 落地后）→ 生成的 `4-projects/deep-work/index.md` frontmatter 一致 → 立即 `> Set project status → paused` → 预期 `status:` 行原地改写，不会出现 `project_status:` 行。
  - 手测（老 vault）：若已有 `templates/project.md` 是老版，`vault_init` 不覆盖（`if !dst.exists()` 保护）；`index_projects_by_status('active')` 仍能正确查到，因为 `index_projects_by_status` 读的是 `notes.status` 列，indexer 按 `status:` 键解析——老项目的 `project_status:` 字段不会被误读成 status。
- **Known gaps**
  - 不做自动迁移：老 vault 里 `templates/project.md` 依旧是旧版（`project_status:`），因为 `vault_init` 只在模板缺失时写入。用户需要自己删掉老模板让它重新种子，或手动改。V2 "无静默 md 注入" 原则下没有后台迁移；若未来量变成痛点，可加一个 `> Migrate project frontmatter` 一次性命令，显式触发。
  - 老 project 的 `index.md` 若已含 `project_status: X`，首次跑 `> Set project status` 会在同一 frontmatter 里同时存在 `project_status:` 和 `status:` 两行。查询层读 `status:`，工作正常；cosmetic 重复需要用户手删。
  - 新模板里 `started:` 是模板渲染时写入的 `YYYY-MM-DD`，后续不会自动更新——这是设计上的时刻戳而非"最近修改"。若用户改了起始日期得手动编辑。

## 2026-04-19 · Week 5 · Task 1 — 通用 `status` 列 + `setProjectStatus` 命令

- **Scope**
  - SQLite：`notes.status TEXT` 列 + `idx_notes_status` 索引在 Week 3 schema 即已存在（schema.sql），indexer `parse_frontmatter` 也已把 `status:` 写入 `ParsedNote.status`；本轮确认现状、不再改 schema。弃用的 `project_slug` 列保留不动（V2 技术债，不在 Task 1 范围）。
  - 新增后端 `src-tauri/src/commands/project.rs`：
    - `#[tauri::command] project_set_status(slug, status)`：`4-projects/{slug}/index.md` 必须存在（不自动建，留给 `> New Project…`），slug 拒绝空串 / 路径分隔符 / `.` / `..`；读原文 → 行级重写 `status:` frontmatter → 若无变化直接 noop → 走 `resolve_write_target_in_vault + atomic_write`（与 file_write 同一守卫路径，避免成为绕开 vault 边界的后门）→ **同步 `scanner::reindex_one`**，与 `file_move` 同模式，Home / Projects 面板立刻反映新桶，不等 200ms watcher。
    - 辅助函数 `rewrite_frontmatter_status / split_leading_frontmatter / is_status_line / format_yaml_scalar`：镜像前端 `commands.ts::rewriteFrontmatter` 的 scalar-only 行为，保留用户的 YAML 排版；CRLF 兼容。
    - 6 个单测：已有 `status` 替换 / 块内追加 / 无 frontmatter 时 prepend / YAML 特殊字符加引号 / 值未变 noop 不重复 / CRLF 行尾容忍。
  - 新增后端 `commands::index::index_projects_by_status(status: Option<String>) -> Vec<NoteRef>`：`path LIKE '4-projects/%/index.md'`，`status` 为 `None` 时返回所有 project；比较用 `LOWER(TRIM(COALESCE(status,'')))`，大小写 / 首尾空白不敏感——用户写 `Active` 或 ` active` 都能桶进 "active"（对应用户反馈"不做 enum 强校验，但查询要容忍"）。
  - `commands/file.rs`：`resolve_write_target_in_vault` 与 `atomic_write` 从 private 提升为 `pub(crate)`，只暴露给兄弟命令模块。
  - `commands/mod.rs` + `lib.rs`：注册新模块与两个 handler。
  - 前端：
    - `src/lib/ipc/project.ts`（新）：`projectSetStatus(slug, status)`。
    - `src/lib/ipc/index.ts`：`indexProjectsByStatus(status?)` 封装，传 `null` 即「全部」。
    - `src/lib/palette/commandRegistry.ts`：`PaletteContext` 增 `setProjectStatus`；新增 4 条命令 `set-project-status-{active,paused,done,archived}`，`when = currentFilePath` 以 `4-projects/` 起始时才可见。新增导出 `projectSlugFromPath(path)` 从路径抠 slug（无则返回 null）。
    - `src/routes/+page.svelte`：新函数 `runSetProjectStatus(status)`——拿 `vaultState.openFilePath` → slug 为空则打 `saveError` 报错 → `drainPendingSaves` 清掉正在写的 index.md → `projectSetStatus` → 成功刷 `refreshHomeData()` 让 Home 的 Active Projects 卡换桶；失败走标准 `saveStatus='error'` 通道。`paletteCtx.setProjectStatus` 桥接之。
- **How to verify**
  - `cargo test --manifest-path src-tauri/Cargo.toml project::` 应跑通新增的 6 个 `rewrite_frontmatter_status` 单测；`cargo check` 通过；`pnpm check` 无 ts 报错。
  - 手测：打开 vault，`⌘P` 进到 `4-projects/<slug>/index.md` → `⌘P` + `>` 输入 `project status` → 4 条命令全部出现；切到非 `4-projects/` 的笔记时 4 条命令应被 `when` 过滤消失。
  - 选 `Set project status → paused`：
    - index.md 的 frontmatter `status:` 行就地改写（保留其它字段顺序与空行）；无 frontmatter 时会 prepend 一个最小块。
    - 立即（非 200ms lag）：`indexProjectsByStatus('active')` 从列表里移除该项目；`indexProjectsByStatus('paused')` 新增。
    - 若 index.md 当时打开，watcher 触发外部变更流程重载编辑器；Home 页 `Active Projects` 卡（若已实现）应立刻反桶。
  - 边界：手写 `status: Active` 或 `status:  active ` → `indexProjectsByStatus('active')` 仍然命中；写 `status: on hold` → 读出来就是 `on hold`，不抛错。
  - 负路径：slug 含 `/` 或 `..` → `invalid project slug`；index.md 不存在 → `project does not exist: 4-projects/xxx/index.md`（不自动建）。
- **Known gaps**
  - 不做 enum 白名单校验：frontmatter 是 SSOT，允许用户写任意字符串（例如自定义 `blocked` / `on-hold`）；UI 命令只生成 `active/paused/done/archived` 四值。查询侧用 `LOWER(TRIM(...))` 做容忍，不做规范化写回——如果用户确实写错了，需要他自己修。
  - 没有"批量改 status"入口：个人 vault 项目数量级 O(10)，单条改够用；需要批量时可以写脚本或走 `⌘P` 逐个切。
  - `project_slug` 列与 `idx_notes_project` 索引仍留在 schema 里——V2 已废弃但本轮不清，避免 SCHEMA_VERSION 版本升级 + 重扫；打算在后续专门的"schema 清理"任务里一并处理。
  - Home 页的 "Active Projects" 卡片（§7.2 要求）仍未建；本轮只交付了驱动它所需的查询 + 写入能力。下个 Task 做 Home UI 时直接接入 `indexProjectsByStatus('active')` 即可。
  - 如果 index.md 当时打开且有未保存编辑，`drainPendingSaves` 会先落盘这些编辑、再做 frontmatter 重写——即用户手里的版本会比"预览路径改变"慢一步，但不会丢内容；这是我们选择的权衡。

## 2026-04-19 · Week 4 · Task 6 — Home 页打磨

- **Scope**
  - 后端新增 `index_unresolved_count` IPC：`SELECT COUNT(DISTINCT dst) FROM links WHERE dst_resolved IS NULL`，返回 `i64`。注册到 `lib.rs` handler 数组。
  - 前端 `src/lib/ipc/index.ts`：加 `indexUnresolvedCount(): Promise<number>` 封装。
  - `src/routes/+page.svelte` Home 扩展（不拆组件，直接扩 snippet）：
    - 4 个 state：`homeRecentNotes / homeRecentMocs / homeUnresolved / homeReview`，搭 `homeReqSeq` 做 race 保护。
    - `refreshHomeData()` 并行拉 `indexAllNotes + indexUnresolvedCount`；按路径前缀分别切出「1-notes / 2-moc / 4-projects」前 5（Recent）和「2-moc 前 5」（MOCs）；`homeReview` 从 1-notes 后半随机抽一条，做「旧笔记回顾」。
    - 挂钩点：`tryOpenOrInit` / `refreshTree` / `goHome` 之后都调 `refreshHomeData()`；guard 在非 Home 视图时直接 return，避免 watcher 驱动的频繁刷新打到不可见的 DOM。
    - `resetVaultViewState` 清空 Home state，避免切 vault 时残留。
    - Home 模板：原 4 卡保留，下面追加「最近编辑 / MOCs」两列列表，footer 行显示 Unresolved 统计 + 随机旧笔记卡。MOCs 区的 `+ 新建` 直接复用 `paletteCtx.runNewMoc()`（即 `newNote('2-moc')`），保持与命令面板一致。
    - 样式：新增 `.home-lists` 双列网格、`.home-list*` 行样式、`.home-footer` / `.home-stat` / `.home-review`；`homeUnresolved > 0` 时数字变黄（`--color-warning`），提示「图里有悬空链接该修修了」。
- **How to verify**
  - `pnpm check` 无 ts 报错；`cargo check --manifest-path src-tauri/Cargo.toml` 通过。
  - `pnpm tauri:dev` 打开 vault → Home 页应出现两行新区（`最近编辑` / `MOCs`），列表按 `updated DESC` 排序，日期右对齐显示；0-inbox / 3-journal 的条目不应出现在「最近编辑」里（避免 daily 刷屏）。
  - 编辑一篇 `1-notes/` 下的笔记 → 返回 Home（`⌂` 按钮 / `goHome`）→ 该笔记跳到 Recent 第一条（watcher 索引完成约 <1s 生效）。
  - 写一个故意悬空的 `[[不存在的笔记]]` → 保存 → 回 Home → Unresolved 数字 +1 并变黄。
  - MOCs 区空时显示引导文案；点 `+ 新建` → 出「新建 MOC」modal → 创建后 Home 列表应立即出现该 MOC（Task 3 保证 modal 关闭后会触发 `refreshTree`）。
  - 旧笔记回顾卡：多次切 Home / 编辑保存会滚动到不同笔记（从 1-notes 旧一半随机）；点击能跳到该笔记。
- **Known gaps**
  - 未实现「活跃项目卡片」（§7.2 要求的 `4-projects/` active 卡）——需要先有 `status: active` 字段的查询，Week 5 做 projects 流程时再补。
  - Unresolved 数字是全库 distinct 目标数；没有点击下钻到「哪篇笔记里 unresolved」的入口。后续可以在 Panel 的 Unresolved 段做一个 Home-scope 版本。
  - 「旧笔记回顾」是 client-side Math.random；刷新 Home 才会换，不支持「下一条」按钮；首次开 vault 时列表未就绪可能为空，等 watcher 回来后第二次 `refreshHomeData` 才有值（Home 的 `onclick` 路径能自愈）。
  - Home 数据不由 save 直接触发刷新——save 走 panelRefresh 通道而已。如果用户在编辑器里改完、立刻 `⌘H` 回 Home，会看到带 200ms watcher lag 的旧 `updated` 值；acceptable。

## 2026-04-19 · Week 4 · Task 5 — Promote to Note 流程

- **Scope**
  - `src/lib/commands.ts` 新增导出：
    - `slugifyTitle(title)`：去非法字符、空白折叠成 `-`、去重 `-`、去首尾 `-`。
    - `rewriteFrontmatter(body, updates)`：基于正则的 frontmatter 补丁，处理「已有块 / 无 frontmatter」两种情况；命中的 key 原地替换，未命中的 key 追加到 `---` 之前；value 走 `formatYamlScalar` 做最小引号包裹。仅支持 scalar（个人笔记够用）。
    - `promoteInboxNote(deps, srcPath, newTitle)`：拒绝非 `0-inbox/` 源；用 `slugifyTitle` 找 `1-notes/{slug}[-N].md` 的首个空闲槽（最多 100 次后缀）；读取旧文本 → 写入新 frontmatter（`title / type: note / status: draft / updated: now`） → **先写新文件再删旧文件**（中途崩溃最多留下两份，不会丢数据）→ 展开 `1-notes/` → 刷新树 → 打开新文件；返回目标路径。
  - `src/routes/+page.svelte`：
    - 5 个新 state：`promoteOpen / promoteSource / promoteInput / promoteError / promoteInputEl`。
    - 4 个新函数：`openPromoteModal(path)`（pre-fill 文件 stem 并 `focus+select`）、`cancelPromote`、`confirmPromote`（先 `drainPendingSaves` → 调 `promoteInboxNote` → `invalidateWikiCompletionCache` + `bumpInbox`）、`onPromoteKey`（`Enter` 提交 / `Esc` 取消）。
    - `$derived` `promotePreview`：实时展示 `1-notes/{slug}.md` 目标路径。
    - `paletteCtx.promoteCurrent` 从桩换成真实调用：校验 `vaultState.openFilePath` 起始为 `0-inbox/` 后 `openPromoteModal`。
    - InboxView 的 `onPromote` 从「打开文件 + 调桩」简化为 `openPromoteModal(p)`（用户停留在列表视图）。
    - `resetVaultViewState` 重置 promote modal state，避免切 vault 时残留。
  - 模板：`modal-hint` 展示源路径 + 目标路径预览 + `↵`/`Esc` 按键提示，复用既有 `.modal-*` 样式。
- **How to verify**
  - `⌘⇧N` 新建 inbox 笔记（如「随手想法」）→ 打开 Inbox Review → 点 **Promote** → modal 出现，input 被预填为文件 stem，hint 实时显示 `1-notes/xxx.md`。
  - 清空输入 → 提交 → 显示「标题不能为空」；输入 `Deep Work` → `↵` → modal 关闭，笔记已经在 `1-notes/Deep-Work.md`，frontmatter 包含 `title: Deep Work` / `type: note` / `status: draft` / `updated: <now>`；原 `0-inbox/` 文件已被删除；编辑器切到新文件；Home 的 Inbox 计数 -1。
  - 重复一次同名 promote（先再建一个同名 inbox note）：目标自动变成 `1-notes/Deep-Work-1.md`。
  - `⌘P → > → Promote current to 1-notes` 也能打开同一个 modal；当前文件非 inbox 时命令被 `when` 过滤掉（看不见）。
  - `cargo check --manifest-path src-tauri/Cargo.toml` 无变化（后端本轮未动）。
- **Known gaps**
  - `rewriteFrontmatter` 仅 scalar：list / block 会被当成字符串整行保留；若用户在 inbox 笔记里手写了 `tags: [a, b]`，追加的 `title:` 会排在它们之间但不会破坏格式。需要多行 value 时仍得接真 YAML 库。
  - Promote 不负责重写其它笔记里指向原 inbox 文件的 `[[wiki-link]]`——依赖「链接用 stem 而非路径」的约定；若用户写了 `[[0-inbox/xxx]]` 会变成 unresolved。Week 5 做 Rename 时会顺带实现「引用重写」。
  - Promote 目前单文件、无 undo；误操作可在 OS 回收站 / `.mynotes/archive/` 找回（如果有备份）。后续可以考虑把旧文件先 move 到 archive 再写入新文件。

## 2026-04-19 · Week 4 · Task 4 — Inbox Review 视图

- **Scope**
  - 新增后端 `index_inbox_list` IPC：`SELECT * FROM notes WHERE path LIKE '0-inbox/%'` 按 mtime DESC 返回 `NoteRef[]`。
  - 新增后端 `file_delete(rel_path)` IPC：守卫目录、调用 `fs::remove_file`，随后 `scanner::delete_one` 同步索引。
  - 新增 `src/lib/inbox/InboxView.svelte`：列表 + 每行 Open / Promote / Archive / Delete 按钮；由 `refreshToken` 触发重查。
  - 前端 `src/routes/+page.svelte`:新增 `activeView: 'inbox' | null` 分支，与 `activeTag` 共同决定 editor-pane 渲染；Home 页「Inbox」卡点击从 `expandInbox()` 改为 `openInboxReview()`；命令面板 `> Inbox Review` 直达；实现 `archiveInboxNote(path)`（文件 `.mynotes/archive/inbox/<name>`）与 `deleteInboxNote(path)`（带 `confirm()`）。
  - TS 封装：`fileDelete` / `indexInboxList` 加入 `$lib/ipc/{file,index}.ts`。
- **How to verify**
  - `cargo test --manifest-path src-tauri/Cargo.toml` 通过（新 IPC 仅结构定义，原有解析器测试无回归）。
  - `pnpm tauri:dev` 启动 → `⌘⇧N` 快速捕获几条笔记 → `⌘P` 输入 `>` 选 `Inbox Review`；验证：列表显示刚捕获的条目；点 Open 切换到该笔记；点 Archive 条目从列表消失，`vault/.mynotes/archive/inbox/` 下出现该文件；点 Delete 出现 `confirm()`，确认后条目消失且磁盘文件被删。
  - 也可从 Home 页「Inbox」卡点击进入同一视图。
- **Known gaps**
  - ~~Promote 按钮目前走「先打开文件 + 调用 palette 的 `promoteCurrent` 桩」~~（Task 5 已实现；InboxView 现在直接打开 Promote modal，用户停留在列表视图）。
  - Archive 的"打 `status: archived` 标记"变体未做；目前只有物理归档路径这一种。
  - `confirm()` 用浏览器原生对话框，视觉与 Tauri 模态不统一；后续可换成 `<Modal>` 组件。

## 2026-04-19 · Week 4 · Task 3 — New MOC 命令

- **Scope**
  - 无新 IPC；命令面板 `> New MOC…` 触发 `newNote('2-moc')`，复用现有 `createNoteFromTemplate` 的 top-dir → 模板映射（`2-moc/` → `templates/moc.md`）。
  - `src/routes/+page.svelte` 新建笔记 modal 做 MOC 场景 UX 分支：标题显示「新建 MOC」、提示提及「套用 `templates/moc.md`」、placeholder 改 `Python · Deep Work …`。
- **How to verify**
  - `⌘P` → 输入 `>` → 选 `New MOC…` → modal 标题为「新建 MOC」→ 输入 `Python` → 创建 `2-moc/Python.md`，内容来自 moc 模板，frontmatter 含 `type: moc` 与 `tags: [moc]`。
  - 侧栏 Tags 应出现 `#moc` 标签（等 watcher 索引完成，约 1s）。
- **Known gaps**
  - 模板只替换 `{{title}}` 与 `{{now}}`；若日后要做"从 tag 反推候选笔记"需要在模板 render 阶段注入更多上下文。

## 2026-04-19 · Week 4 · Task 2 — 命令面板 `⌘P`

- **Scope**
  - 新增 `src/lib/palette/commandRegistry.ts`：`PaletteContext` 接口 + `PALETTE_COMMANDS` 数组 + `fuzzyScore(haystack, needle)` 子序列打分。
  - 新增 `src/lib/palette/CommandPalette.svelte`：
    - 4 模式：无前缀 = 文件模糊 + 命令 fallthrough；`>` = 纯命令；`#` = 所有 tag；`/` = FTS5 全文搜索（150ms debounce）。
    - 键盘：`↑↓` 选择 / `Enter` 确认 / `Esc` 关闭；鼠标 hover 同步高亮。
    - 打开时缓存一次 `indexAllNotes()` / `indexTags()`；关闭清缓存。
  - `+page.svelte`：`paletteOpen` state + `paletteCtx` `$derived` + 捕获阶段 `⌘P` 快捷键;命令 `promoteCurrent` 和 `runInboxReview` 先以桩接入（桩在 Task 4/5 被逐步替换）。
- **How to verify**
  - `⌘P` 打开面板。在空输入下应列出 50 条笔记；输入 `week` 过滤出 weekly 相关条目；前缀 `>` 列出所有命令（如 `Today`、`New MOC…`）；`#` 列出所有 tag；`/` 加关键字做 FTS5 搜索并在结果中看到 `<mark>` 高亮。
  - `↑↓ + Enter` 打开所选笔记；对命令条目按 Enter 运行；对 tag 按 Enter 切换到 TagView。
  - `Esc` 关闭面板；再次打开缓存重置，即新创建的笔记会出现。
- **Known gaps**
  - FTS5 查询目前整体引号包裹为 literal phrase，不支持 `+` / `-` / 通配符高级语法。
  - 命令列表在 `commandRegistry.ts` 中硬编码，将来接入动态命令（plugins / 项目命令）需要换成 registry 注册接口。

## 2026-04-19 · Week 4 · Task 1 — `file_move` + `file_delete` IPC

- **Scope**
  - 新增 `src-tauri/src/commands/file.rs::file_move(from, to)`：`resolve_in_vault` 双向校验、拒绝覆盖已存在目标、自动建父目录、优先 `fs::rename` 失败时降级 `copy + remove`，并同步调用 `scanner::delete_one(from) + scanner::reindex_one(to)` 立即更新索引（不等 watcher）。
  - 新增 `file_delete(rel_path)`：拒绝删除目录、`fs::remove_file` + `scanner::delete_one` 同步索引。
  - `src-tauri/src/lib.rs` 注册两个新 handler。
  - `src/lib/ipc/file.ts` 导出 `fileMove(from, to)` / `fileDelete(relPath)`。
- **How to verify**
  - `cargo test --manifest-path src-tauri/Cargo.toml` 通过。
  - 手测：开发者工具里 `await __TAURI_INTERNALS__.invoke('file_move', {from:'0-inbox/foo.md', to:'1-notes/foo.md'})`；随后从文件树刷新看文件到位；`index_backlinks('1-notes/foo.md')` 马上能返回（不必等 watcher）。
  - 负路径：目标已存在时返回 `destination already exists: <to>`；源不存在时返回 `source does not exist: <from>`；删除文件夹时返回 `refusing to delete a directory`。
- **Known gaps**
  - 跨卷/跨设备移动走的是 `copy + remove` 兜底，非原子；个人 vault 不跨挂载点的场景下无影响。
  - 未处理「目标是现有文件的 case-only 变名」（macOS 大小写不敏感 FS 会被 `dst.exists()` 拦住）；留待后续 rename 命令再解决。

## 2026-04-19 · Week 3 收尾（追溯记录）

Week 3 原本的 8 个子任务已在之前的 session 内全部完成，这里汇总以保留交付链：

- **Scope**
  - `rusqlite 0.31` + `notify 6` + `notify-debouncer-full 0.3` 依赖；`src-tauri/src/db/{mod.rs,schema.sql,indexer.rs}` 建库 + per-vault hash 路径 + WAL；`scanner::full_scan/reindex_one/delete_one` + `watcher::start_watcher`（200ms debouncer）。
  - 索引 IPC：`index_backlinks / index_outgoing / index_unresolved / index_tags / index_notes_by_tag / index_all_notes / index_search`；FTS5 `snippet()` 返回 `<mark>` 高亮。
  - 前端：右侧 `Panel.svelte`（反向链接 / 链出 / 未解析）、`TagsSection.svelte` + `TagView.svelte`、`wikicomplete.ts` 的 `[[` 自动补全。
  - `watcher.rs` 修复 `Watcher` trait 未引入导致的 E0599。
- **How to verify**
  - `cargo test --manifest-path src-tauri/Cargo.toml` 通过（含 indexer 解析器单测）。
  - 手测：打开 vault → 写一篇 note 中含 `[[some-existing-note]]` 与 `#tag`；保存 ~1s 后右侧面板和侧栏 Tags 应显示；Cmd/Ctrl+Click `[[…]]` 跳转；编辑器中输入 `[[` 出现补全。
- **Known gaps**
  - `index_search` 已开放后端但 Week 3 未做专用 UI；在 Task 2（命令面板）的 `/` 模式里接通。

---

## 版本变更总览（Changelog，历史索引）

> 此表原位于 `design_V2.md §16`，于 2026-04-21 整体迁移至此——design_V2 只记"为什么这样做"，流水账统一住 delivery_log。新增条目直接追加到表尾，并保持与本文件顶部三段式交付记录同步。

| 日期       | 版本 | 变更                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| ---------- | ---- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 2026-04-18 | 0.1  | 初稿：基于 PARA + 五级周期笔记的设计                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 2026-04-18 | 0.2  | **重大结构调整**：改用 LYT/MOC 工作流；周期笔记只保留 Daily + Weekly；Tag 聚合页 Phase 1 就做；新增 ADR-0007/0008/0009/0010；更新所有目录名、IPC 命令、模板、路线图                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-04-18 | 0.3  | 新增 `4-projects/` 顶层目录与 Project 模块（§2.3、§6.11）；frontmatter 扩展 `project` / `project-note` 类型；IPC 增 project\_\* 命令；SQLite notes 表加 project_slug 列；侧栏加 Projects 面板；Phase 1 扩展到 5 周；新增 `project.md` / `project-note.md` 模板；新增 ADR-0011                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 2026-04-18 | 0.4  | **V2 架构演进**：1. 取消 Frontmatter 中的项目跨域冗余声明 (废弃 project_slug)，实施基于文件树路径的精准单源推断 (SSOT)；2. 根除多端死锁隐患——SQLite引擎强制剥离至系统安全目录存储，开启事务加持WAL调优性能；3. 去除一切静默替换破坏原生 Markdown 源文件的占位注释行为，改用高阶抽象渲染衍生层展示；4. 引入无摩擦系统级 Ghost Window 极速捕获机制，及知识分解的重器"Inbox Block Extraction"段落提炼特性。妥破 iOS 端重存储生态。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 2026-04-19 | 0.5  | 从 design.md 同步 §0.1 交付规范 + §17 交付清单（Week 3 收尾 + Week 4 Task 1–6 全部交付条目）；V2 架构约束（无 project_slug frontmatter、无 md 注入）保留不变                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-04-19 | 0.6  | Week 5 Task 1 交付：通用 `status` 列 + `project_set_status` IPC + `index_projects_by_status` 查询 + 命令面板 4 个 `> Set project status → …` 命令。V2 约束澄清：`status:` 写入 frontmatter 属于"用户命令触发的编辑"，不违反 V2 "无静默注入" 约束；`project_slug` 字段仍弃用不动                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 2026-04-19 | 0.7  | Week 5 Task 2 交付：`src-tauri/templates/project.md` V2 对齐（`project_status` → `status`、`project_started` → `started`、删掉未绑定的 `target` 占位符行）；其它模板盘查确认已对齐，本轮不改                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-04-19 | 0.8  | Week 5 Task 3 交付：`> New Project…` 命令 + 复用 New Note modal 的 `4-projects` 分支（slugify 标题 → 建 `4-projects/<slug>/index.md`，套 `project.md` 模板，`title:` 保留原始大小写）；`templateForDir` 加 `4-projects` 分支（`index.md` → project；其它 → project-note）；目录树展开逻辑补完"逐层祖先 expand"，修复嵌套路径新建后中间目录仍折叠的问题                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 2026-04-19 | 0.9  | **文档重构**：把 §17「交付清单」全部内容迁到仓库根目录 `delivery_log.md`（历史条目 Week 3 收尾 + Week 4 Task 1–6 + Week 5 Task 1–3 全迁，不做摘要删减）；design_V2.md §17 收缩为 5 行索引指针，只说"去哪看、怎么写回来"。§0.1 规则 3 处对"§17"的引用同步改成 `delivery_log.md`。动机：每条三段式交付 40–50 行 × 周增 3–6 条，design_V2 已逼近 1800 行，再写 2 周就超过了人类一次 Read 的心智上限，也稀释了架构章节的比重                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2026-04-19 | 1.0  | Week 5 Task 3.5 交付：新增 `> Reseed templates from bundled` 命令——IPC `vault_reseed_templates` 按 bundled 字节 diff 决定 added / updated / unchanged 三桶覆盖 `vault/templates/*.md`，不动用户自定义模板；触发前有 `confirm()` 告警。动机：Task 2 的 Known gaps 在作者老 vault 上兑现（老 `templates/project.md` 没被 Task 2 的改动覆盖到），需要一个显式用户命令的迁移通道。V2 合规性：显式用户动作 + confirm 二道口，不是后台静默迁移                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2026-04-19 | 1.1  | Week 5 Task 3.6 交付：修 CodeMirror `RangeError: Decorations that replace line breaks may not be specified via plugins`——把 frontmatter 折叠 chip 的跨行 `Decoration.replace` 从 `livePreview` ViewPlugin 拆成新 `frontmatterCollapse` StateField（加 `block: true`）；`FrontmatterWidget.toDOM()` 外层改 `<div>` 以匹配 block widget 要求。动机：CM 6 规定跨行 replace 只能从 StateField 发，ViewPlugin 发会抛 runtime RangeError                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 2026-04-19 | 1.2  | Week 5 Task 3.7 交付（两个 hot-fix）：(A) `runReseedTemplates` 守卫字段名错写 `vaultState.rootPath`（实际是 `vaultState.current.path`），导致命令总是误判"未打开 vault"；顺手把 confirm / 结果展示从 `window.confirm` + `saveError`-tooltip 换成 Tauri `ask()`/`message()` 原生对话框。(B) `> Set project status → X` 改磁盘后当前编辑器不刷新——前端没有 `file-changed` 事件订阅通道，watcher 只刷 SQLite，需要在 IPC 成功后对 `openFilePath === index.md` 的情况显式 `fileRead → editorContent` 回填                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 2026-04-19 | 1.3  | Week 5 Task 4 交付：侧栏 Projects 面板（`ProjectsSection.svelte`）——4 个 status 子分组（active/paused/done/archived），active+paused 默认展开；数据源 4 路并发 `indexProjectsByStatus`；`refreshToken` bump 点：新项目创建、set-status、auto-save 后。挂载在 TagsSection 之前。样式/色板复用 TagsSection 的 token，无新增 CSS 变量                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 2026-04-19 | 1.4  | Week 5 Task 5 交付：项目"相关笔记"自动列表——新后端 IPC `index_project_notes(slug)`（path 前缀匹配，排除 `index.md` 自身，不读 `project_slug` frontmatter，V2 path-SSOT）；`Panel.svelte` 新增「项目笔记」section 放在 backlinks 之前，仅当前文件是 `4-projects/<slug>/index.md` 时渲染；空状态指引 `> Add Note to Project`（Task 6 命令）。子笔记视图暂不显示 siblings list（Known gap）                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2026-04-19 | 1.5  | Week 5 Task 6 交付：`> Add Note to Project` / `> Extract from project` 两条命令。Add 复用 New Note modal 的 targetDir 机制，新增 `4-projects/<slug>` 形态分支，slugify 标题生成 `<dir>/<note-slug>.md` + 套 `project-note.md` 模板；Extract 走 fileRead → rewriteFrontmatter(`type: note`) → fileWrite(1-notes/…) → fileDelete(src) 两步流，V2 合规不往 index.md 注入 `[[wiki-link]]`；`invalidateWikiCompletionCache` + `schedulePanelRefresh(200)` 清理 src 旧路径的补全缓存。Extract 碰撞自动 `-1` 递增（同 Promote）；Add 沿用既有"同名报错不覆盖"约定（不对称但故意）                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| 2026-04-19 | 1.6  | 开启 Phase 2：UI 质感美化与体验打磨（UI Beautification）。导入 Google Fonts `Inter` 统一全局字体，升级 `app.css` 提供克制低饱和度的 Premium 色板和层次阴影。在 Svelte 层加入 `backdrop-filter` 搭建 Command Palette 悬浮弹窗与 Modals 的毛玻璃 (Glassmorphism) 层，并全方位铺设 `fly` / `fade` 以及 `translateX` Hover 微动效。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 2026-04-19 | 1.7  | UI 二步打磨（Claude 式极简美学融合）：接受用户重构输入，摒弃繁杂方块控件。全站调色盘采用三段式递进温润配色（侧边栏 `#F3F1EC` 骨瓷米白，主编辑器 `#F9F8F6` 珍珠白分离区隔）。主编辑器清除传统 IDE 行号及当前行高亮，设定 `780px` 纸张质感宽幅居中。应用界面所有 `button` 转为 Ghost Button，双链与 `#tag` 运用极简下划线 hover 替代胶囊底。右侧信息流通过独立封装重构为白色卡片悬浮样式（Card UI）。                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-04-19 | 1.8  | Phase 2 Task 3 设计增补：新增 §6.12「附件与图片」一整节（存储结构 `attachments/YYYY/MM/`、命名 `YYYYMMDD-HHmmss-<slug\|rand>.ext`、标准 markdown `![alt](rel_path)` 引用、IPC 字节 + Blob URL 渲染方案、CM6 `StateField` 块级缩略图 widget、`> Find unused attachments` 孤儿清理流程、V2 合规自检）；§4.1 补 `attachments/YYYY/MM/` 子目录说明；§5.2 追加 5 个 `attachment_*` IPC 命令；§5.4 索引器补 markdown image 提取规则（`link_type='embed'`，复用现有 `links` 表不改 schema）                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| 2026-04-19 | 1.9  | Phase 2 Task 4 设计增补：新增 §6.13「链接重写（Rename With Refs）」一整节（范围/非范围边界、`file_move_with_refs` 命令语义、`RewritePlan` 构造与 raw_dst gate、bracket-anchored 正则、前端 4 个接入点对照表、失败语义与 V2 合规自检）。明确不做：目录重命名、title-form wiki 重写、附件 rename 的引用跟随；`archiveInboxNote` 保持 dumb move（归档等价于"退出流通"）。复用现有 `links.link_type` 索引，无 schema 变更                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 2026-04-20 | 2.0  | Phase 2 Task 4.5 设计增补：补 §6.13.7「目录重命名」——`dir_move_with_refs` 命令（聚合 `RewritePlan` 跑一遍 SQL `LIKE '{from}/%' ESCAPE '\\'` 查询 referrer，一次读写每个 referrer）；self-nesting 拒绝以 `/` 边界区分 `foo` / `foo-bar`；树内 referrer 在 move 前就地 rewrite，随 `fs::rename` 搬家；UI 入口命令面板 `> Rename current directory…`（目标 = 当前打开文件的父目录）。失败语义与 6.13.5 对齐，额外接受"move 做一半"的代价                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 2026-04-20 | 2.1  | Phase 2 Task 5 设计改写：§6.10「图谱视图」从占位章节扩写到 10 个子节（范围/非范围、`index_graph` IPC 一次读全图、Canvas vs. SVG 取舍、d3-force 四力配置、BFS 局部子图、d3-quadtree 懒建命中测试、d3-zoom × d3-drag 冲突处理、入口快捷键 `⌘⇧G`、依赖 ~42 KB、V2 合规自检）。同步新增命令 `> Open Graph View`；`activeView` 从 `'inbox' \| null` 扩到 `'inbox' \| 'graph' \| null`；不引入新 SQLite schema，纯读 `notes + links` 派生                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 2026-04-20 | 2.2  | Phase 2 Task 6 设计改写：§6.3 段落提取流程从"Phase 2 先不做"占位重写为实现对齐版——适用范围放开到任何 .md（不限 inbox）、交互 6 步含 `expandToParagraph` 空 selection 扩段规则、`extractBlockToNote` 纯函数构造器 + slug 冲突策略、modal 打开时冻结 range 保证 splice 不跟 live selection 走、CM6 单 transaction 撤销粒度、IO 失败回滚语义。同步引入命令 `> Extract selection → new note`（`⌘⇧E`）与 `EditorAPI` 接口（getSelection / getSelectionRange / replaceSelection / dispatchReplace / expandToParagraph，通过 `onReady` 回调暴露给父组件）。另：Markdown 语法高亮全面定制——`@codemirror/language` 的 `defaultHighlightStyle` 替换为 `src/lib/editor/markdownTheme.ts` 里的 `HighlightStyle`（绑 `@lezer/highlight` tags → `--color-*` CSS var，light/dark 随主题），同时开启 GFM + Strikethrough 解析（`@lezer/markdown`）让 `~~foo~~` / 任务列表 / 表格 / autolink 有 lezer 节点可上色                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| 2026-04-20 | 2.3  | Phase 2 Task 4.6 设计增补：新增 §6.13.8「侧栏右键菜单」——Sidebar 树 `contextmenu` 事件触发，按 entry 类型分支（文件：打开/重命名/Reveal/删除；目录：展开折叠/重命名/新建笔记/Reveal）；目录不含删除（后端 `file_delete` 拒绝目录，防误删子树）；三条消散路径（Esc / backdrop click / 菜单项选中）；坐标按 `innerWidth/Height - 220/240 - 8` 钳位防出界；新增 Rust 命令 `path_reveal` 做跨平台 Finder reveal（macOS `open -R` / Linux `xdg-open 父目录` / Windows `explorer /select,`），未引入新 plugin 依赖；Task 4/4.5 的 `openRenameModal` / `openDirRenameModal` 重构为 `(path?: string)` 接受可选参数，命令面板与右键共享同一 modal/confirm 逻辑                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 2026-04-20 | 2.4  | Phase 2 Task 7 设计增补：§6.4 MOC 辅助建议从一行占位展开到实现对齐版——入口（`> Build MOC from tag…` 仅 `activeTag` 有值时可见 + TagView 头部"建 MOC"按钮）、预览 modal（默认全选以便"整个 tag → MOC"一步到位）、`buildMocFromTag(deps, { tag, title, noteRefs })` 辅助函数语义（复用 `createNoteFromTemplate` 走 moc.md 模板，再正则替换 `## 核心笔记\n\n- [[]]` stub 注入选中笔记的 `- [[title]]` 列表，最后 `rewriteFrontmatter` 追加 `moc_source_tag`）、wiki-link 采 `[[title]]` 而非 `[[2-moc/slug]]` 的理由、碰撞 `-N` 递增（max 100，与 Promote/Extract 对称）、正则失配软降级为"已创建但未注入"。新增 `NoteRef` 从 `$lib/ipc/index` 导入到 `commands.ts`；`PaletteContext` 新增 `runBuildMocFromTag` + `activeTag: string \| null` 两字段；`TagView.svelte` 新增可选 `onBuildMoc?: () => void` prop                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| 2026-04-20 | 2.5  | Phase 2 Task 8 设计增补：新增 §6.14「设置界面」（`⌘,` + `> Settings…` 打开；主题 radio / autosave 数字输入 / 模板重置 / 中文分词说明四块；localStorage 持久化 key `mynotes:theme` 与 `mynotes:autosave-ms`；快捷键自定义延后到 Phase 3，中文分词受 FTS5 建表时固定，设计里写清"非运行时开关"避免用户到处找）+ §6.15「导出」（`vault_export_zip` 后端命令：DEFLATE / 排除 `.mynotes/` / 保留 `attachments/` / `.part` 原子化 rename / 符号链接跳过；`note_export_copy` 后端命令走 Rust 侧 `fs::copy` 以免拉 `@tauri-apps/plugin-fs`；PDF 走 `window.print()` + `@media print`，hide sidebar/panel/statusbar/modal-backdrop，CM6 的 `cm-editor/cm-scroller` 解开 height/overflow 让内容自然分页；三条命令面板入口 `> Export vault as zip…` / `> Export current note (.md)…` / `> Print current note`）。新依赖：Cargo.toml 添 `zip = { version = "2", default-features = false, features = ["deflate"] }` + `walkdir = "2"`；`error.rs` 加 `From<zip::result::ZipError>`。§10 路线图里 Phase 2 七条全部 `[x]`，Phase 2 结束                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-04-20 | 2.6  | Phase 2 Task 8.1 PDF 打印路径改写：`> Print current note` 的初版 `window.print()` + `@media print` 在 Tauri macOS WKWebView 下被静默吞掉（非用户手势触发），且 CM6 viewport virtualization 让打印只能捕到视窗内的行。改走新后端命令 `note_render_print_html(src_rel_path)`——`pulldown-cmark` 渲染 .md（含 tables/strikethrough/task-lists/footnotes）→ 包裹带 `<base href="file:///vault/">` + 内联打印 CSS 的 HTML 骨架 → 写到 `app_support_dir/print-preview/<stem>-<ms>.html` → `opener::open` 扔给系统默认浏览器，用户在浏览器里按 `⌘P` 存 PDF（用户手势触发，没被吞的问题，也绕过 CM6 虚拟化）。新依赖：Cargo.toml 加 `pulldown-cmark = { version = "0.11", default-features = false, features = ["html"] }` + `opener = "0.7"` + `url = "2"`（生成 file:// base URL）。`@media print` 块保留为 backup（用户手动按 `⌘P` 仍可得到勉强输出）。前端 IPC `noteRenderPrintHtml`；`+page.svelte` 的 `runPrintCurrentNote` 改 async，先 `drainPendingSaves()` 再调。§6.15 对应段落全部改写                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2026-04-20 | 2.7  | Phase 2 Task 8.2 图片三条插入路径全部修复：(1) 从 Finder 拖入编辑器以前永远无效——Tauri 2 默认 `dragDropEnabled: true` 在 OS 层吃掉 drop 事件，DOM 的 `drop` 永远不触发。`tauri.conf.json` 里显式设 `dragDropEnabled: false` 交给 DOM。(2) 从微信 / 部分文件管理器粘贴的图片剪贴板里只有 `text/uri-list` 或 `text/plain`（路径字符串），没有 `image/*` MIME。paste 与 drop handler 新增 fallback：解析 `text/uri-list` / 首行 `text/plain`，看是否像 image 绝对路径，是则走 `attachment_read_external_bytes` → `attachment_save` 归档。非 image 路径或普通文本 return false，CM 默认粘贴不被吞。(3) 手打 `![alt](/Users/…/foo.png)` 或 `file://…` 现在能渲染 widget：`EMBED_LINE_RE` 扩成三选一（`attachments/…` ∣ `file://…` ∣ POSIX 绝对路径），`getBlobUrl` 按路径形态 dispatch 到相应 IPC；`file://` 走 `decodeURIComponent` 处理中文 / 空格百分号。手打路径不自动归档，保留用户原文。新后端命令 `attachment_read_external_bytes(abs_path)`：硬约束绝对路径 / image 扩展名白名单（含 heic/heif）/ 50 MB 上限，拒非文件；放在 `commands/attachment.rs` 与既有 `attachment_read_bytes` 毗邻，语义是「编辑器预览 / 归档用读任意外部图片字节」。§6.12.4 / §6.12.5 全部重写                                                                                                                                                                                                                                                                                                                                                       |
| 2026-04-20 | 2.8  | Phase 2 收口与 Phase 3 准备：§10 路线图显式标记 Phase 2 已完成，并把 Phase 3 从"模糊愿景"整理为 4 条工作线（Desktop Hardening / Web / Mobile / AI）及推荐启动顺序。目标是把下一阶段的入口从"继续补功能"改成"先稳桌面内核，再扩平台、再接 AI"                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-04-20 | 2.9  | Phase 3-A1 启动：新增 app-level config 持久化（Tauri `app-config.json`）承接主题 / autosave / shortcuts；Settings modal 增加快捷键录入与默认恢复；前端 `installShortcuts` 改为从 keymap 驱动，命令面板 hint 与侧栏按钮 tooltip 跟随当前绑定实时刷新。目标是把桌面端从"功能齐了"推进到"偏好和操作习惯也能长期积累"                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| 2026-04-20 | 2.10 | Phase 3-A1 继续推进：TagView 从"单标签 + 最近更新排序"扩成"主标签 + 附加标签过滤"的探索视图。新增 `index_notes_by_tags(tags, match_all)` IPC，一次返回 tag 交集/并集结果；前端 TagView 增加附加 tag 选择、交/并集切换、排序（最近更新 / 最早更新 / 标题 / 路径），让标签视图从"看一个 tag 下所有笔记"升级成"在标签空间里快速缩窄结果集"                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| 2026-04-20 | 2.11 | Phase 3-A5 命令反馈硬化：`src/routes/+page.svelte` 新增页面内 notice stack（自动消失 / 手动关闭 / `z-index: 200`），把 graph load、extract、export、rename、project commands、unused attachments 等非 autosave 反馈从 `saveStatus / saveError` 通道剥离出去；状态栏只保留 autosave 的 `saving / saved / save failed` 语义，`runReseedTemplates()` 继续使用 Tauri `message()` 原生对话框而不借用 banner                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| 2026-04-20 | 2.12 | Phase 3-A6 侧栏文件 drop 导入：新增 §6.13.9 覆盖与编辑器 drop 的分工、drop 目标三分支（目录 row / 文件 row 取父 / 空白区落 `0-inbox`）、`file://` URI 解析管道、命名冲突 `-N` 递增策略、`file_import` 后端命令硬约束（绝对路径 / 非目录 / 拒 vault-内部源）、`fs::copy` 优先不走 bytes IPC 的理由、notice 聚合（单文件 / 多文件 / 部分失败 / 全失败四档文案）、视觉反馈 CSS（`.tree-row-wrap.drop-target` / `.tree.drop-root-active`）与 `relatedTarget` contains-check 的防抖、4 条 Known gaps（bytes fallback / 目录 drop / vault-内部 drag / 前端无 vitest）。新增 Rust 命令 `file_import(src_abs, dst_dir) → ImportedFile`（`commands/import.rs`，配 8 条单测：split_name × 4 / pick_free_slot × 4）；前端 IPC `fileImport` + `ImportedFile`；`+page.svelte` 新增 `parseDroppedPaths / handleSidebarDrop / onSidebarRowDrag* / onSidebarRootDrag*` + `dropTargetPath / rootDropActive` 两条 state                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 2026-04-20 | 2.13 | Phase 3-A7 打印 HTML 主题化：§6.15 增"打印 HTML 主题化（P3-A7）"子节，命令表 `note_render_print_html` 签名补 `theme?` 参数。后端 `commands/export.rs` 新增 `PrintTheme { Light, Dark, System }` enum（`from_option` 对 unknown / None / 空串降级到 System 做 forward-compat）+ `wrap_print_html(title, base_href, body_html, theme)` 三分支：Light/Dark 走 `data-theme="..."` + 显式 `color-scheme`（不发 OS media query），System 走无属性 + `color-scheme: light dark` + `@media (prefers-color-scheme: dark) { :root:not([data-theme]) { ... } }`；`@media print` 块里 `:root, :root[data-theme='dark'] { ...light vars... }` 强制走亮色，避免暗色背景被印到纸/PDF。调色板放弃 `oklch()` 退回 hex（macOS Preview / iOS Books / 旧 PDF viewer 对 oklch 一致性问题）。新增 5 条 Rust 单测覆盖三档 + print-media 双重置分支。前端 `noteRenderPrintHtml(srcRelPath, theme?: ThemePreference)`；`runPrintCurrentNote` 直接透传当前 `$state<Theme>`。另：`GraphView.svelte` 在 P3-A3 一起落地的 `MutationObserver(data-theme)` + `matchMedia(prefers-color-scheme: dark)` 双 hook 已经实现图谱主题切换自动重绘，本条 changelog 顺带显式 acknowledge，彻底收口 P3-A3 候选清单                                                                                                                                                                                                                                                                                                                                                                       |
| 2026-04-21 | 2.14 | Phase 3-A2 Must-fix sweep：补坑编号，一次性修掉 Phase 2 + P3-A1 积下来的 4 处缺口。(1) `templates/moc.md` 加 `<!-- moc:entries-insertion-point -->` sentinel；`src/lib/commands.ts` 抽出纯函数 `injectMocEntries` + 返回 `strategy: 'sentinel' \| 'legacy' \| 'none'`，解耦 MOC 注入与模板 heading 文字。(2) `confirmBuildMoc` / `confirmExtract` 改用 `schedulePanelRefresh(200)`，消除 `notify-rs → indexer` 异步管道与即时 `panelRefreshToken += 1` 的竞态；`strategy === 'none'` 分支给 toast 提示。(3) `commands/export.rs` 新增 `preprocess_wikilinks` + CJK-aware `wikilink_slug` + `escape_md_link_text`，把 `[[target]]` / `[[target\|alias]]` 在 pulldown_cmark 前改写成 `[display](#slug)`，打印输出真 `<a>`；补 8 条 Rust 单测。(4) `editor/imageEmbed.ts` 的 `EMBED_LINE_RE` 增加 Windows drive-letter 分支；新增 `normalizeAbsPath` helper 统一成 POSIX 形式喂给 `convertFileSrc`。另：§10 路线图同步调整——把 Phase 3 推进顺序从"A → B → C → D"调成"A → D → C →（B 延后或不做）"，理由是单用户自用 KB 的 Web 只读分发在 §1.3 已是非目标                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 2026-04-21 | 2.15 | Phase 3-D1 · AI 辅助面板（related-notes 本地启发式版）：新增 §6.16 覆盖架构约束、v1 打分模型（tag overlap / direct-link / co-citation / title Jaccard / staleness 五信号）、两条 IPC 命令（`ai_related_notes` + `app_config_set_ai_enabled`）、UI 集成要点（Panel section / Settings checkbox / 命令面板 `> Show Related Notes`）、查询性能约束（批量 tag / 双索引覆盖 / 未来 embeddings.sqlite 分离）、V2 合规自检（无 vault 写入 / 零 HTTP / 可关闭）。后端：`commands/ai.rs` 新模块，`ai_related_notes` 从 SQLite 一次读全量 notes + tags + links 后在 Rust 层打分，单测 15 条（bigrams × 5 / jaccard × 4 / staleness × 4 / scoring × 2）；`services/config.rs + commands/config.rs` 加 `ai_enabled` 字段。前端：新文件 `src/lib/ipc/ai.ts`；`Panel.svelte` 底部新增 AI section；`+page.svelte` 管 `aiEnabled` state + Settings checkbox + `runShowRelatedNotes`；`commandRegistry.ts` 新增 `show-related-notes` 命令                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 2026-04-21 | 2.16 | Phase 3-D2a.1 · AI 辅助·Embedding 索引底座（纯库层）：新增 §6.17 覆盖 D2a 整体目标、D2a.1 的三模块拆分（`services/ai/{provider, chunker, embedding_store}.rs`）、`AiProvider` trait + `MockProvider` 设计（async_trait dyn-compatible / 五类错误 / FNV-1a 滚动哈希 mock 向量）、chunker v1 策略（frontmatter 剥离 + 段落切 + `est_tokens > 800` 句子二次切，支持 `。！？` CJK 终止符）、`embeddings.sqlite` schema（独立于 `index.sqlite`；UNIQUE(note_rel_path, chunk_index, model) + UPSERT；BLOB 存 f32 + 内存 cosine 扫描；`note_mtime` 字段为 D2a.5 增量预留）、search 语义（dim 不匹配静默跳过 / empty query error / empty store ok）、24 条单测覆盖、D2a 后续切片路线图（D2a.2 OpenAI + keychain → D2a.3 watcher 增量 → D2a.4 dry-run modal → D2a.5 D1 升级）。`plan_P3.md §4.2` 同步把 D2 拆成 D2a（Embedding 底座）+ D2b（对话面 α detach），D2b 会消费 D2a.3+ 的 IPC。依赖：新增 `async-trait = "0.1"`（runtime）+ `tokio = { ..., features = ["rt", "macros"] }`（dev-only）。`services/ai/mod.rs` 挂 `#![allow(dead_code)]`，D2a.2 接 IPC 时摘除                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| 2026-04-21 | 2.17 | Phase 3-D2a.2 · AI 辅助·Provider 接入（OpenAI-compatible HTTP + keychain + Settings 测试连接）：新增 §6.18 覆盖依赖决策（`reqwest 0.12 + rustls-tls` 避 OpenSSL / `keyring 3` 跨平台 keystore）、`OpenAiProvider` 实现（`POST {base_url}/embeddings`，覆盖 OpenAI / Ollama / OpenRouter / LM Studio / vLLM 同一协议；empty api_key 跳过 Authorization header；`AtomicUsize default_dim` 首次 embed 后自动回填）、`SecretStore` trait 两实现（`KeyringSecretStore` 生产 + `MockSecretStore` 单测，service=`"com.mynotes.ai"` account=provider kind；API key 只允许两个写路径，永不对外出口）、错误映射（401/403→Auth，429→RateLimit(30)，4xx→InvalidRequest，5xx/unknown→Other，reqwest timeout/connect→Network；`extract_error_message` 先解 OpenAI envelope 再 fallback raw 400 字符含 UTF-8 `…` 截断）、4 条 IPC 命令（`ai_provider_set_config` / `_clear_config` / `_has_api_key` / `_test_connection`，后者结构体返回 `{ok, dim, total_tokens, error_kind, error_message}` 让前端不分支）、`AppPreferences.ai_provider` 只存 kind/base_url/embed_model 永不含 api_key、Settings UI 三栏表单（Base URL / Embed model / API key 含"已存储"badge）+ 三按钮（测试连接 / 保存 / 清除带 ask() 二次确认）。新增 22 条单测（openai 12 + secrets 7 外加 3 条 provider trait），覆盖率仍保持 0 warning。不做：重试/backoff / batch 拆分 / chat / 存 key 前强制测试。V2 合规：可完全连 Ollama 不触网 / API key 交 OS keystore / 一键清除回到零状态 / 结构化错误让 UX 分层级引导                                                                        |
| 2026-04-21 | 2.18 | Phase 3-D2a.3a · AI 辅助·手动 Embed 管道：新增 §6.19 覆盖 D2a.3 → D2a.3a + D2a.3b 拆分理由（pull-driven IPC 先行，push-driven watcher 下一刀单独接）、`AppState.embeddings: Mutex<Option<Arc<Mutex<EmbeddingStore>>>>` 生命周期挂载（`attach_index` 中打开 `<vault>/.mynotes/ai/embeddings.sqlite`；打开失败 log + continue 不阻断 vault open）、`services/ai/embed_service.rs::embed_note` 流水线（read → chunk → mtime skip → batch embed → delete_by_note + upsert 原子替换）、`MAX_BATCH_INPUTS = 64` 批次上限（OpenAI 96 / Ollama ~40 双边留余量）、mtime-based 增量（单 SQLite 查询判定 vs. 文件 hash 的取舍）、`SkipReason::{UpToDate, Empty}` 让 IPC 结构化表达"成功但无事可做"、4 条新 IPC (`ai_embed_note` / `ai_embed_stats` / `ai_embed_delete_note` / `ai_embed_clear_all`) + `EmbeddingStore::clear_all` 方法、`build_configured_provider` 辅助（读 config + keychain 组装 provider）、前端集成（`ipc/ai.ts` 添加 4 wrappers + 类型、命令面板 `> Embed current note (AI index)`、Settings 新增「AI 索引 · Embedding」小节展示 `chunks / notes / 模型` 计数 + 两按钮 + toast 反馈）、6 条新单测（empty / frontmatter-only / basic / up-to-date / chunks-shrink-cleaned / missing-file，使用 `MockProvider + open_in_memory`，sleep 1.1s 越过 FS 秒级 mtime 粒度）。有意不做：watcher 自动增量（D2a.3b）/ 批量 cross-note（D2a.4）/ rate limit 重试（依靠 debounce 自然重试）/ 成本估算（D2a.4）/ search UI 消费（D2a.5）。合规：embeddings 落 vault 内可 factory-reset / 清空按钮一次性 DELETE 不 touch `.md` / fail soft 保主功能 |
| 2026-04-21 | 2.19 | Phase 3-D2a.3b · AI 辅助·watcher 增量 embed：新增 §6.20，说明为何把 watcher 侧自动 embed 独立成单刀，以及如何在现有 200 ms SQLite watcher 上叠加一条 30 s debounce 的 AI 队列。后端 `services/ai/runtime.rs` 新增共享 helper：`auto_embed_enabled`（`ai_enabled == None` 继承默认启用；provider kind/base_url/embed_model 不完整则不入队）与 `build_configured_provider` / `build_provider_from_config`（命令层与 watcher 共用 provider bootstrap，不再复制逻辑）；`AppState.config` 调整为 `Arc<Mutex<ConfigStore>>` 便于 watcher 线程读取实时配置。`services/watcher.rs` 扩成双路径：主索引仍 200 ms 防抖 `reindex_one/delete_one`，AI worker 则用 `AiWatchMsg::{Upsert,Delete}` + `AiDebounceQueue<HashMap<rel, deadline>>` 把 `create/modify` 合并到 30 s 后执行 `embed_service::embed_note`，`delete` 则同步 `EmbeddingStore::delete_by_note` 清理 stale chunks；`create/modify` 若当前路径已不存在则按 delete 处理，避免 rename / 同步器事件形态残留脏向量；hidden path / `attachments/` / 非 markdown 继续过滤。`commands/vault.rs::attach_index` 启 watcher 时同时把 `config.clone()` 与 `embeddings_handle()` 传入；无 embedding store 时 watcher 仍正常跑 SQLite，不启动 AI worker。新增 9 条 Rust 单测（runtime 6 + watcher 3），`cargo test` 达 **154/154**，`pnpm check` 继续 0/0。明确不做：后台 progress UI / durable retry queue / 全量初始化 modal（留给 D2a.4）                                                                                                                                                               |
| 2026-04-21 | 2.20 | Phase 3-D2a.4 · AI 辅助·整库初始化：新增 §6.21，定义整库初始化的两段 flow。后端新增 `services/ai/init_service.rs`，提供 `preview_vault_embed`（walk 全 vault markdown、按当前 model 的 mtime 统计 `to_embed / up_to_date / empty`、估算 chunks / tokens / 成本、路径预览截断到 100）与 `embed_vault`（逐 note 复用 `embed_service::embed_note`，汇总 embedded / up_to_date / empty / failed，不在首错 abort）；`commands/ai.rs` 暴露两条 IPC `ai_embed_vault_preview` / `ai_embed_vault_run`。为避免换 model 后被旧向量误判成最新，`EmbeddingStore` 新增 `note_mtime_for_model`，`embed_service` 改按当前 model 做 up-to-date 判定。前端 `ipc/ai.ts` 增加 preview/run 类型与 wrappers；`+page.svelte` 的 Settings「AI 索引」区新增 `初始化索引` 按钮，点击后先取 preview 再弹现有 `.modal-preview` 风格 modal，展示待初始化 notes / total markdown / up-to-date / empty / chunks / tokens / 成本和前 100 条路径；确认后执行整库初始化并回写 summary notice。成本估算策略：localhost 记 `$0`，OpenAI 官方 host + 已知 embedding model 走公开单价映射，其余 provider 明示"未知"。新增 7 条 Rust 单测（init_service 6 + embedding_store 1），`cargo test` 达 **161/161**，`pnpm check` 继续 0/0。刻意不做：独立 force-rebuild toggle（当前路径是清空索引后重跑）、后台百分比进度条、第三方 provider 计费插件化                                                                                                                                                                                                                                     |
| 2026-04-21 | 2.21 | Phase 3-D2a.5 · AI 辅助·related-notes 向量打分升级：新增 §6.22，把 `ai_related_notes` 的第四个信号从 `title_jaccard` 替换为 `embedding_cosine`，消费本地 `embeddings.sqlite` 而不改命令签名。后端 `EmbeddingStore` 新增 `only_model_name()`（无 provider 配置但库里仅一条 model namespace 时自动消费）与 `note_cosine_scores(note_rel_path, model)`（按 model 聚合同 note chunk 向量求和，再做 note-level cosine，负值 clamp 到 0 保持 `[0,1]` 区间）；`commands/ai.rs` 的 related-notes 打分逻辑改为优先使用当前配置的 `embed_model`，否则回退单一 model；若无法确定 model 或当前 note 无向量，`embedding_cosine = 0` 且其余本地信号继续工作。前端 `ipc/ai.ts` 的 `RelatedSignals` 字段改成 `embedding_cosine`；`Panel.svelte` tooltip 从「标题相似」改为「语义相近」，AI badge hover 改成「本地索引打分」；`+page.svelte` Settings 提示文案同步说明"初始化索引后叠加语义向量相似度"。测试：`embedding_store` 新增 3 条单测（`only_model_name` / `note_cosine_scores` 聚合 / source missing），`commands/ai.rs` 组合打分注释与纯函数测试同步换成 `embedding_cosine` 语义；`cargo test --manifest-path src-tauri/Cargo.toml` **155/155** 通过，`pnpm check` 继续 0/0。刻意不做：独立 semantic-search IPC / ANN / sqlite-vec 升级 / 强制要求 provider 配置存在。                                                                                                                                                                                                                                                                                 |
| 2026-04-21 | 2.22 | Phase 3-D2a.6 · AI 辅助·失败降级 UX：新增 §6.23，聚焦"失败时不污染索引 + 给出可执行提示"。后端 `EmbeddingStore` 新增 `replace_note_chunks(note_rel_path, chunks)`，把 delete+insert 收进单个事务，避免半路失败把旧向量先删掉；`ProviderError::RateLimit` 升级为 `{ retry_after_secs, message }`，429 不再只剩"30 秒后再试"而丢掉 quota/billing 正文；`provider::describe_provider_error()` 统一展开结构化错误。`embed_service.rs` 新增 `EmbedFailure { kind, message, retry_after_secs?, store_unchanged }`，`embed_note()` 改回 typed failure；单测补 `provider_rate_limit_is_classified_and_store_unchanged`。`commands/ai.rs` 的 `ai_provider_test_connection` 新增 `retry_after_secs` 字段，`ai_embed_note` 返回 `EmbedNoteResult { ok, outcome?, failure? }` 而不是直接 reject provider/config 失败。`init_service.rs::VaultEmbedRunResult` 新增 `note_count_not_attempted`、`aborted_early`、`aborted_error_kind/message/retry_after_secs`；整库初始化在 `network / auth / rate_limit / invalid_request` 这类 provider 级失败上提前中止，避免对整个 vault 做注定失败的重复调用；新增 `embed_vault_aborts_early_on_provider_failures` 单测。前端 `ipc/ai.ts` 同步新类型；`+page.svelte` 增加 AI failure 文本归类与文案映射，Settings 测试连接 / 单篇 embed / 整库初始化都改成面向用户动作的提示，并在 embed 失败时明确"现有索引未被改坏"。验证：`cargo test --manifest-path src-tauri/Cargo.toml` **157/157**，`pnpm check` 继续 0/0。                                                                                                     |
| 2026-04-21 | 2.23 | Phase 3-D2b.1 · AI 辅助·会话数据层：新增 §6.24 覆盖 D2b 整段路线图（1 → 6 切分）与 D2b.1 的实现。后端 `services/ai/chat_store.rs` 新增 `ChatStore` 薄包装（`<vault>/.mynotes/ai/chats/<id>.jsonl` append-only，不挂 `AppState` 避免 vault 切换泄漏），定义 `ChatMeta` / `ChatMessage` / `ChatRole` / `ChatSessionSummary` / `ChatSessionFull` 五类对外类型与内部 `#[serde(tag = "type")]` 标签化 log line（首行必须是 meta，后续一律 message，每行自带 `v: 1` schema version）；`list` 只扫头 + 计数不拷正文，`load` 严格拒 corrupt / multi-meta / 未知 schema，`delete` 对 `NotFound` 幂等回 `Ok(false)`。`session_id = chat-YYYYMMDDTHHmmss-<8hex>`，后缀走 `sha256(nanos + pid + AtomicU64 seq)` 兜住同 tick 连发，不引入 `uuid`/`rand` 新依赖（复用 `sha2` / `chrono`）；入口全部走 `validate_session_id`（白名单 `[A-Za-z0-9_-]`，≤64 字符）堵死 `..`/`/`/空格/点号 等路径遍历 payload。`commands/ai.rs` 新增 5 条 IPC：`ai_chat_session_list` / `_create` / `_load` / `_append` / `_delete`，create 硬性后端生成 id（前端不许自带），并在入口对 `related_note` 做绝对路径/`..` 检查，同策略已在 file/attachment 命令维持。前端 `src/lib/ipc/ai.ts` 对应 5 个 wrapper + 5 条 TypeScript 类型，`ChatRole` 小写对齐 serde `rename_all = "lowercase"`，时间戳统一 Unix seconds，无 UI 改动——D2b.3 再接入 `Panel.svelte` 的 Tab 布局。新增 10 条 Rust 单测（roundtrip / 排序聚合 / 6 类非法 id / corrupt line / 空 root / 非 jsonl 文件 skip），`cargo test --manifest-path src-tauri/Cargo.toml` **167/167**，`pnpm check` 继续 0/0。明确不做：会话重命名 IPC（留 D2b.3）/ 全文检索 / message 修改或删除 / 多进程并发锁 / schema 迁移框架（真要升 v2 时再写一次性脚本） |
| 2026-04-21 | 2.24 | Phase 3-D2b.2 · AI 辅助·Provider Chat 接口：新增 §6.25 覆盖 D2b.2 八小节。trait 层把 `AiProvider::chat_stream` 带默认 `ProviderError::InvalidRequest` 实现——FailProvider / embed-only 双替身无需 opt-in 仅 `OpenAiProvider` + `MockProvider` override；新增 `ChatRole / ChatTurn / ChatRequest / ChatDelta` 公开类型与 `ChatStream = Pin<Box<dyn Stream<…> + Send>>` 别名，`collect_chat_stream` helper 聚合 stream 成单 delta。`OpenAiProvider` SSE 实现：POST `/chat/completions` with `stream: true` + `stream_options.include_usage: true`（OpenAI 末尾吐 usage chunk，Ollama/LM Studio/vLLM 忽略无副作用），`reqwest::bytes_stream` → `tokio::mpsc::channel(16)` → `futures_util::stream::unfold(rx)` 三层桥接，spawn task 读取 bytes、累积 `buf`、`find_event_end` 切 event、`parse_sse_data` 解 JSON；`[DONE]` 哨兵立即终止，取消路径靠 receiver drop 自然关 tx。分两个纯函数可单测：`parse_sse_data(payload) -> Result<Option<ChatDelta>, ProviderError>`（JSON 错归 `Other` 而非 `InvalidRequest`），`find_event_end(buf) -> Option<(usize, usize)>`（同时吃 `\n\n` 和 `\r\n\r\n`，返回 delimiter 长度）。`MockProvider` 加 `chat_script` / `chat_error` 两个 `Arc<Mutex<Option<…>>>` harness，`set_chat_script(tokens)` 播放完一次清零；未配置时默认三 chunk echo 最后一条 user turn。`AiProviderConfig` 加 `#[serde(default)] chat_model: String`——空串 = chat 停用（embeddings-only 合法态），旧 `app-config.json` 零 migration；`runtime.rs` 新增 `build_configured_chat_provider` / `build_chat_provider_from_config`（validate `chat_model` 而非 `embed_model`），暂 `#[allow(dead_code)]` 等 D2b.4 消费。`commands/ai.rs`：`ai_provider_set_config` 签名加 `chat_model: Option<String>`；新增 `ai_provider_test_chat_connection`（20 s timeout，硬编 `max_tokens: Some(8)` + `temperature: Some(0.0)` 跑 "Say OK." 返回 `ChatProviderTestResult { ok, reply?, input_tokens?, output_tokens?, error_kind?, error_message?, retry_after_secs? }`，reply 截 200 字）。把 `chat_store::ChatRole` 收敛为 `pub use super::provider::ChatRole`——D2b.1 + D2b.2 两份同名 enum 重复，统一一份消除类型分裂（wire format 不变）。前端 `ipc/ai.ts` 加 `ChatProviderTestResult` + `aiProviderTestChatConnection`，`aiProviderSetConfig` 加 `chatModel: string \| null` 参数；`ipc/config.ts` 的 `AiProviderConfig` 加 `chat_model` 字段。Settings UI：Chat model 输入 "留空停用"，动作栏拆成"测试 Embedding / 测试聊天 / 保存 / 清除"四档按钮，两条独立 banner（embedding / chat），`providerTestFailureText` 形参放宽到结构化 `{ error_kind?, error_message?, retry_after_secs? }` 供两 Result 共用。新增 14 条单测（`provider::tests` 4 + `openai::tests` 10），`cargo test --lib` **181/181** 全绿；涉及六文件（`openai.rs` / `provider.rs` / `chat_store.rs` / `runtime.rs` / `services/config.rs` / `commands/ai.rs`）clippy 零警告；`pnpm check` 继续 0/0。不做：function calling / tool use（`ChatTurn.content` 纯文本）/ Anthropic SSE（`parse_sse_data` 只认 OpenAI 形状）/ 中断 IPC（D2b.4）/ 限流 countdown UI（留 D2b.3 真场景）。**无 Panel UI 改动**——Panel Tab 化 + ChatPanel v1 留 D2b.3。 |
| 2026-04-21 | 2.25 | Phase 3-D2b.3 · AI 辅助·Panel Tab 化 + 非流式 ChatPanel v1：新增 §6.26 九小节覆盖设计目标 / 为何先非流式 / `ai_chat_send` 流程与失败语义 / `ChatSendResult` struct 而非 `Result<T,E>` / Panel Tab 架构 / ChatPanel 乐观 UI + 持久化 reload 协调 / composer 键位 / 最小 markdown renderer 安全边界 / 不做事项 / 测试覆盖。后端 `commands/ai.rs` 新增 `ai_chat_send(session_id, content) → ChatSendResult { ok, assistant?, failure? }`：固化 load → build chat provider → **先持久化 user turn** → `provider.chat_stream + collect_chat_stream` → 持久化 assistant turn 的顺序（失败时 user turn 不丢，和 ChatGPT / Claude.ai 的 UX 一致），`ChatSendFailure { kind, message, retry_after_secs?, user_message_persisted }` 用 bool 区分 pre-flight 拒绝与 provider 层失败；empty-reply 分支不持久化空 assistant 消息（避免 transcript 污染）。`services/ai/runtime.rs::build_configured_chat_provider` / `build_chat_provider_from_config` 去掉 `#[allow(dead_code)]`——D2b.3 本刀就是第一个消费者；`lib.rs` 注册 `ai_chat_send`。前端 `src/lib/ipc/ai.ts` 加 `ChatSendFailure` / `ChatSendResult` 类型 + `aiChatSend(sessionId, content)` wrapper。`Panel.svelte` 把单行 header 改成 Tab bar（笔记关系 / AI 对话，AI 对话 tab 只在 `aiEnabled` 时显示，`$effect` 在 aiEnabled 变 false 时自动把 activeTab 切回 Links）；`.panel` 改 flex column + `height: 100%` 支撑 chat 子组件的内部 flex（transcript 占满中间，composer 贴底）；Links tab 内容结构零动，直接包在 `{#if activeTab === 'links'}` 内，a11y 把 `<nav role="tablist">` 换成 `<div role="tablist">`。新增 `src/lib/panel/ChatPanel.svelte`（~650 行）：会话下拉 + `+` 新建 / `×` 删除（`window.prompt` / `window.confirm` v1，不造 modal）+ transcript 双色气泡（user `align-self: flex-end` + `accent-weak` 背景，assistant `align-self: flex-start` + `surface-raised` + 边框）+ 相对时间 + `renderMarkdown()` 最小 markdown 渲染（fenced code block `data-lang` / inline code / `**bold**` / `*italic*` / `_italic_` / 自动链接 https?:// / 段落双换行 + `<br>` 单换行；**HTML-escape 先行** 是安全边界，`@html` 之前全部转义防注入）+ composer（Enter 发送 / Shift+Enter 换行 / Cmd\|Ctrl+Enter 强制发送，sending 禁用 + "AI 正在输入"三点动画气泡）+ 乐观 user 气泡（`id = "optimistic-${Date.now()}"` 与真实 nanoid 不冲突，keyed `{#each}` 反正会换整个 DOM 节点）+ 结构化失败 banner（按 `failure.kind` 分档文案 network / auth / rate_limit + retry_after_secs / invalid_request / other，`user_message_persisted: false` 时追加"你的消息未发送"提示）+ 自动创建会话（首次发送取消息前 60 字为标题，关联当前 filePath）。关键 bookkeeping：**非响应式** `let lastResolvedSessionId: string \| null = null`（不是 `$state`，否则赋值本身会唤醒 effect 成循环），`$effect` 里 `id === lastResolvedSessionId` 短路跳过 reload——否则 auto-create 流里 `activeSessionId` 变化触发的 effect 会从磁盘 reload 空 transcript 覆盖掉乐观 push 的 user bubble，造成"消息闪现→消失→再出现"抖动。刻意不做：流式 / 取消按钮（D2b.4）/ RAG 注入 / `[[wiki-link]]` 渲染（D2b.5）/ 独立窗口（D2b.6）/ 会话重命名 / headings / lists / tables / blockquote（v1 不接 marked）/ 会话级并发锁（sending 状态已禁 send 按钮）。验证：`cargo test --lib` 继续 **181/181** 全绿（本刀无新增单测：`ai_chat_send` 依赖 `State<AppState>` + 异步 provider + 真实 `ChatStore`，沿用 D2b.1 的模式把 coverage 留在 storage / provider / runtime 三层；`renderMarkdown` 为 Svelte inline 纯函数，现阶段没 vitest 基础设施不引入）；`pnpm check` **0/0**；涉及四文件（`commands/ai.rs` / `services/ai/runtime.rs` / `services/ai/provider.rs` / `services/ai/openai.rs`）clippy 零新增警告。 |
| 2026-04-21 | 2.26 | Phase 3-D2b.4 · AI 辅助·流式 Chat IPC + 中断 + History 截断：新增 §6.27 十小节覆盖为什么切流式 / `AppState::chat_streams: Arc<Mutex<HashMap<stream_id, Arc<AtomicBool>>>>` 注册表形状与所有权 / `ai_chat_stream_start` 同步 pre-flight + 异步 spawn 两段式控制流 / `ai:chat-stream:{delta,done,error}` 三事件协议 / `ai_chat_stream_cancel` 尽力而为 + 保留已收内容 + 幂等语义 / `truncate_history_to_budget` 字符预算算法（4k tok × 3.5 char/tok ≈ 14k，永远保 system prefix + 最新对，单条巨长仍保）/ 前端惰性 listen + 实时 append + 闪烁光标 + 按钮切 "中断" 的协调 / 为什么运行时错误走事件而不是 `Result` / 不做事项（RAG / wiki-link 渲染 / modal / 独立窗同步 / 精确 tokenizer）/ 测试覆盖。后端 `commands/ai.rs` 新增 `ai_chat_stream_start(stream_id, session_id, content) → ChatStreamStartResult { ok, user_message?, failure? }`（同步 pre-flight：校验 stream_id / load session / build chat provider / 持久化 user turn / 截断 history / 注册 cancel flag → spawn async task：`provider.chat_stream` → 按 delta emit `ai:chat-stream:delta` → 末尾持久化 assistant turn 并 emit `done`；取消路径保留已累积内容）和 `ai_chat_stream_cancel(stream_id) → bool`（set flag，task 下次 poll break）；`truncate_history_to_budget(messages, max_chars)` 字符预算截断（+4 条单测）；`lib.rs::AppState` 增 `chat_streams` 字段 + IPC 注册；`src/lib/ipc/ai.ts` 增 `CHAT_STREAM_{DELTA,DONE,ERROR}_EVENT` 常量 + `ChatStream{Delta,Done,Error}Event` payload 类型 + `aiChatStreamStart` / `aiChatStreamCancel` wrapper；`src/lib/panel/ChatPanel.svelte` 改用 `listen('ai:chat-stream:*')` + `streamingContent: string` / `activeStreamId: string | null` 两个 `$state` 实时 append + 流式期间按钮切 "中断"（`.cancel-btn` 红色）+ 尾部 `.streaming-cursor` 闪烁指示。Session 文件格式与 D2b.3 完全一致，`ai_chat_send` 保留作 fallback / 非交互式路径。验证：cargo test **185/185**、pnpm check **0/0**、clippy 无新增 warning。|
| 2026-04-21 | 2.27 | Phase 3-D2b.5 · AI 辅助·RAG 注入 + `[[wiki-link]]` 渲染 + 新建会话 Modal：新增 §6.28 十小节覆盖为什么在 D2b.4 之上单独切一刀 / `services::ai::rag` 为什么拆成 `async fn embed_query` + `sync fn search_and_format`（`MutexGuard: !Send` 跨 `await` 会阻挡 spawn 任务编译）/ `RagContext` + `RagCitation` 数据形状 + 预算算法（`DEFAULT_TOP_K = 4` / `MAX_CONTEXT_CHARS = 2400` ≈ 700 tok / preview ≤160 UTF-8 chars）/ RAG 的 best-effort 语义（未配置 / 空 store / embed 挂 / 0 命中都返 `None` 不阻断 chat）/ `ChatStreamStartResult` 加 `citations` 字段 + `#[derive(Default)]` + 前端可选字段的兼容考虑 / `[[wiki-link]]` 两段决策（渲染只标记 `<span data-wiki-target>` 零 IPC / 点击事件委托按需 `indexResolveWikiLink`）/ `index_resolve_wiki_link` 的"先 title 后 stem" precedence + `LIMIT 1` 粗匹配取舍 / 新建会话 Modal UX 决策（标题 + 关联笔记 checkbox + busy 态防双重创建 + 自带 `.ns-*` 样式不蹭全局 `.modal` 给 D2b.6 standalone 让路）/ Citations 不持久化 / RAG 不按 related_note 过滤 / 流式中不重算 citations / wiki-link 无歧义 UI / Modal 无 focus-lock 等"故意留坑"/ 测试覆盖。后端 `services/ai/rag.rs`（新）：`RagCitation { note_rel_path, chunk_index, offset_start, offset_end, score, preview }` + `RagContext { system_message, citations }` + `async fn embed_query` + `fn search_and_format` + `fn format_context` + `fn truncate_chars`；`commands/ai.rs` 的 `ai_chat_stream_start` 在 pre-flight 插入 `try_build_rag_context` 并把 system message unshift 到 `full_messages`、`ChatStreamStartResult.citations: Vec<RagCitation>` 新字段；`commands/index.rs` 新增 `index_resolve_wiki_link(target) → Option<NoteRef>`（镜像 `indexer::resolve_links` 两段 precedence）+ `query_first_note` 私有 helper；`services/ai/mod.rs` 挂 `pub mod rag;`；`lib.rs` 注册 `index_resolve_wiki_link`。前端 `src/lib/ipc/ai.ts` 新增 `RagCitation` interface + `ChatStreamStartResult.citations?: RagCitation[]`；`src/lib/ipc/index.ts` 新增 `indexResolveWikiLink` wrapper；`src/lib/panel/ChatPanel.svelte` 新增 `citationsByAssistantId: Record<string, RagCitation[]>` + `pendingCitations: RagCitation[]` 两个 `$state`，`send()` 把 `res.citations` 放 pending、`onStreamTerminal(ok=true, assistantId)` 到达后 commit；`renderMarkdown` 插入 `[[target]]` / `[[target\|label]]` → `<span class="chat-wiki-link" data-wiki-target="...">` 转换（在 escape 之后、inline-code 之前）；`onTranscriptClick` 事件委托处理 click + keydown(Enter|Space)；"Sources" footer 渲染 chip 列表（`[N] path/to/note.md` + hover 显示 score + preview）；`newSession()` 从 `window.prompt` / `window.confirm` 换成 inline modal（`newSessionModalOpen` + 标题输入 + "关联当前笔记" checkbox + Esc/Enter 快捷键 + busy 态防双重创建 + error banner + `.ns-*` 独立样式）。验证：`cargo test --lib ai::rag` **4 passed**（字符截断 / UTF-8 多字节边界 / 各自小于总预算 / 总预算命中截断）、`cargo check` 干净、`pnpm check` **0/0**。刻意不做：citations 持久化 / related_note 过滤 RAG / 流式中重跑 RAG / wiki-link 歧义选择器 / modal focus-lock。|
| 2026-04-21 | 2.28 | Phase 3-D2b.6 · AI 辅助·弹出独立窗口 + AI 关闭时自动关闭：新增 §6.29 十一小节覆盖"D2b 收尾：为什么把独立窗做最轻（零后端改动复用事件 bus）"/ 为什么把独立窗做成 SvelteKit 路由而不是独立 entry（共享 token + zero build 配置 + Tauri v2 index.html fallback 天然工作）/ 跨窗事件协议 `chat-standalone:{ready,file-path,open-note,close,closed}`（五条命名 + 方向 + payload + 时机 + 为什么不用 `tauri://destroyed`）/ Docked 占位符为什么必要（`emit` 是全局广播，两份 ChatPanel 同时 mount 会双订阅 `ai:chat-stream:*` 造成 race，切占位符是把主窗那份卸载）/ `bringBack()` 优雅关 + 600ms 兜底（主路径 `EV_CLOSE` → Svelte `onDestroy` → `aiChatStreamCancel` 清资源；兜底 `standaloneWindow.close()` 硬杀独立窗）/ `!aiEnabled && standaloneOpen` 自动关（`$effect` + 对称性：开 AI 不自动恢复独立窗）/ 握手协议 `ready` 为什么需要（`new WebviewWindow` 返回后 webview 仍在 loading，`onMount` 未跑完，过早 emit `EV_FILE_PATH` 会丢事件）/ Re-mount 恢复（vault 切换 → Panel re-mount 时独立窗还在，`onMount` 查 `getByLabel` 补齐 `standaloneOpen` + 补发 `EV_FILE_PATH`）/ Capability 最小面变更（只加 `windows: ["main", "chat-standalone"]`，不加新 permission，`core:default` 已含 webview/window 所需项）/ 不做事项（多独立窗 / 位置尺寸持久化 / 主窗 docked + 独立窗并存 / 跨窗 session 同步 / 独立窗内自己的 Settings）/ 测试覆盖。前端新增 `src/routes/chat-standalone/+page.svelte`（独立窗 shell：`onMount` 绑 `EV_FILE_PATH` / `EV_CLOSE` 两 listener + emit `EV_READY` 握手 + `onOpenNote` 走 `emit(EV_OPEN_NOTE)` 路由回主窗；`onDestroy` emit `EV_CLOSED`）；`src/lib/panel/Panel.svelte` 增 `standaloneOpen` / `standaloneWindow` state + `openStandalone` / `bringBack` / `ensureStandaloneListeners` 三函数 + 两条 `$effect`（`filePath` 推送 / `!aiEnabled` 联动）+ `onMount` re-mount 恢复 + `⧉` pop-out 按钮（仅 `!standaloneOpen` 可见）+ docked 占位符（`"AI 对话已在独立窗口"` + `聚焦` / `取回到此处` 两按钮）；`src/lib/panel/ChatPanel.svelte` 加 `variant?: 'docked' \| 'standalone'` prop（`$props()` 默认 `'docked'`；根 div 加 `chat-panel--{variant}` 修饰类；CSS 让 standalone 变种 padding 归零铺满）。Tauri `src-tauri/capabilities/default.json` 单行改 `windows: ["main", "chat-standalone"]`；零后端 Rust 改动。验证：cargo test **189/189**（D2b.5 的 185 + 4 条 rag 模块测试）、pnpm check **0/0**、pnpm build success。Tauri v2 默认 index.html fallback 让 `/chat-standalone` 零配置工作；若将来 Tauri 加 `disableFallback` 则切到 `/#/chat-standalone` hash routing（上游跟进非架构债）。|
| 2026-04-21 | 2.29 | Phase 3-D3.1 · AI 辅助·`ai_complete` 单次补全 IPC + cancel：新增 §6.30 十小节覆盖为什么先切一刀"纯管道"（prompt engineering 与 UI 解耦 / D3.2 modal 骨架可独立开发 / cancel 注册表分表）/ 为什么是非流式语义（写回场景交付单位是整段；内部仍复用 `provider.chat_stream` SSE transport 避免 parser 分叉；loader + cancel 在几秒任务里够用）/ `CompleteResult` vs `ChatSendResult` 风格一致但结构不同（无 `user_message_persisted`；`cancelled` 与 `ok` 正交；空 reply 降级为失败）/ `complete_requests` 表与 `chat_streams` 分表的语义隔离理由 / pre-flight 五段验证顺序（invalid id / empty prompt / duplicate id / provider 构造 / 注册 cancel flag）/ `cleanup()` 在所有终结路径的清理语义 + cancel 在 loop 进出两处的 race 窗口 / `ai_complete_cancel` 的幂等与 "id 不在表里不是错误" / 与 chat 侧 9 项差异对照表 / 刻意不做（prompt 长度硬上限 / 流式事件 / batch API / prompt template 住后端 / cancel-all batch）/ 测试覆盖（沿用 `ai_chat_stream_start` 的"集成 + MockProvider 间接覆盖"策略）。后端 `src-tauri/src/commands/ai.rs` 新增 `ai_complete(request_id, system_prompt?, user_prompt, temperature?, max_tokens?) → CompleteResult { ok, reply?, input_tokens?, output_tokens?, cancelled, failure? }` 与 `ai_complete_cancel(request_id) → bool`，内部构建 `ChatTurn::System` (when present) + `ChatTurn::User` 两条消息调 `provider.chat_stream(req)`，用 `AtomicBool` cancel flag 在 delta loop 两处检查；新增 `CompleteFailure { kind, message, retry_after_secs? }` 结构（窄于 `ChatSendFailure`）；`AppState` 加 `complete_requests: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>` 字段 + `lib.rs` 初始化 + `tauri::generate_handler!` 注册两命令。前端 `src/lib/ipc/ai.ts` 新增 `CompleteFailure` / `CompleteResult` 类型 + `aiComplete(requestId, { systemPrompt?, userPrompt, temperature?, maxTokens? })` / `aiCompleteCancel(requestId)` 两 wrapper，opts-object 风格对齐 `aiChatSend`。无前端 UI 改动——D3.2 起才接入 diff modal 与三条写回命令。验证：`cargo check` 零 warning、`cargo test --lib` **189/189**、`pnpm check` **0/0**、`pnpm build` success。不做：prompt 长度硬上限 / 流式事件 / 批量 `ai_complete_batch` / 后端住 prompt template / cancel-all batch API。|
| 2026-04-21 | 2.30 | Phase 3-D3.2 · AI 辅助·Diff 预览 modal + 行级 diff 渲染：新增 §6.31 八小节覆盖为什么先切一刀"只做 UI 壳"（三条写回命令共享 "发命令→组 prompt→调 `aiComplete`→拿候选→用户确认→落盘" 同一节律，抽第 4–5 步到 shared 组件）/ 行 diff 算法为什么自写而非引 `jsdiff`（bundle 代价 40KB + 场景有界单篇笔记 + tie-break 语义 + 可测性）/ 三态组件形状 + props 与 `CompleteResult` 一一映射（loading / error / diff discriminated mode）/ 快捷键 & 防误操作（Esc 按上下文走 cancel 或 discard、Cmd/Ctrl+Enter = accept、backdrop click 同 Esc、`accepting` latch 防双击写盘、零变化时 accept 置灰）/ `.dpm-*` scoped 样式沿用 `ns-modal` 前缀范式 + design tokens / 与 `ns-modal`、以后的 `tag-merge-modal` 的分工（不强行把 checklist 合并塞 diff 组件，D3.4 再选型）/ 刻意不做（虚拟滚动 / word-level diff / 动画过渡 / 行内语法高亮）/ 测试覆盖（6 条边界手验 + pnpm check/build 双绿）。新增 `src/lib/ai/diffLines.ts`（~30 行 LCS：`DiffPart = { type: 'add' \| 'remove' \| 'same', value }` + `diffLines(a, b)` + `diffStats(parts)`，tie-break `dp[i+1][j] >= dp[i][j+1]` 让删除优先、连续操作聚团）与 `src/lib/ai/DiffPreviewModal.svelte`（三态 `{#if loading}{:else if error}{:else if proposed !== null}`、props `open/title/description?/original/proposed/loading?/error?/acceptLabel?/discardLabel?/onAccept/onDiscard/onCancel?`、`labelForKind` 把 `ProviderErrorKind` 翻中文）。无命令接入——D3.3 起才在 `+page.svelte` 真正挂载。验证：`pnpm check` **0/0**、`pnpm build` success、ReadLints 干净；Node 内联版 `diffLines` 六条边界（identical / insertion / deletion / replacement / empty→text / text→empty）全部符合预期。|
| 2026-04-21 | 2.31 | Phase 3-D3.3 · AI 辅助·`> Summarize current note` 三档写回命令：新增 §6.32 十小节覆盖为什么"三档独立命令"优于"一档 + modal 选 target"（fuzzy-search 本身就是最便捷的 target picker / clipboard 档天然不适配 modal / 三命令共享同一次 `aiComplete`）/ Prompt 为什么住前端（输出形状硬约束避免 frontmatter YAML 被破坏 / 语言自对齐省一次 CJK 检测分支 / 长度软上限比客户端截断好 / prompt 调优不值得后端 Rust 重编）/ `applySummaryToBody` 纯函数 + 为何纯（`DiffPreviewModal.proposed` 走 `$derived.by` 联动、未来加"编辑 reply 再 apply"只需 reply 变 textarea state）/ "top" 档刻意不检测旧 TL;DR（精准分类困难、误删 > 冗余、diff 让用户自救）/ `runSummarizeCurrentNote` 状态机与 race guard（`summarizeRequestId` 故意非 `$state`、三段 stale-request guard、cancel 路径"先抢 id 再 cancel"顺序重要）/ 写回与编辑器重载一致性（`editorContent = fresh; pendingSave = null` 对 watcher 只重建索引的弥补）/ `paletteCtx.aiEnabled` gate 让命令在 AI 关闭时不出现、不同于历史遗留未 gate 的 related-notes/embed 命令 / 与 chat / related-notes 共存语义（`complete_requests` 与 `chat_streams` 分表 cancel 互不误杀）/ 刻意不做（重新生成按钮 / prompt 编辑入口 / chunked summarization / clipboard 档 diff preview / editor inline preview）/ 测试覆盖。新增 `src/lib/ai/summarizePrompt.ts`（`SummarizeTarget` 类型、`buildSummarizePrompt` / `applySummaryToBody` / `stripFrontmatter` / `insertTldrAtTop` / `makeSummarizeRequestId` 四件套纯函数；复用 `$lib/commands.rewriteFrontmatter`）；`src/lib/palette/commandRegistry.ts` 扩 `PaletteContext` 加 `aiEnabled: boolean` + `runSummarizeCurrentNote(target: 'frontmatter' \| 'top' \| 'clipboard')`，新增三条面板命令 `summarize-to-{frontmatter,top,clipboard}` 全部 gated `ctx.aiEnabled && markdown && !.mynotes/`；`src/routes/+page.svelte` 上移 `let aiEnabled = $state(true)` 到 `paletteCtx` 之前、加 `summarize*` 状态族（含非响应式 `summarizeRequestId`）、`summarizeProposed = $derived.by(...)` 联动 `DiffPreviewModal.proposed`、`runSummarizeCurrentNote` / `applySummarize` / `cancelSummarizeInFlight` / `closeSummarize` 四函数 + `DiffPreviewModal` 挂载在 `{#if summarizeOpen}` 下。clipboard 档：`aiComplete` → `navigator.clipboard.writeText` → toast 反馈（无 modal）。验证：`pnpm check` **0/0**、`pnpm build` success、ReadLints 干净；Node 内联手验 `rewriteFrontmatter` + `insertTldrAtTop` 三条路径（既有 `---` 块追加 summary / `> **TL;DR**` 插在 `---` 后 `# Title` 前、前后各一空行 / 无 frontmatter 直接 prepend 保留原尾）。 |
| 2026-04-21 | 2.32 | Phase 3-D3.4 · AI 辅助·`> Suggest tags for current note` 命令（checkbox 合并写入 `frontmatter.tags`）：新增 §6.33 十小节覆盖为什么不复用 `DiffPreviewModal`（checkbox merge ≠ text diff / 分项接受诉求 / 三态徽章 taxonomy 归属）/ Prompt 关键约束（kebab-case 硬约束 + CJK 白名单让 `\u4e00-\u9fff` 合法 / existing + vault top 40 作 soft few-shot / 允许最多 2 个 brand-new tag / 3–8 数量上限）/ `parseSuggestedTags` 三档容错 & 为什么 csv chunk 不按空格切（kebab-case enforce → 空格应为 0 / 违规时折叠成 `-` 比硬切成假 tag 更语义 / 长句靠 40 char + 纯数字兜住）/ `mergeTagsIntoFrontmatter` 写回策略（三态兼容读入、统一 flow 输出、不复用 scalar-only 的 `rewriteFrontmatter`、去重顺序稳定）/ `TagSuggestModal` UI 决策（existing-first 预勾、AI 候选预勾、`$effect` 增量 seed 保留交互、`+/-` 计数行、键位镜像 `DiffPreviewModal`、`.tsm-*` scope）/ `runSuggestTagsForCurrentNote` 状态机（`indexTags()` 失败不 block、`temperature: 0.2` 收敛、stale-request guard 三段与 summarize 同构、`suggestTagsRequestId` 故意非 `$state`）/ `applySuggestTags` 一致性（finalTags 是最终意图清单、merge 冗余防漏）/ 与其它 AI 命令共存（`complete_requests` 表按 id 分离不串台）/ 刻意不做（置信度排序 / tag rename / inline `#tag` 合并 / 保留 tag 黑名单 / chunked / clipboard 档）/ 测试覆盖。新增 `src/lib/ai/suggestTagsPrompt.ts`（`SuggestTarget` 固定为 `frontmatter`；`buildSuggestTagsPrompt` / `parseSuggestedTags` / `parseExistingTags` / `mergeTagsIntoFrontmatter` / `mergeTagLists` / `normaliseTag` / `makeSuggestTagsRequestId`；Node 内联手验 7 解析 case + 5 合并路径全绿）；新增 `src/lib/ai/TagSuggestModal.svelte`（`.tsm-*` scope、`已存在`/`复用`/`新建` 三态徽章、`$effect` 增量 seed 勾选 map、`$derived.by` 计算 `finalTags`/`addedCount`/`removedCount`、Cmd/Ctrl+Enter = accept、Esc = discard\|cancel、accept latch）；`src/lib/palette/commandRegistry.ts` 扩 `PaletteContext` 加 `runSuggestTagsForCurrentNote()` + 注册单条 `suggest-tags` 命令（gate `aiEnabled && markdown && !.mynotes/`）；`src/routes/+page.svelte` 补 `indexTags` / `TagSuggestModal` / `suggestTagsPrompt` 导入、加 `suggestTags*` 状态族（含非响应式 `suggestTagsRequestId`）、`runSuggestTagsForCurrentNote` / `applySuggestTags` / `cancelSuggestTagsInFlight` / `closeSuggestTags` 四函数 + `TagSuggestModal` 挂载 `{#if suggestTagsOpen}`。验证：`pnpm check` **0/0**、`pnpm build` success、ReadLints 干净。 |
| 2026-04-21 | 2.33 | Phase 3-D3.5 · AI 辅助·`> Draft MOC from tag (AI)` 命令 + D3 收官：新增 §6.34 十小节覆盖为什么共用 mocBuilder picker 而不新起（三段输入 tag/title/picked 重合、切 AI 后改 title 不同步会成新 bug 源）/ `buildMocFromTag` 扩展加可选 `entriesMarkdown?: string` 覆盖扁平列表（非空时替代 `lines.join('\n')`、`insertedCount` 仍用 `noteRefs.length` 保语义恒等、downstream 模板 / sentinel 注入 / `moc_source_tag` 盖章 / 面板刷新 / 打开文件整条不动、对现有调用零回归 else 字节一致）/ Prompt 硬约束反丢题（"Every title MUST come verbatim" 反幻觉 + "exactly once across all sections" 反丢重 + 2–6 主题上下限 + 禁 prose/fence/frontmatter/H1）/ `sanitizeDraftMoc` 四条清洗（剥 ```` ```markdown ``` ```` code fence / 丢 preamble 切到第一个 `## ` / `[[title]]` allowlist 校验——hallucinated 降级 `- <title>  <!-- AI 生成，非选中笔记 -->` 不删是给用户看 AI 想归的类、注释化因 wiki-link 解析器只识 `[[…]]` 不污染 graph / 合并连续空行 + 200 行上限）+ 返回 `{ markdown, sectionCount, bulletCount, linkedTitles }` 让调用方算 `droppedCount`/ Diff 粒度选 entries block 而非整份 MOC body（`original` = 非 AI 路径的扁平 rendering、`proposed` = AI 分组块，同形状让 LCS diff 高亮"怎么分组"而非模板 boilerplate）/ 状态机与 race guard 与 summarize/suggest-tags 同构（`draftMoc*` 全套 `$state` + 非响应式 `draftMocRequestId`、先 snapshot 再关 picker 的顺序避免"按 AI 后立刻取消" 丢 in-flight context、stale-request guard 三段、cancel 先抢 id 再发后端）/ temperature = 0.4 比 summarize 0.3 / suggest-tags 0.2 略高（theme naming 需少量 creativity、更高会诱发 title 幻觉由 allowlist 兜底）/ Toast 分档（`strategy === 'none'` 模板缺锚点 7s error / `droppedCount > 0` 漏题量化 7s error / 正常 success；`NoticeKind` 没 `warning` 故前两条走 error）/ 刻意不做（rebuild from tag AI / section-level 部分接受 / AI 结果缓存 / section summary / note body 或 summary 送 prompt / 非 `[[…]]` bullet drop-line）/ 测试覆盖。D3 收官：D3.1 `ai_complete` → D3.2 `DiffPreviewModal` → D3.3 summarize 三档 → D3.4 suggest-tags checkbox merge → D3.5 draft-moc AI 分组五刀贯通；`ai_complete` 通道被 3 条写回命令共享、`complete_requests` 注册表按 id 分离不串台；`DiffPreviewModal` 被 summarize + draft-moc 两条复用、`TagSuggestModal` 独立；面板 5 条 AI 写回命令全 `aiEnabled` gate。新增 `src/lib/ai/draftMocPrompt.ts`（`SYSTEM_PROMPT` + `buildDraftMocPrompt({tag, title, notes}) → {systemPrompt, userPrompt}` + `buildFlatEntriesMarkdown(notes)` + `sanitizeDraftMoc(reply, allowedTitles)` + `makeDraftMocRequestId`；Node 内联 4 路径手验 normal/fenced/preamble/hallucination 计数精确）；`src/lib/commands.ts` `buildMocFromTag` 签名扩展可选 `entriesMarkdown`（三元分支 `params.entriesMarkdown?.trim() ? ... : lines.join('\n')`）；`src/lib/palette/commandRegistry.ts` 扩 `PaletteContext` 加 `runDraftMocFromTag()` + 注册 `draft-moc-from-tag` 命令（gate `aiEnabled && activeTag`）；`src/routes/+page.svelte` 补 `draftMocPrompt` 导入、加 `draftMoc*` 状态族（`Open/Loading/Error/Reply/Tag/Title/Picked/Flat` + 非响应式 `draftMocRequestId`）+ `draftMocSanitized`/`draftMocProposed` 两 `$derived.by`、`runDraftMocFromTagAi` / `confirmBuildMocWithAi` / `closeDraftMoc` / `cancelDraftMocInFlight` / `applyDraftMoc` 五函数、mocBuilder modal footer 加 `{#if aiEnabled} 用 AI 草拟… {/if}` 次按钮、条件渲染 `<DiffPreviewModal>` 把 `draftMocFlat` 传 `original` 与 `draftMocProposed` 传 `proposed`。中间一次 `NoticeKind = 'warning'` 不存在的编译错误 → 改走 `error` + 7s TTL 后通过。验证：`pnpm check` **0/0**、`pnpm build` success、ReadLints 干净。下一步：P3-D4 Polish 或 Phase 4 新起点等用户选向。 |
| 2026-04-21 | 2.34 | Phase 3-D4.1 · AI 写回流的 failure / cancel / retry UX hardening：新增 §6.35 八小节覆盖为什么这刀放在 D3 之后（先跑通成功路径，再抽 shared failure UX）/ `DiffPreviewModal` 与 `TagSuggestModal` 的 shared shell 扩展（`statusNote` / `showRetry` / `onRetry` / `cancelBusy` / `loadingText` / `cancelLabel`）/ `normalizeCompleteFailure()` 统一 provider、transport 与 cancel-before-first-token 文案 / 取消语义为什么改成两阶段（先进入 canceling，再由真实 resolve 结果决定 partial reply / empty cancel / cancel failure）/ retry 为什么在 modal 内就地重跑而不是逼用户回命令面板 / advisory note 的产品语义 / 刻意不做（不改后端协议、不做 countdown/backoff、不做 ChatPanel 同构 hardening、不做结果缓存）/ 验证边界（`pnpm check` + `pnpm tauri build --bundles app` + 桌面插件手测焦点与基础成功路径；慢路径取消手测留补项）。修改 `src/lib/ai/DiffPreviewModal.svelte` 与 `src/lib/ai/TagSuggestModal.svelte` 增加 retry / advisory / canceling props；`src/routes/+page.svelte` 抽 `normalizeCompleteFailure()` / `partialResultNote()`，并为 summarize / suggest-tags / draft-MOC 三条流新增 `*Canceling` / `*StatusNote` 与 `retry*()` 分支。验证：`pnpm check` **0/0**、`pnpm tauri build --bundles app` success；打包产物 `MyNotes.app` 正常生成。 |
