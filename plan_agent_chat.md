# Agent Chat 功能完整实现计划

> **定位**：本文件是"**Agent Chat 主线**"的专用执行手册，覆盖 P3-D5 **全部切片（D5.1 → D5.7）** 的剩余工作。
>
> **与其它文档关系**：
> - `design_V2.md` §6.30+ · 对话面架构决策（不复述）
> - `plan_P3.md` §4.2 D5 · Phase 3 总地图中的 agent chat 章节（本文件的源头摘要）
> - `delivery_log.md` 顶条 · D5.1 / D5.2 已落地的交付记录
> - 本文件只写"**剩余怎么做**"，不复述已完成内容
>
> **本文件产生的前置动机**：D5.1 + D5.2 两刀已落地（2026-04-21），协议层通 + 5 件读取类真工具通；下一步到 D5 收官还剩 D5.3 (UI 卡片) / D5.4 (写回类工具) / D5.5 (system prompt + 轨迹) / D5.6 (权限矩阵) / D5.7 (破坏类 + 审计，可选) 五刀。
>
> **完成判定同 plan_P3.md §7**：用户能在对话里自然语言驱动完成 "找笔记 → 提议改 → 接受 → 落盘" 全流程，不必记命令名。

---

## 0. 起点快照（2026-04-21）

### 已落地（D5.1 + D5.2）

**协议层**：
- 后端 `ChatTurn` 加 `tool_calls` / `tool_call_id` 可选字段；新增 `ChatRole::Tool`
- `ChatRequest` 加 `tools: Vec<ToolDefinition>`
- `chat_store.jsonl` schema v=2（v=1 混行向后兼容）
- OpenAI Provider SSE 重组 `BTreeMap<u32, ToolCallAccumulator>`
- `ai_chat_stream_start` 多轮循环（`MAX_TOOL_ITERATIONS = 8`）
- 原子单元历史截断 `truncate_history_to_budget`
- `MockProvider` per-iteration script

**Registry + 5 件读取类 Tool**（D5.2）：
- `services/ai/tool_registry.rs` 的 `Tool` trait + `ToolContext`（vault_root / index / embeddings / embed_model / cancel 全 `Option<_>`）
- `services/ai/tools/` 模块：`search_by_tag` / `search_fulltext` / `list_tags` / `read_note` / `get_related_notes`
- `lib.rs::setup()` 里 `register_readonly_tools(&mut reg)` 预组装
- `related_notes_core` 抽 `pub(crate)` 命令 + 工具共用
- `fts_sanitize` 提升 `pub(crate)`；FTS5 contentless `f.path=NULL` → rowid JOIN 修正
- 36 条新 tool 测试 + 208 原有，共 244 全绿

**前端**：
- `ai.ts` 加两事件常量 `CHAT_STREAM_TOOL_CALL_{REQUESTED,RESULT}_EVENT` + TS payload 类型
- `ChatRole` 扩 `'tool'`；`ChatMessage.tool_calls?: ToolCall[]`
- `ChatPanel.svelte` 加 `inlineToolEvents` 占位 chip 渲染（`▸ tool request: name(args)` / `◂ tool result: json…`）

### 未落地（本文件覆盖范围）

| 切片 | 状态 | 产物概要 |
|---|---|---|
| **D5.3** | ⬜ 待开工 | `ToolCallCard.svelte` 替换占位 chip；`ProposalCard.svelte` 骨架（空壳，D5.4 填料） |
| **D5.4** | ⬜ 待开工 | 4 件 🟡 写回类 tool：`propose_summary` / `propose_tag_update` / `propose_moc` / `propose_note_edit`；`ProposalCard` 接受 → 落盘 |
| **D5.5** | ⬜ 待开工 | System prompt 升级 + tool use trace 状态条 + history 截断对 tool messages 独立预算 |
| **D5.6** | ⬜ 待开工 | Settings "AI 工具权限"开关矩阵；运行时 pre-flight gate；关 AI 时整片禁用 |
| **D5.7** | ⬜ 可选 | 🔴 `delete_note` / `rename_note` + `.mynotes/ai/audit.log` + 双重确认 UI |

### 环境前提（每刀开工前自检）

- `cd src-tauri && cargo test --lib` — 244 全绿
- `cd src-tauri && cargo build` — 清编译
- `pnpm run check` — 0 error 0 warning
- `pnpm run build` — 成功
- `clippy --all-targets -- -D warnings` — 23 条预存 warning 维持不变（不顺手修）

---

## 1. 总体设计原则（跨刀约束）

### 1.1 铁律（产品哲学，不可动摇）

