# MyNotes 交付记录

> 逐任务的 **Scope / How to verify / Known gaps** 三段式交付记录。按完成时间**倒序**，最近的在最上方。
>
> 与 `design_V2.md` 的分工：
> - `design_V2.md` 记"**为什么这样做**"——架构、原则、章节化的决策。
> - `delivery_log.md`（本文件）记"**这次做了什么 / 怎么验 / 留了什么坑**"——每个 Task 的流水账，方便下一任务快速对齐上下文。
>
> 新任务启动前先扫读最近 2–3 条；若一个 Task 修改了架构决定，**先改 design_V2 对应章节，再回来写 delivery_log**。
> 交付规范（三段式、倒序、不复述全局架构等）见 `design_V2.md §0.1`。

---

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
  - 新增后端 `commands::index::index_projects_by_status(status: Option<String>) -> Vec<NoteRef>`：`path LIKE '4-projects/%/index.md'`，`status` 为 `None` 时返回所有 project；比较用 `LOWER(TRIM(COALESCE(status,'')))`，大小写 / 首尾空白不敏感——用户写 `Active` 或 `  active ` 都能桶进 "active"（对应用户反馈"不做 enum 强校验，但查询要容忍"）。
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