1. **Markdown 永远是 SSOT**：AI 绝不直接写 markdown；任何落盘必须走用户显式确认的 diff UI
2. **关 AI 即短路**：`ai_enabled = false` 时所有 `ai_*` IPC 入口直接 return；前端整片 UI 禁用
3. **读取 vs 写回 vs 破坏分三档**：🟢 读取免确认 / 🟡 写回 inline diff 确认 / 🔴 破坏双重确认 + 审计
4. **任何 AI 调用可审计**：delta / tool_call / 落盘结果全进 `.mynotes/ai/usage.log`（D5.7 扩到 audit.log）

### 1.2 架构骨架（D5.1/D5.2 已确立，后续刀继承）

```
用户对话输入
    ↓
aiChatStreamStart (前端) → ai_chat_stream_start (Tauri command)
    ↓
多轮循环 (0..MAX_TOOL_ITERATIONS)
  ├─ provider.chat_stream(tools=[…]) → SSE
  │     ├─ delta → CHAT_STREAM_DELTA_EVENT → 前端渲染 streamingContent
  │     └─ tool_calls accumulator 重组 → FinishToolCalls
  ├─ 持久化 Assistant-with-tool_calls 到 jsonl (先于执行)
  ├─ for call in tool_calls:
  │     ├─ CHAT_STREAM_TOOL_CALL_REQUESTED_EVENT → 前端渲染 ToolCallCard (loading)
  │     ├─ 权限 gate: 若是 🟡 propose_* → 暂停等用户接受；若 🔴 → 双重确认
  │     ├─ tool_registry.execute(name, id, args, &ctx)
  │     ├─ 持久化 Tool 消息到 jsonl (tool_call_id 绑定)
  │     └─ CHAT_STREAM_TOOL_CALL_RESULT_EVENT → 前端 ToolCallCard / ProposalCard 展示结果
  └─ 如果 tool_calls 空 → FinishText → 终止循环
    ↓
CHAT_STREAM_DONE_EVENT → 前端 loadActiveSession 刷新持久化 transcript
```

### 1.3 命名约定（工具分类前缀）

- **读取类**（🟢 免确认）：`search_*` / `list_*` / `read_*` / `get_*`
- **写回类**（🟡 inline 确认）：`propose_*` 前缀（D5.4 4 件）
- **破坏类**（🔴 双重确认 + 审计）：动词明确（`delete_*` / `rename_*` / `move_*`）

**冲突处理**：未来新增时，凡 `propose_*` 不得返回"已执行"语义，必须返回"待接受的 proposal payload"。

### 1.4 ProposalCard 落盘模型（D5.4 前置决策）

**关键分歧**：`propose_*` tool 的语义到底是"**执行**"还是"**生成 proposal**"？

- **决策**：tool 只生成 **proposal payload**（包含 target path / 原文 / 新文 / diff），**不落盘**
- **落盘触发**：前端 `ProposalCard` 用户点"接受"→ 调既有的 `file_write` / `vault.rename_with_refs` 等非 AI IPC
- **好处**：
  - AI 不持有 write capability，铁律物理兜底
  - 取消对话 / 关窗不会误写
  - 权限矩阵可以只管"能不能生成 proposal"，写 IPC 自带 vault 写 capability 检查

**ProposalPayload 统一形状**（所有 `propose_*` 复用）：
```json
{
  "proposal_kind": "summary" | "tag_update" | "moc" | "note_edit",
  "target_rel_path": "foo/bar.md",
  "original_content": "...",
  "proposed_content": "...",
  "summary": "一行人类可读摘要，chip 折叠态显示",
  "metadata": { /* per-kind 自由扩展 */ }
}
```
前端根据 `proposal_kind` 路由到不同的 card 子组件（或一个通用组件 + kind-specific preview）。

---

## 2. P3-D5.3 · Inline Tool / Diff 卡片 UI

### 2.1 范围

**做**：
- 把 ChatPanel 里的占位 chip（`inlineToolEvents` 纯文本 li）替换为 `ToolCallCard.svelte`（所有 tool 通用）
- 新建 `ProposalCard.svelte` **骨架**（空壳 props + 三按钮，accept/reject/adjust 回调先 no-op；D5.4 填料）
- 把持久化消息里 `role: 'tool'` 的 ChatMessage 也用 ToolCallCard 渲染（不只是 in-flight chip）

**不做**：
- `propose_*` 真实 tool（D5.4）
- 权限 gate（D5.6）
- System prompt 升级（D5.5）
- tool use trace 状态条（D5.5）

### 2.2 关键决策

#### (a) `ToolCallCard` vs `ProposalCard` 分工
- **ToolCallCard**：所有 tool 通用，展示 **name / 折叠 args / 折叠 result / loading 态 / error 态**
- **ProposalCard**：仅 🟡 写回类（`proposal_kind` 存在时），**包裹 ToolCallCard** 并在下方加 **diff 预览 + 三按钮**
- ProposalCard 检测 `result.content` 里能否 `JSON.parse` 出 `proposal_kind`；否则回退为 ToolCallCard

#### (b) In-flight vs Persisted
- **In-flight**：`inlineToolEvents` 里的 requested + result 按 `call_id` 配对，渲染一张 card 带 live 状态
- **Persisted**：`ChatMessage.role = 'assistant'` 带 `tool_calls` + 紧跟着的 `role: 'tool'` 消息配对，每 call 渲染一张 card（lazy mode 默认折叠）
- **统一 props**：两种来源都转换为同一 `ToolCallViewModel` 结构，card 组件不关心数据来源

#### (c) Diff 渲染
- 复用 `src/lib/ai/diffLines.ts`（LCS 行级 diff，~30 行）
- 复用 `DiffPreviewModal.svelte` 里的 `.dpm-diff-*` CSS tokens
- ProposalCard 内 diff 为**折叠默认**；点"展开完整 diff"打开 `DiffPreviewModal` 作为 adjust 入口

#### (d) 消息布局
- Assistant-with-tool_calls 的 text 部分在上；每个 tool_call 和它配对的 tool result 组成一张卡片，紧挨 Assistant 下面（hierarchical，缩进 1 级）
- 最后一个 Assistant（无 tool_calls，`finish_reason: "stop"`）才是"最终答复"气泡

### 2.3 文件变更

| # | 路径 | 变更 | 预计 LOC |
|---|---|---|---|
| 1 | `src/lib/chat/ToolCallCard.svelte` | **新** 通用 tool 卡片 | +180 |
| 2 | `src/lib/chat/ProposalCard.svelte` | **新** 写回类卡片（骨架，accept/reject/adjust 回调先 no-op 或 console.warn） | +220 |
| 3 | `src/lib/chat/toolCallViewModel.ts` | **新** in-flight + persisted → ViewModel 归一 | +100 |
| 4 | `src/lib/panel/ChatPanel.svelte` | 替换占位 chip；transcript 渲染循环加 tool_call pairing | +80 / -40 |
| 5 | `src/lib/ai/diffLines.ts` | 可能微扩：加 `unified(oldLines, newLines, context=3)` helper | +30 |

### 2.4 测试

**前端 vitest**（当前仓库暂无 vitest harness；D5.3 先不引入 harness，测试点用 **手测 checklist** + TypeScript 编译严格模式覆盖）：

- TS 严格：`ToolCallViewModel` discriminated union + `ProposalPayload` schema 类型，`pnpm run check` 0 new error
- 手测 checklist：见 §2.5

### 2.5 Done-done 判定

**自动化**：
1. `pnpm run check` — 0 new error
2. `pnpm run build` — 构建成功
3. `cargo test --lib` — 244 不回归

**手测**（用 D5.2 已经通的 5 件读取工具触发）：
4. 发 "列出我所有的 tag" → `list_tags` 卡片展示 loading→result；展开后可见完整 JSON
5. 发 "帮我找 #project 下的笔记" → `search_by_tag` 卡片 args 折叠、result 展开显示 notes 数组
6. 发 "FooNote.md 里写了什么" → `read_note` 卡片，默认折叠 content（长文本）
7. 对话流结束后刷新会话 → 持久化 transcript 里的 tool_calls 按 Assistant→Tool 配对正确显示卡片
8. 手工构造一个 fake proposal JSON 喂给 MockProvider → `ProposalCard` 骨架渲染 diff + 三按钮（按钮点击暂时 console.log）

### 2.6 风险

| 风险 | 缓解 |
|---|---|
| tool_calls 跨消息 pairing 复杂 | 在 `toolCallViewModel.ts` 做一次性转换：扫 transcript → 建 `Map<call_id, ResultMessage>`，渲染时 O(1) 查 |
| Svelte 5 rune 在嵌套组件里 prop 传递复杂 | 统一用 `$props()` + TypeScript 类型；不在 card 内部自己做 $state |
| 长 result 撑爆 UI | card 默认 **500 字符折叠**，点"展开"显示全量；超 2k 字符加滚动容器 |
| 旧会话 v=1 没有 tool_calls，不渲染卡片 | 不处理 —— v=1 本来就没这语义；ChatMessage.tool_calls 缺失时普通气泡渲染即可 |

---

## 3. P3-D5.4 · 🟡 写回类 Tool（4 件）

### 3.1 范围

**做 4 件 `propose_*`**：

| Tool | 用途 | 复用管线 |
|---|---|---|
| `propose_summary` | TL;DR 写 frontmatter.summary / 插入笔记顶部 / 复制剪贴板 | D3.3 `summarizePrompt.ts` + `ai_complete` |
| `propose_tag_update` | 基于笔记内容建议 tag 变更 | D3.4 `suggestTagsPrompt.ts` + `mergeTagsIntoFrontmatter` |
| `propose_moc` | 从 tag 草拟 MOC 结构 | D3.5 `draftMocPrompt.ts` + `sanitizeDraftMoc` + `buildMocFromTag` |
| `propose_note_edit` | 对任意笔记做自由编辑提议（新能力） | 无直接复用，新写 prompt |

**落地策略**：
- tool 只生成 **ProposalPayload**（不落盘）
- 前端 `ProposalCard` 接受 → 调 `file_write` / `vault_rename_*` 等既有 IPC（已有 capability 检查）

**不做**：
- 权限矩阵 gate（D5.6）
- 新 proposal 类型（`propose_new_note`、`propose_split_note` 等，本刀定死 4 件）
- 写回结果回填到对话（接受后不向对话添加"我已接受"消息 —— transcript 里靠 tool_call_id 顺序推断即可）

### 3.2 关键决策

#### (a) Tool 内部复用 `ai_complete` 管线？
- **方案 A**（选）：tool 内部直接调 `AiProvider::complete_text(prompt)` —— tool 层持有 provider handle，和 `ai_complete` 命令走同一条底层
- **方案 B**（弃）：tool 调 `ai_complete` IPC —— 会有 tauri command 嵌套调用 + cancel 注册表混乱

**ToolContext 扩字段**（**D5.4 重要决策**）：
```rust
pub struct ToolContext {
    // ... D5.2 已有 ...
    pub provider: Option<Arc<dyn AiProvider>>,  // 新加
    pub chat_model: Option<String>,  // 用于 complete 调用的模型名（区别于 embed_model）
}
```
`lib.rs::setup()` 不预塞 provider（lazy：每次 `ai_chat_stream_start` 构造 ctx 时从 `state` 抽）。

#### (b) `propose_note_edit` 的 prompt 形态
- 入参：`{ rel_path: string, instruction: string }`（例如 "把列表项改成表格"）
- 系统 prompt 包含：原文全量 + 指令 + "只输出修改后的完整 markdown，不要解释"
- 输出：按新全文 + `diffLines(old, new)` 组装 ProposalPayload
- **截断保护**：原文 > 8k tokens 时 tool 返回 error（提示用户先拆分笔记）；D5.5 refine

#### (c) `propose_tag_update` 合并策略
- `ai_complete` 返回候选 tags → `mergeTagsIntoFrontmatter(existing, suggested, checkboxState=all)` 模拟"全勾选"生成 preview
- ProposalCard 渲染为 **frontmatter diff**（只 diff frontmatter block）
- 用户在 ProposalCard 里可以反勾单个 tag —— 接受时用勾选子集重新 merge 后 `file_write`

#### (d) `propose_moc` 特殊性
- 现有 `buildMocFromTag` 吃"扁平列表 + 可选 AI 覆写 entriesMarkdown"
- tool 内部：先 `search_by_tag` 拉列表 → 构造 baseline flat entries → 调 AI 生成 grouped entries → `sanitizeDraftMoc` 校验 → 组成 full markdown → ProposalPayload
- 接受时 **新建文件**而不是覆盖：`target_rel_path` 指向 `2-moc/<tag>.md`，若已存在则 proposal 带 `existing: true` 标记，ProposalCard 提示用户"已存在，接受将覆盖"

#### (e) Cancel 在 tool 执行中
- 每个 `propose_*` tool 都会调 `provider.complete_text(...)` —— 这个调用必须受 `ctx.cancel` 控制
- `provider` trait 已有 cancel token 机制（D3.1 留的），tool 透传 `ctx.cancel` 即可

### 3.3 文件变更

| # | 路径 | 变更 | 预计 LOC |
|---|---|---|---|
| 1 | `src-tauri/src/services/ai/tool_registry.rs` | `ToolContext` 加 `provider` / `chat_model` 字段 | +15 |
| 2 | `src-tauri/src/services/ai/tools/mod.rs` | 再 `register_writeback_tools(&mut reg)` | +30 |
| 3 | `src-tauri/src/services/ai/tools/common.rs` | 加 `ProposalPayload` struct + `propose_ok(payload)` helper | +60 |
| 4 | `src-tauri/src/services/ai/tools/propose_summary.rs` | **新** | +180 |
| 5 | `src-tauri/src/services/ai/tools/propose_tag_update.rs` | **新** | +200 |
| 6 | `src-tauri/src/services/ai/tools/propose_moc.rs` | **新** | +260 |
| 7 | `src-tauri/src/services/ai/tools/propose_note_edit.rs` | **新** | +200 |
| 8 | `src-tauri/src/commands/ai.rs` | `ai_chat_stream_start` 构造 ctx 时传 provider + chat_model | +10 |
| 9 | `src-tauri/src/lib.rs` | setup() 里 `register_writeback_tools(&mut reg)` | +1 |
| 10 | `src/lib/chat/ProposalCard.svelte` | 接上 accept → `file_write` IPC；reject → 置 chip 灰；adjust → 打开 `DiffPreviewModal` | +120 / -20（填料阶段） |
| 11 | `src/lib/chat/acceptProposal.ts` | **新** 落盘逻辑（kind → IPC 路由） | +150 |

**总量**：约 1200 LOC + 文档

### 3.4 测试

**单测**（每 tool 4-6 条，合计 ~20 条）：
- `propose_summary` happy path（mock provider 返回 TL;DR 文本 → 检查 ProposalPayload）
- `propose_summary` cancel during complete（cancel flag 抬起 → 返回 is_error）
- `propose_tag_update` no new tags 提案（AI 返回空 → payload 仍生成，diff 为空）
- `propose_moc` dedup（AI 幻觉出不在 tag 列表里的 note → sanitize 降级为注释）
- `propose_note_edit` 超大原文（> 8k tokens → is_error）

**provider 层**：
- MockProvider 扩一个 `script_complete(responses: Vec<Result<String, Error>>)` —— 已有 chat_stream 有 per-iteration script，complete 同形

### 3.5 Done-done 判定

**自动化**：
1. `cargo test --lib` — 244 + ~20 新 = ≥264 全绿
2. `cargo build` / `pnpm check` / `pnpm build` — 三绿

**手测**：
3. 发"帮当前笔记写摘要写到 frontmatter" → `propose_summary` → ProposalCard 展示 frontmatter diff → 接受 → 文件磁盘 frontmatter.summary 更新
4. 发"给当前笔记建议标签" → `propose_tag_update` → ProposalCard 可反勾单个 tag → 接受 → frontmatter.tags 更新为勾选子集
5. 发"把 #project 下的笔记整理成 MOC" → `propose_moc` → ProposalCard 展示 MOC 全文 diff（baseline vs grouped）→ 接受 → `2-moc/project.md` 生成
6. 发"把 FooNote.md 里的列表改成表格" → `propose_note_edit` → ProposalCard 展示 body diff → 接受 → 笔记内容更新
7. 在 complete 进行时点取消 → ProposalCard 翻红 "cancelled"
8. Reject 一次，会话继续 → 下轮 assistant 可以看到 "用户拒绝了"（通过持久化 tool result 里的 is_error=false 但 content 带拒绝标记）

### 3.6 风险

| 风险 | 缓解 |
|---|---|
| tool 返回的 ProposalPayload JSON 很大（笔记全文）撑爆 SSE | content 阈值：> 64KB 时 tool 持久化 content 到 `.mynotes/ai/drafts/<id>.json` 返回引用；D5.4 先不做这个优化 —— 等真实遇到再补 |
| Accept 点击后 `file_write` 失败（权限 / 冲突） | 前端 catch → 回显 error banner；proposal 保持可见允许 retry |
| 用户在 ProposalCard 里改了勾选后 diff 和 AI 原提议不一致 | "实际落盘内容" 用勾选子集重算，不是 AI 原 proposal —— 审计 log 里记"modified_before_accept: true" |
| `propose_moc` 接受时目标文件已存在 | ProposalPayload 标记 `existing: true`；ProposalCard 强制二次确认"覆盖？"；拒绝覆盖则退回到"另存为"（用户填新路径） |
| provider trait 缺 cancel 支持 | 验证 D3.1 留的 cancel 确实穿透到 openai.rs 的 reqwest stream；若缺则补 `complete_text_with_cancel(...)` 重载 |

---

## 4. P3-D5.5 · System Prompt + Tool Use Trace + History 截断

### 4.1 范围

**做**：
- **System prompt 升级**：写一版"agent mode"系统提示，指导 AI
  - 何时调读取工具（问路 / 找笔记）vs 何时调 `propose_*`（要改东西）
  - 引用笔记统一用 `[[title]]` 格式（前端已有 wiki-link resolver）
  - 回答简洁，tool 结果不全量复述
- **Tool use trace 状态条**：ChatPanel 在流式中显示"AI 正在搜索 #xxx…"、"AI 正在读取 xxx.md…"、"AI 正在起草摘要…" 的 **inline 状态条**（不是卡片，更轻）
- **History 截断对 tool messages 独立预算**：
  - 当前 `truncate_history_to_budget` 按"原子单元"截（Assistant+Tool 不拆）
  - D5.5 改为 **分层预算**：
    - 近 N 轮保持原子完整
    - 再往前的 tool messages 可以独立 evict（只保留 Assistant text 部分，tool_calls 字段置 null，tool 消息整条删）
    - 最古老的 Assistant+User 对仍按原逻辑

**不做**：
- 权限矩阵（D5.6）
- 破坏类工具（D5.7）

### 4.2 关键决策

#### (a) System prompt 放哪
- **位置**：`src-tauri/src/services/ai/system_prompt.rs`（新文件）
- **内容**：硬编码 agent 指令 + 运行时注入 `{vault_root_name}` / `{current_note_rel_path}` / `{tool_catalog_summary}`（工具名列表）
- **注入点**：`ai_chat_stream_start` 里构造 messages 时，首条如果是 user message，前面自动插一条 `role: "system"` —— 不持久化到 jsonl（jsonl 只存 user / assistant / tool）

#### (b) 状态条轻量渲染
- 后端：每 tool_call_requested 时额外 emit 一个 `chat-stream:tool-trace` 事件（payload: `{kind: 'searching' | 'reading' | 'drafting', label: string}`）
- 或复用 `tool_call_requested` 事件 —— 前端看 `name` 判断 kind，无需新事件
- **决策**：复用现有事件，前端查表（`list_tags` → "列出标签"；`search_*` → "搜索中"；`propose_*` → "起草中"）。不加新事件减少协议面。

#### (c) History 分层截断算法
```
预算分配（示例，配置化）：
- 总预算 = provider.context_window * 0.8
- 近 3 轮原子单元 → 保留 100%
- 再往前每轮：只保留 user + assistant text，丢 tool_calls + tool msgs
- 最古老：按原 evict 逻辑
```

#### (d) 状态条和 ToolCallCard 的关系
- **并存**：状态条是"全局横条"，当前正在执行的 tool 名（"AI 正在…"），流完即消失
- **卡片**：每个 tool_call 一张，永久在对话里
- **动机**：卡片折叠后用户仍想知道"现在到底在干啥"，状态条就是 glanceable 信号

### 4.3 文件变更

| # | 路径 | 变更 | 预计 LOC |
|---|---|---|---|
| 1 | `src-tauri/src/services/ai/system_prompt.rs` | **新** agent system prompt + 注入 helper | +120 |
| 2 | `src-tauri/src/commands/ai.rs` | `ai_chat_stream_start` 调用 `inject_system_prompt(..)` | +15 |
| 3 | `src-tauri/src/services/ai/chat_store.rs` | `truncate_history_to_budget` 改分层算法；加单测 | +80 / -30 |
| 4 | `src/lib/chat/toolTraceBar.svelte` | **新** 状态条组件（一行高度，渐变背景） | +90 |
| 5 | `src/lib/panel/ChatPanel.svelte` | 接上 toolTraceBar；维护 `currentToolInFlight` 状态 | +40 |
| 6 | `src/lib/chat/toolLabels.ts` | **新** tool_name → 中文描述映射 | +40 |

### 4.4 测试

- `chat_store.rs` 单测：历史 20 轮 + 中间 5 轮带 tool_calls → 截断后预期保留近 3 轮完整 + 前 17 轮去 tool
- 手测：
  - 开新会话发"找我所有的 draft 笔记" → 看 prompt 效果（AI 应调 `search_by_tag({tag: "draft"})` 而不是闲聊）
  - 流式中观察状态条翻转（搜索中 → 列出标签 → 读取…）
  - 历史会话 100 条消息后，context 不爆

### 4.5 Done-done 判定

1. `cargo test --lib` 全绿（新增 ~6 条截断测试）
2. 手测：Agent 行为明显更"会用工具"（对比未升级前）
3. 手测：状态条在每次 tool_call 切换时可见（≥ 300ms），流结束后 100ms 内消失
4. 手测：`truncate_history_to_budget` 不再产出 orphan Tool

---

## 5. P3-D5.6 · Settings 工具权限矩阵

### 5.1 范围

**做**：
- `AppConfig` 加 `ai_tool_permissions: AiToolPermissions` 字段
- Settings "AI 辅助" 区块加 "工具权限" 折叠卡
- 运行时 pre-flight gate：`ai_chat_stream_start` 构造 `tools: Vec<ToolDefinition>` 时按权限过滤
- 关 AI 时整片变灰（复用现有 `ai_enabled` 开关）

**不做**：
- per-tool fine-grained（只按 🟢 / 🟡 / 🔴 三档开关，不按具体工具名）
- per-vault 权限（全局配置，所有 vault 共享）

### 5.2 关键决策

#### (a) 配置 shape
```rust
#[derive(Default, Serialize, Deserialize)]
pub struct AiToolPermissions {
    pub allow_readonly: bool,    // default true
    pub allow_writeback: bool,   // default true
    pub allow_destructive: bool, // default false
}
```

#### (b) Gate 位置
**三重兜底**：
1. **IPC 入口**：`ai_chat_stream_start` 构造 `tools` 列表时过滤
2. **Registry 执行前**：`tool_registry.execute(..)` 加一个 kind check（万一模型幻觉出未注册的 🔴 名字）
3. **前端发送前**：Settings 里 🟡 被关时，`aiChatStreamStart` 不传对应 tools

**主线**：第 1 道（IPC 入口）；其它为保险

#### (c) tool → category 映射
- 所有工具在 `Tool` trait 加 method `fn category(&self) -> ToolCategory`（默认 `Readonly`）
- `enum ToolCategory { Readonly, Writeback, Destructive }`
- `register_readonly_tools` / `register_writeback_tools` 约定注册时 category 和 fn 一致（不一致 `debug_assert!`）

### 5.3 文件变更

| # | 路径 | 变更 | 预计 LOC |
|---|---|---|---|
| 1 | `src-tauri/src/services/config.rs` | 加 `AiToolPermissions` struct；`AppPrefs` 加字段；`set_ai_tool_permissions` setter | +60 |
| 2 | `src-tauri/src/commands/config.rs` | 新 IPC `app_config_set_ai_tool_permissions` | +30 |
| 3 | `src-tauri/src/services/ai/tool_registry.rs` | `Tool::category()` method + `ToolCategory` enum | +20 |
| 4 | `src-tauri/src/commands/ai.rs` | `ai_chat_stream_start` 过滤 tools by permissions | +25 |
| 5 | `src/lib/settings/AiToolPermissions.svelte` | **新** 三档开关 UI | +130 |
| 6 | `src/lib/settings/SettingsView.svelte` | 挂载 AiToolPermissions 组件 | +10 |
| 7 | `src/lib/ipc/config.ts` | 加 `aiToolPermissions` 类型 + setter wrapper | +30 |

### 5.4 测试

- `config.rs` 单测：save/load/default
- 单测：`ai_chat_stream_start` 在 `allow_writeback=false` 时传给 provider 的 tools 不含 `propose_*`
- 手测：
  - Settings 里关 🟡 → 对话里让 AI 写摘要 → AI 收到 "tool not available" 会走自然语言回应
  - 关 AI 整档 → 整个权限区块灰掉
  - 关 🔴（默认就是关的，D5.7 后才显示意义）

### 5.5 Done-done 判定

1. `cargo test --lib` 全绿
2. `pnpm check` 0 new error
3. 手测：三档开关独立生效；关后对话里对应 tool 类别从 AI 视角消失

---

## 6. P3-D5.7 · 🔴 破坏类 Tool + 审计（可选）

### 6.1 范围

**做**（若做）：
- 2 件 🔴 tool：`delete_note` / `rename_note`
- `.mynotes/ai/audit.log` JSONL 每行 `{ts, tool_name, args, tool_call_id, session_id, accepted_by_user, result}`
- ProposalCard 对 🔴 加"二次确认" modal（"确认永久删除 xxx.md？此操作不可撤销"）

**不做**：
- `move_note` / `batch_delete` / `rename_folder`（可推到 Phase 4）
- 软删除 / 回收站（Tauri dialog API 已提供 OS-level trash，用那个）

### 6.2 决策要点

- **所有删除走 OS trash**（`tauri-plugin-dialog` 或 `trash` crate），**不直接 `fs::remove_file`**
- **Audit log 每次 tool execute 都写一行**，不管 accepted/rejected/cancelled
- **Rename 复用 `file_move_with_refs`** —— 已经有 refs 修正 pipeline

### 6.3 Done-done

- 手测：点"删除" → 二次确认 → 文件进 OS trash → audit.log 有条目
- 手测：取消二次确认 → audit.log 也记 `accepted_by_user: false`

### 6.4 评估

- **做的价值**：AI 在对话里能彻底替代"手工删/改名"；更连贯的 workflow
- **风险**：用户被钓鱼 prompt 诱导"删除重要笔记" —— OS trash + 二次确认 + audit 三重保护够用
- **建议**：D5.1→D5.6 全绿后再评估；若 D5.4 的 ProposalCard 交互证明用户**真的在用**，再做；否则永久搁置

---

## 7. 跨切片不变量（每刀验收必查）

1. **关 AI = 整条路径短路**
   - `ai_enabled = false` → `ai_chat_stream_start` 直接 return 错误
   - Settings 整片灰（含 D5.6 的权限矩阵）
   - ChatPanel 空态显示"AI 已禁用，请到设置开启"
2. **`.mynotes/` 目录可删 = AI 出厂重置**
   - embeddings / chats / drafts / audit.log 都在 `.mynotes/ai/`
   - 主 `index.sqlite` 不受影响
3. **铁律**：任何 markdown 变更必须用户明确点"接受"
   - tool 不直接 write；proposal payload 再经前端 IPC 才落盘
4. **测试规模预期**：
   - D5.3 结束：~244（不新增 Rust 测试）
   - D5.4 结束：~265（+4 件 propose tool × ~5 条）
   - D5.5 结束：~272（+ 截断 6 条）
   - D5.6 结束：~278（+ config roundtrip 3 条 + gate 3 条）
   - D5.7 结束：~285（+ 2 件 destructive × ~3 条 + audit 1 条）

---

## 8. 执行顺序建议

**硬序**（前后依赖）：D5.3 → D5.4 → D5.5 → D5.6。D5.7 最后评估是否做。

**理由**：
- D5.3 是 D5.4 的 UI 载体（ProposalCard 骨架必须先立住）
- D5.4 是 D5.5 的验证对象（没有 propose_*，agent system prompt 没东西指导）
- D5.5 是 D5.6 的前置（没有 tool trace，用户不知道权限矩阵开关后果）
- D5.6 是"完成判定硬闸" —— D5.7 属于 optional 增强

**并行机会**：
- D5.3 UI 开工时，可让另一个人 / 另一个 agent 并行把 D5.4 的 4 件 propose tool 后端 stub 出来（无需 UI 联调）
- D5.5 的 system prompt 可以在 D5.4 刚完时先写好，配 `propose_moc` 场景 dogfood

---

## 9. 每刀开工 checklist

每刀动手前：
- [ ] 读本文件对应章节 + `plan_P3.md` §4.2 D5
- [ ] 读 `delivery_log.md` 顶条（前一刀实际怎么做的）
- [ ] `cargo test --lib && pnpm check` 起点绿
- [ ] 进入 plan 模式跑 Explore → Design → Review → Final Plan → ExitPlanMode 五段
- [ ] 按 `delivery_log.md §0.1` 三段式（Scope / How to verify / Known gaps）落交付记录
- [ ] 更新 `plan_P3.md §0` 快照 + §11 changelog
- [ ] 本文件对应章节末尾加一行 `✅ 已完成（YYYY-MM-DD）`

每刀收工后：
- [ ] `cargo clippy --all-targets -- -D warnings` 无新 warning
- [ ] 跨切片不变量第 §7 节 4 条全过
- [ ] 手测 checklist 全通
- [ ] 更新 `design_V2.md §6.X`（若架构决策变动）

---

## 10. Out of scope（本文件明确不处理）

- **新 AI 能力**（图像理解 / 语音输入 / 自动同步到云端）—— 都不在 D5
- **Anthropic tool_use adapter** —— OpenAI 路径打通即可；Anthropic 推到 D5.7 之后
- **多 provider 热切换** —— 只支持一个 provider active
- **移动端 agent chat** —— P3-C 范围
- **Cost dashboard** —— D4.3 backlog
- **ToolCallCard 国际化** —— 硬编码中文，未来 Phase 4 再做 i18n

---

## 11. 风险汇总（跨刀）

| 风险 | 概率 | 影响 | 缓解 |
|---|---|---|---|
| Provider 厂商限速 / 多轮 tool call 触发 rate limit | 中 | 对话中断 | `MAX_TOOL_ITERATIONS = 8` 已限；用户手动 retry；D4.1 failure UX 已有 |
| ProposalCard accept 后 `file_write` 失败 | 中 | 用户以为接受了实际没写 | Accept 是 async，失败时 card 翻红 + banner；proposal 保留 |
| AI 在长会话里 confuse tool_call_id 导致错位 | 低 | 展示错乱 | 持久化时后端保证配对；前端按 call_id 严格 pairing，不按顺序 |
| Svelte 5 rune 在深层 props 传递时性能 | 低 | UI 卡顿 | Card 组件 memo 友好；不用 `$derived` 递归 |
| 用户误触"接受"导致覆盖 | 低 | 数据损失 | `file_write` 走系统 OS 有 journaling；D5.7 加 audit.log 后可追溯 |
| Tool 注册表在 dev reload 时重复注册 | 低 | panic | `ToolRegistry::register` 检查重名 assert；dev 下不会 reload setup() |
| 64KB+ ProposalPayload 撑爆 SSE | 低 | tool call 超时 | 实际遇到时再做 draft 外链；目前 ProposalPayload 里 diff 不含 binary，预期 < 16KB |

---

## 12. 里程碑 & 预估

| 切片 | 预估工日 | 累计 |
|---|---|---|
| D5.3 | 1–2 天 | 1–2 |
| D5.4 | 2–3 天 | 3–5 |
| D5.5 | 1 天 | 4–6 |
| D5.6 | 1 天 | 5–7 |
| D5.7（可选）| 1–2 天 | 6–9 |

**Phase 3 完成判定硬闸**：D5.3 + D5.4 + D5.6 三刀全绿 —— 预计 **4–6 工日**。

---

**End of plan_agent_chat.md**
