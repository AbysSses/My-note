---
title: MyNotes 设计文档 V2
status: draft
version: 0.5
created: 2026-04-18
updated: 2026-04-19
tags: [project, notes-app, design-doc]
---

# MyNotes 设计文档

> 一个基于 **LYT (Linking Your Thinking)** 工作流思想、用纯 Markdown 作数据真相源的个人跨平台知识库应用。
> 本文档是"自用练手项目"的设计蓝图，作为开发过程中的单一参考源 (single source of truth)。

---

## 0. 文档约定

- 面向读者：**项目作者本人**。因此不做名词启蒙，直接谈细节与权衡。
- "**决策**"=已拍板；"**开放问题**"=还可改；"**ADR**"=架构决策记录，格式化沉淀。
- 路径约定：`vault/` = 用户 vault 根目录；`repo/` = 代码仓库根目录。
- 修订节奏：每完成一个 Phase 更新 `updated` 字段和相关章节。

### 0.1 交付规范（适用于所有任务）

**每一个任务（以路线图 §10 的周 Task 编号为粒度）交付时，必须在 `delivery_log.md` 补一条记录，包含三段**：

1. **范围（Scope）**：这一步主要完成了什么能力 / 引入了哪些新文件或 IPC / 改动了哪些现有模块。
2. **验证方法（How to verify）**：开发者自测步骤（命令 / 手测路径 / 断言检查），做到"照着操作就能复现"。尽量给出 `cargo test` / `pnpm check` 之类可自动化的命令；纯 UI 行为用"点 A → 看到 B"描述。
3. **已知限制 / 后续跟进**：功能上未完成的部分、临时桩（stub）、以及由此衍生出来的新 TODO。

**规则**：

- 每新任务启动前先读 `delivery_log.md` 最近一条交付记录，理解上一步留下的上下文。
- 任务进行中若有设计面的决定变化，先改相应章节（§5 / §6 / §10），再去 `delivery_log.md` 记录交付。
- 交付记录按"任务完成时间"倒序写最近在最上方；一次 commit 可以包含多个任务的交付条目。
- 只写"本次交付"的增量变更，不要复述全局架构（那些放在对应章节）。
- 后端每个 IPC 必须注明：入参、返回、可能的错误模式；UI 组件必须注明：props、依赖的上游状态、能触发的下游副作用。

---

## 1. 项目定位

### 1.1 一句话目标

**一个用纯 Markdown 文件作为数据唯一真相源、以"捕获 → 消化 → 链接 → 涌现结构"为工作流的跨平台个人知识库**。桌面 (macOS/Windows/Linux) 为主，后续扩展到 iOS / 安卓 / Web。

### 1.2 核心设计原则

1. **数据自持**。所有笔记都是普通 `.md` 文件，放在用户自选文件夹里，没有专有格式。卸载 MyNotes 后数据完好。
2. **外包不重要的复杂性**。同步交给云盘/Syncthing/Git；编辑器底层交给 CodeMirror；UI 交给 Webview；分发交给 Tauri。我们只做知识库工作流这一层。
3. **索引可抛弃**。SQLite 仅是派生数据，丢了可以秒级重建。
4. **结构是涌现的，不是预设的**。不要求用户先决定一篇笔记"属于哪个分类"；分类通过 tag + MOC 在后期自然浮现。
5. **工作流先于功能**。功能多但工作流破碎 < 功能少但闭环流畅。

### 1.3 明确的非目标 (non-goals)

| 不做 | 理由 |
| --- | --- |
| 自建云同步服务 | 等同于做第二个产品；iCloud/Syncthing/Git 已够用 |
| 多人协作 | 定位是个人；协作必然引入 CRDT + 账号 |
| WYSIWYG 富文本（Notion 风格） | 坚持纯 Markdown |
| 自研插件生态 | 初期没必要 |
| 兼容 Obsidian 插件格式 | 会严重绑死架构 |
| 全文移动端原生编辑体验 | 移动端只管"捕获 + 浏览"，深度编辑留桌面 |
| PARA/层级分类 | 参见 §1.4（但保留一个轻量 `4-projects/` 作折中，见 §2.6） |

### 1.4 为什么选 LYT 不选 PARA

PARA（Projects/Areas/Resources/Archives）本质是"人生管理"框架，适合管理有死线的项目和持续责任。但用它当**知识库**的骨架，会引入"这条笔记算 Project 还是 Area 还是 Resource"的分类焦虑，每次保存都要做一次决定。

LYT（Linking Your Thinking，Nick Milo 提出）的骨架更简单：笔记只分两个状态——**未消化 (inbox)** 和 **已消化 (notes)**——结构通过手工编写的 MOC（Maps of Content）后置浮现。这样：

- 写的时候心智负担接近零；
- 扩展到几千篇笔记也不会乱；
- tag 和 `[[双链]]` 天然配合 MOC。

**ADR-0007 已拍板**：采用 LYT 工作流，不采用 PARA。

---

## 2. 核心概念

### 2.1 Vault

**Vault** 是用户指定的一个文件夹，所有笔记、模板、配置都在内部。启动时要求用户选择（首次）或记住上次的 vault。

Vault 的最小识别标志：根目录下存在 `.mynotes/config.json` 文件。

### 2.2 笔记的三种状态：Inbox / Note / MOC

LYT 工作流下，一篇笔记的生命周期：

```
      随手记                 消化/重命名            主题多了
草稿 ────────→ 0-inbox/ ──────────────→ 1-notes/ ──────→ 被 2-moc/ 里的枢纽页引用
                 │ (有些条目会被删/合并)     │                      │
                 ▼                           ▼                      ▼
             最终定稿              原子笔记（一篇一件事）         有机涌现的主题图谱
```

三个状态对应三个文件夹：

| 文件夹 | 作用 | 笔记的共同特征 |
| --- | --- | --- |
| `0-inbox/` | 捕获区。随手记，不考虑命名和分类 | 通常文件名带时间戳，内容零散、未定型 |
| `1-notes/` | 已消化的原子笔记。"一篇笔记只讲一件事" | 有 well-formed 标题、自洽、可被其他笔记链接 |
| `2-moc/` | Maps of Content。手工编写的主题枢纽页 | 通过 `[[...]]` 把 notes 里的相关笔记组织成一个可读索引 |

### 2.3 Projects：LYT 之外的第四块

纯 LYT 只有 inbox/notes/moc/journal 四块，是**知识**视角。但用下来会发现，有些内容不属于"永恒知识"，而是**某个具体项目的上下文**——目标、任务清单、进度、会议记录、决策日志、交付物链接。硬塞进 `1-notes/` 会污染原子笔记的纯度，塞进 `0-inbox/` 又显得临时。

**决策**：在 LYT 之外新增一个 `4-projects/` 顶层目录，每个项目一个**子文件夹**，里面至少有一个 `index.md`（项目枢纽页，类似 MOC 但范围窄）+ 任意项目相关笔记。

```
4-projects/
├─ my-notes/                      # 一个项目
│   ├─ index.md                   # 项目主页（必有）
│   ├─ 架构取舍讨论.md              # 项目内碎片笔记
│   └─ 2026-04-18-周会.md          # 会议记录
└─ 搬家/
    ├─ index.md
    └─ 装修预算表.md
```

**与 `1-notes/` 的分工**：

| 问题 | 放哪 |
| --- | --- |
| "Markdown 比专有格式更利于长期保存"（一个独立可读的观点/事实） | `1-notes/` |
| "MyNotes 这个项目第二周要做模板引擎" | `4-projects/my-notes/index.md` |
| "关于组件化拆分的选型讨论"（项目内部讨论） | `4-projects/my-notes/组件拆分讨论.md` |
| "组件化思想本身的原理"（知识性陈述） | `1-notes/` + 被 `2-moc/` 引用 |

**区分规则**：**可不可以被任意外部笔记链接并独立理解**——能独立理解就是知识（进 notes），依赖项目上下文才有意义就是项目内容（进该 project 子文件夹）。

**项目生命周期**：项目完成后不删，在 frontmatter 把 `status` 改成 `done` 或 `archived` 即可；侧栏会把 active / paused / done 分开显示。

这不是 PARA。**PARA 用分类来决定每篇笔记属于哪个桶**，因此存在分类焦虑；**我们用分类决定的是"这个文件夹内容的生命周期"**（项目结束后整体归档），单篇笔记依然无分类压力。

完整 Project 模块设计见 §6.11。

### 2.4 周期笔记：只保留 Daily + Weekly

- **Daily**：每天一页，用来记当日想法 / 行动 / Daily Record 碎片。
- **Weekly**：每周一页，用来做回顾和下周计划。

**不做** Monthly / Quarterly / Yearly——知识库不需要那么多层的时间单元；想写年度总结时手动 New Note 即可，不需要固定位置。

两者都放在 `3-journal/` 下。

### 2.5 Daily Record 区块

Daily 笔记里预留一个 `## 📝 Daily Record` 区块。应用支持**全局热键快速追加一条**——不打断当前工作、不切换窗口，弹一个 popover 输入框、确认后插入带时间戳的条目并保存。这是高频动作，必须做到摩擦为零。

### 2.6 Tag 与双向链接

没有目录结构做导航骨架时，**tag + 双向链接 + 全文搜索** 是主要的找回机制。

- **Tag**：机器易解析，适合跨概念标记（`#学习方法`、`#分子生物学`）。放 frontmatter 的 `tags:` 数组里，正文里 `#tag` 形式也识别。
- **双向链接**：`[[目标笔记名]]`，机器能提取成链接关系。是 MOC 的基础。
- **全文搜索**：SQLite FTS5。

三者搭配：tag 做宽泛归类，双链做精确引用，MOC 做人类可读的叙事索引。

---

## 3. 技术选型

### 3.1 桌面外壳：Tauri 2

同一份代码可以打包到 macOS / Windows / Linux / iOS / Android，同时前端构建产物可作 Web 版。Rust 后端，二进制 < 10 MB（对比 Electron 100MB+），内存占用远小于 Electron，权限模型细粒度。Tauri 2 自 2024 年底稳定发布。

**不选 Electron**：包体/内存大、移动端需另做 RN/Capacitor、跨端体验割裂。
**不选 Flutter**：Markdown 编辑器生态在 Flutter 里弱，且强绑 Dart。

**风险点**：移动端支持较新、生态不成熟；iOS 沙箱下文件夹访问需用 DocumentPicker；Rust 学习曲线（但后端代码量不大）。

### 3.2 前端：SvelteKit 5（Svelte 5 runes）

- 语法清爽，运行时开销小，学习曲线短；
- 用 `adapter-static` 以 SPA 模式跑，**不用** SSR 和服务端路由；
- 状态管理用 Svelte 5 runes (`$state` / `$derived` / `$effect`)，不引入 Redux/Zustand。

### 3.3 编辑器：CodeMirror 6

- Obsidian 自己用的，成熟；
- 模块化，按需引入 `@codemirror/lang-markdown` / `search` / `autocomplete` 等；
- 可扩展：写自定义 extension 实现 wiki 链接高亮、任务勾选、标签识别。

**不选 ProseMirror / Monaco**：ProseMirror 偏富文本；Monaco 体积大偏代码编辑器。

### 3.4 索引层：SQLite（通过 `rusqlite`）

存什么：每篇笔记的路径/mtime/title/frontmatter 摘要；tag 多对多；链接关系；任务；全文搜索 (FTS5)。

**不存**：笔记正文——正文永远以 `.md` 为真相源。SQLite 丢了全量重建。

### 3.5 文件监听：notify-rs

跨平台监听 vault 目录变化，增量更新索引。**兜底**：每 5 分钟做一次全量扫描对比 mtime，防止 `notify` 在 iCloud Drive 下漏事件。

### 3.6 模板引擎：极简占位符

**不用** Handlebars/Liquid 这类大家伙。只需：`{{date}}` / `{{date:YYYY-MM-DD}}` / `{{week}}` / `{{year}}` / `{{title}}` / `{{filename}}` / `{{prev}}` / `{{next}}` / `{{now}}` / `{{uuid}}`。自己写 100 行 Rust。

### 3.7 日期库

前端 `date-fns`（tree-shakeable）；后端 Rust `chrono`。

### 3.8 包管理与工具链

- Node：`pnpm`
- Rust：`cargo`
- 格式化：`prettier` + `rustfmt`
- Lint：`eslint` + `clippy`
- CI：GitHub Actions（Phase 2 再加）

### 3.9 技术栈一览

| 层 | 技术 | 版本锚 |
| --- | --- | --- |
| 外壳 | Tauri | 2.x |
| 前端 | SvelteKit / Svelte 5 | 2.x / 5.x |
| 打包 | Vite | 5.x |
| 编辑器 | CodeMirror | 6.x |
| 索引 DB | SQLite + FTS5 | rusqlite bundled |
| 文件监听 | notify (Rust) | 7.x |
| 日期 | date-fns / chrono | latest |
| 图标 | lucide-svelte | latest |

---

## 4. 数据模型

### 4.1 目录约定

Vault 初始化后的目录：

```
vault/
├─ 0-inbox/              # 捕获区（未消化）
├─ 1-notes/              # 正式原子笔记
├─ 2-moc/                # Maps of Content（主题枢纽页）
├─ 3-journal/            # 日记 + 周记
│   └─ 2026/
│       ├─ 2026-04-18.md    # daily
│       └─ 2026-W16.md      # weekly
├─ 4-projects/           # 项目子文件夹（每个项目一个目录）
│   └─ my-notes/
│       ├─ index.md          # 项目主页
│       └─ 架构取舍讨论.md
├─ attachments/          # 图片、附件
├─ templates/            # 模板（含默认的 inbox/note/moc/daily/weekly/project）
└─ .mynotes/             # 应用配置数据（随 Vault 一同跨端同步）
    └─ config.json
    # 注意：index.sqlite, logs 等依赖设备环境且极易同步锁死的派生缓存数据，
    # 已全部转移至系统级独立存储（如 ~/Library/Application Support/com.yanghc.mynotes/{VaultHash}/）
```

**数字前缀**是为了在文件树里按"工作流顺序"排序（inbox → notes → moc → journal → projects），不是分类编号。

`attachments/` 目录全小写无前缀，因为它不参与 LYT 工作流，只是附件仓库。

所有目录名可在 `config.json` 里覆盖，但默认开箱即用。

### 4.2 文件命名

| 场景 | 约定 | 示例 |
| --- | --- | --- |
| Inbox 条目 | `YYYY-MM-DD-HHmmss-{slug}.md` 或无 slug | `2026-04-18-143012-random-idea.md` |
| 正式笔记 | `{well-formed 标题}.md`，允许空格和中文 | `为什么-Markdown-比富文本更适合长期保存.md` 或 `为什么 Markdown 比富文本更适合长期保存.md` |
| MOC | `{主题}.md` | `2-moc/个人知识管理.md` |
| Daily | `YYYY-MM-DD.md` | `2026-04-18.md` |
| Weekly | `YYYY-Www.md`（ISO 8601） | `2026-W16.md` |
| Project 主页 | `4-projects/{slug}/index.md` | `4-projects/my-notes/index.md` |
| Project 子笔记 | `4-projects/{slug}/{任意标题}.md` | `4-projects/my-notes/架构取舍讨论.md` |

**命名建议**：正式笔记标题尽量是**一个能独立读懂的陈述/名词短语**（Evergreen Notes 原则）。好标题："Markdown 的长期可维护性优于专有格式"；坏标题："笔记格式想法"。

### 4.3 Frontmatter Schema

```yaml
---
title: string              # 可选；缺省从 H1 或文件名推断
type: inbox|note|moc|daily|weekly|project|project-note
status: draft|evergreen|archived     # type=note：draft/evergreen/archived
                                     # type=project：active/paused/done/archived（见下）
created: "YYYY-MM-DD HH:mm"
updated: "YYYY-MM-DD HH:mm"          # 保存时自动更新
tags: [tag1, tag2]
aliases: [other-name]                # 用于 wiki link 别名

# 周期笔记特有
period: "2026-W16"                   # 规范化周期 id

# MOC 特有
moc_scope: "知识管理"                 # 可选，人类可读的主题描述

# Project 特有（仅 type=project 的 index.md）
project_status: active|paused|done|archived
project_started: "2026-04-18"
project_target: "2026-06-30"          # 可选，预期完成日期
project_owner: "self"                 # 可选，多人时用

# Project 子笔记特有（仅 type=project-note）
# （已废弃 Frontmatter 中的 project_slug 字段，统一由文件相对路径 4-projects/{slug} 自动推断，遵循单一事实源原则）
---
```

**规则**：

- 应用写新笔记时自动补齐 `created` / `updated` / `type`；
- 用户可自由加未识别字段，应用原样保留；
- `updated` 在每次保存时刷新；
- `type` 由笔记所在目录推断：`0-inbox/*` → `inbox`；`1-notes/*` → `note`；`2-moc/*` → `moc`；`3-journal/*` → `daily`/`weekly`（按文件名模式）；`4-projects/{slug}/index.md` → `project`；`4-projects/{slug}/*` (非 index) → `project-note`；frontmatter 里显式写的优先。
- `status` 对 `project` 类型的语义独立——`active/paused/done/archived`，与 note 的 `draft/evergreen/archived` 共用字段但值域不同；UI 按 `type` 分别渲染。

### 4.4 模板机制

模板放在 `vault/templates/`。默认提供：

- `templates/inbox.md`
- `templates/note.md`
- `templates/moc.md`
- `templates/daily.md`
- `templates/weekly.md`
- `templates/project.md`（项目 index.md 的默认内容）
- `templates/project-note.md`（可选，项目子笔记的默认内容）

模板语法 `{{变量}}` / `{{变量:格式}}`。首次启动若模板缺失，应用写入默认版本。用户可自由修改。示例 `templates/daily.md`：

```markdown
---
title: "{{date:YYYY年MM月DD日}} ({{date:ddd}})"
type: daily
period: "{{date:YYYY-MM-DD}}"
created: "{{now}}"
updated: "{{now}}"
tags: [daily]
---

# {{date:YYYY-MM-DD}} {{date:ddd}}

## 🎯 今日计划

- [ ] 

## 📝 Daily Record

## 💭 随记

- 上一篇：[[{{prev}}]]
- 下一篇：[[{{next}}]]
```

---

## 5. 系统架构

### 5.1 进程视图

```
┌─────────────────────────────────────────────────┐
│ Tauri Main Process (Rust)                        │
│  ├─ Vault Service    (读写/初始化/模板渲染)       │
│  ├─ Indexer Service  (全量扫描 + notify + SQLite) │
│  └─ Config Service   (.mynotes/config.json)       │
│         ▲                                         │
│         │ IPC (JSON commands)                     │
└─────────▼─────────────────────────────────────────┘
┌──────────────────────────────────────────────────┐
│ Webview (SvelteKit SPA)                          │
│  ├─ Layout: Sidebar | Editor | RightPanel        │
│  ├─ Editor (CodeMirror 6)                        │
│  ├─ Command Palette                              │
│  └─ State (Svelte 5 runes)                       │
└──────────────────────────────────────────────────┘
```

### 5.2 IPC 边界

**后端负责**：所有文件读写、SQLite 读写、文件监听推送、模板渲染、路径解析、外部程序调用。

**前端负责**：UI 渲染、编辑器状态、快捷键、命令面板、业务流程编排。

IPC 命令（初版）：

```rust
// Vault lifecycle
vault_open(path: String) -> VaultInfo
vault_init(path: String) -> VaultInfo    // 创建默认目录结构 + 模板
vault_recent() -> Vec<String>

// File ops
file_read(rel_path: String) -> String
file_write(rel_path: String, content: String) -> ()
file_create_from_template(
    template: String,          // "inbox" / "note" / "moc" / "daily" / "weekly"
    vars: Map<String, String>,
    target_rel_path: String
) -> ()
file_rename(from: String, to: String) -> ()
file_move(from: String, to: String) -> ()    // 用于 Promote from inbox to notes
file_delete(rel_path: String) -> ()
file_exists(rel_path: String) -> bool
file_list(rel_dir: String) -> Vec<DirEntry>

// LYT workflow
inbox_quick_capture(content: String) -> String   // 追加到 0-inbox/ 新文件，返回 rel_path
inbox_list_unprocessed() -> Vec<NoteRef>
note_promote(from: String, new_title: String) -> String   // 0-inbox/* → 1-notes/{new_title}.md
moc_create(topic: String) -> String                        // 2-moc/{topic}.md

// Project workflow
project_create(slug: String, title: String, target: Option<Date>) -> String   // 4-projects/{slug}/index.md
project_list(status: Option<String>) -> Vec<ProjectRef>                        // 按 status 筛选
project_get(slug: String) -> ProjectDetail                                     // index.md + 子笔记列表 + 任务汇总
project_add_note(slug: String, title: String) -> String                        // 4-projects/{slug}/{title}.md
project_set_status(slug: String, status: String) -> ()                         // 修改 project_status
project_archive(slug: String) -> ()                                            // 改 status=archived，前端折叠显示
project_rename(old_slug: String, new_slug: String) -> ()                       // 目录重命名 + 重写 frontmatter project 字段

// Index queries
index_search(query: String) -> Vec<Hit>
index_tags() -> Vec<TagCount>
index_notes_by_tag(tag: String) -> Vec<NoteRef>
index_backlinks(rel_path: String) -> Vec<Link>
index_outlinks(rel_path: String) -> Vec<Link>
index_unresolved_links() -> Vec<Link>
index_rebuild() -> ()

// Periodic
periodic_open_or_create(kind: "daily"|"weekly", date: Date) -> String

// Events (后端推送前端)
on_file_changed(path)
on_file_created(path)
on_file_deleted(path)
on_index_rebuild_progress(percent)
```

### 5.3 SQLite Schema

```sql
-- 【性能核心】必须在新建连接时执行：PRAGMA journal_mode = WAL;
-- 另外，全量索引扫表操作务必包裹在同一个 BEGIN TRANSACTION ... COMMIT 事务中，以确保处理 1000+ 篇极速建表。
CREATE TABLE notes (
  path TEXT PRIMARY KEY,            -- 相对 vault 的路径
  title TEXT,
  type TEXT,                        -- inbox / note / moc / daily / weekly / project / project-note
  status TEXT,                      -- note: draft/evergreen/archived; project: active/paused/done/archived
  created TEXT,
  updated TEXT,
  size INTEGER,
  mtime INTEGER,
  project_slug TEXT,                -- 仅 type=project 或 project-note 时非 NULL
  frontmatter_json TEXT             -- 未识别字段也保留
);
CREATE INDEX idx_notes_type ON notes(type);
CREATE INDEX idx_notes_updated ON notes(updated);
CREATE INDEX idx_notes_status ON notes(status);
CREATE INDEX idx_notes_project ON notes(project_slug);

-- Projects 是 notes 里 type=project 行的物化视图；用单独表只是为了查询方便
-- 也可以不建表，完全走 notes 表 WHERE type='project'，以免双写不一致
-- 决策：不建 projects 表，查询走 notes WHERE type='project'

CREATE TABLE tags (
  note_path TEXT REFERENCES notes(path) ON DELETE CASCADE,
  tag TEXT,
  PRIMARY KEY (note_path, tag)
);
CREATE INDEX idx_tags_tag ON tags(tag);

CREATE TABLE links (
  src TEXT REFERENCES notes(path) ON DELETE CASCADE,
  dst TEXT,                         -- [[目标]] 的原文
  dst_resolved TEXT,                -- 解析后的实际路径，未解析 NULL
  link_type TEXT,                   -- wiki / markdown / embed
  position INTEGER
);
CREATE INDEX idx_links_dst ON links(dst_resolved);
CREATE INDEX idx_links_dst_unresolved ON links(dst) WHERE dst_resolved IS NULL;

CREATE TABLE tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  note_path TEXT REFERENCES notes(path) ON DELETE CASCADE,
  line INTEGER,
  text TEXT,
  done INTEGER,
  due TEXT,
  completed_at TEXT
);
CREATE INDEX idx_tasks_done ON tasks(done);

CREATE VIRTUAL TABLE notes_fts USING fts5(
  path UNINDEXED,
  title,
  body,
  content=''
);
```

### 5.4 索引更新策略

**启动时**：

1. 读 `.mynotes/config.json` 拿 vault 路径。
2. `SELECT path, mtime FROM notes` 拿上次快照。
3. 遍历 vault 所有 `.md`，对比 mtime：新增/修改/删除三态处理。
4. 启动 `notify` 监听。

**运行时**：`notify` 事件 → debounce 200ms → 更新相应行。

**单篇解析**：

```
read file → 分离 frontmatter + body
          → 解析 YAML
          → 提取 body 中的 tag (#tag)
          → 提取 wiki link [[...]]
          → 提取 task - [ ] / - [x]
          → 提取 H1 作为 title（缺省时）
          → 写入 notes / tags / links / tasks / notes_fts
```

**性能目标**：1000 篇笔记 (~10MB) 全量索引 < 2 秒，用 rayon 并行解析。

**链接解析**：`[[目标]]` 的目标解析策略——先精确匹配 title，再匹配 filename (不含扩展)，再匹配 aliases。找不到就是 unresolved，会在 UI 里特别标记（点击可创建空笔记）。

---

## 6. 功能模块详设

### 6.1 Vault 初始化

1. 首次启动或 "File → Open Vault Folder..."；
2. 系统文件对话框选文件夹；
3. 检查 `.mynotes/config.json`：
   - 已存在：直接打开；
   - 不存在：询问 "Initialize as new vault?"。确认后创建 `.mynotes/`、`0-inbox/`、`1-notes/`、`2-moc/`、`3-journal/`、`4-projects/`、`attachments/`、`templates/`；把默认模板写入 `templates/`；
4. 路径记入"最近打开"；
5. 启动索引扫描。

**边界情况**：

- 目录已是 Obsidian vault（有 `.obsidian/`）：共存，提示用户"共享 Markdown 文件"；
- 已有 Markdown 但没 LYT 结构：询问"仅加载现状"或"补齐 LYT 目录"。

### 6.2 Inbox 工作流

**快速捕获 (Quick Capture)** — 这是最高频的交互，必须零摩擦：

| 入口 | 交互 |
| --- | --- |
| 全局捕获 `Cmd+Shift+N` | （利用 `tauri-plugin-global-shortcut`）呼出系统级 Ghost Window/Spotlight 风格悬浮输入框。即使主界面最小化关闭，输入回车即保存入 0-inbox，将思维捕捉的全局摩擦降为 0。 |
| 命令面板 "Quick Capture" | 同上 |
| 右键系统托盘（Phase 2） | 全局可用 |

新建的 inbox 文件格式：

```markdown
---
type: inbox
created: "2026-04-18 14:30"
updated: "2026-04-18 14:30"
---

{用户输入的内容}
```

**Inbox 回顾视图** — 命令面板 "Review Inbox"：

- 左侧列出 inbox 所有未处理条目（按时间倒序）；
- 点开后右侧是快速编辑区，下方有三个按钮：
  - `Promote to Note`：选 `1-notes/` 目标文件名 → 补齐 title/tag → 移动到 `1-notes/`；
  - `Archive`：直接移动到 `.mynotes/archive/inbox/` 或打上 `status: archived`；
  - `Delete`：删除。

### 6.3 笔记 Promote 流程

从 inbox → notes 的晋升是知识库的关键一步，要把散乱的碎片变成"一个能独立读懂的笔记"。

**交互流程 1：整篇提炼 (Promote Full Note)**：

1. 触发 `Promote to Note`（命令面板 / 右键菜单 / `Cmd+Shift+P`）；
2. 弹窗要求输入**新标题**；

**交互流程 2：段落提取 (Block-level Extraction)**（知识重构的杀手级捷径）：

1. 选中一段包含多行思绪的 Inbox 正文；
2. 呼出菜单点击 `Extract to Note`；
3. 输入新标题，系统将该段落文本直接抽离、创建并移入 `1-notes/` 对应的独立文件里，同时在原 Inbox 文件的挖空处自动替换为 `[[新标题]]` 双向链接。这允许在一个大 Inbox 草稿中快速剥离出多个原子知识，是LYT精髓。

后续执行与重构步骤（整合与分档）：
3. 前端发 `file_move(...)` 或针对段落发起 `content_extract(...)` 命令到后端；
4. 后端：
   - 读原文件；
   - 更新 frontmatter：`type` 改为 `note`、`status: draft`、`updated: 今天`；
   - 写到新路径；
   - 删除原文件（事务：新写成功才删旧，或用 rename）；
   - 触发索引更新（旧路径的索引删掉、新路径插入）。

**链接重写**（Phase 2）：如果其他笔记已经 `[[原文件名]]` 引用了这篇 inbox 条目，晋升后要把这些 wiki 链接更新到新文件名。初版先不做，反正 inbox 里的文件一般不会被外部链接引用。

### 6.4 MOC 创建与维护

**MOC 是纯手工维护的文件**——应用不自动生成 MOC 内容，只提供创建和跳转便利。

**创建 MOC**：

命令面板 "New MOC" → 输入主题名 → 基于 `templates/moc.md` 创建 `2-moc/{主题}.md`。默认模板：

```markdown
---
title: "{{title}} · MOC"
type: moc
created: "{{now}}"
updated: "{{now}}"
tags: [moc]
moc_scope: "{{title}}"
---

# {{title}} · MOC

> 这是关于 **{{title}}** 的主题索引页。按下方分组组织相关笔记。

## 核心

- [[]]

## 延伸

- [[]]

## 待整理

- [[]]
```

用户在笔记里打 `[[moc 名字]]` 就能跳过去，或在命令面板里按 `#moc` 过滤。

**MOC 辅助建议**（Phase 2）：针对一个 tag，可以"建议的 MOC 内容"——列出打了该 tag 的所有笔记，让用户一键填进 MOC 作为起点。

### 6.5 周期笔记（简化版）

**核心命令**：

| 命令 | 快捷键 | 行为 |
| --- | --- | --- |
| Open Today's Daily Note | `Cmd+D` | 打开今日 daily，没有就从 `templates/daily.md` 建 |
| Open This Week | `Cmd+Shift+W` | 同上 weekly |
| Open Yesterday / Tomorrow | — | 相对今天 |
| Previous / Next Daily | `Alt+[` / `Alt+]` | 在当前 daily 基础上前后翻 |
| Jump to Date... | `Cmd+G` | 输入日期/周数跳转 |

**日期计算**：

- Week 用 ISO 8601（周一起始，第 1 周包含 1 月 4 日）；
- Weekly 文件命名 `YYYY-Www`（`W` 后两位数字）。

**Daily Record 快速追加**（高频交互）：

全局热键 `Cmd+Shift+D` → 弹 popover 输入框 → 确认 → 在今日 daily 的 `## 📝 Daily Record` 区块下追加：

```markdown
- 14:32 — {用户输入}
```

若今日 daily 不存在，自动创建。

### 6.6 模板引擎（Rust 实现）

**占位符语法**：`{{变量}}` 或 `{{变量:格式}}`。

**预定义变量**（以 2026-04-18 14:30 为例）：

| 变量 | 值 |
| --- | --- |
| `{{now}}` | `2026-04-18 14:30` |
| `{{date}}` / `{{date:YYYY-MM-DD}}` | `2026-04-18` |
| `{{date:YYYY年MM月DD日}}` | `2026年04月18日` |
| `{{date:ddd}}` | `Sat` |
| `{{week}}` | `2026-W16` |
| `{{year}}` | `2026` |
| `{{title}}` | 用户输入的标题 |
| `{{filename}}` | 最终文件名（不含扩展） |
| `{{prev}}` | 上一篇同粒度周期笔记 id（如 `2026-04-17`） |
| `{{next}}` | 下一篇 |
| `{{uuid}}` | 随机 UUID |

Rust 伪码：

```rust
pub fn render(template: &str, vars: &HashMap<String, String>) -> String {
    let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();
    re.replace_all(template, |caps: &Captures| {
        let key = caps[1].trim();
        if let Some(colon) = key.find(':') {
            let (name, fmt) = key.split_at(colon);
            render_formatted(name.trim(), fmt[1..].trim(), vars)
        } else {
            vars.get(key).cloned().unwrap_or_default()
        }
    }).to_string()
}
```

### 6.7 编辑器模块

**必要的 CM6 扩展**：

1. `@codemirror/lang-markdown` — 基础 Markdown 高亮；
2. `@codemirror/search` — 文件内搜索；
3. `@codemirror/autocomplete` — 补全；
4. 自定义 **wiki link 扩展**：识别 `[[...]]`、高亮、Cmd+Click 跳转、输入 `[[` 时触发补全弹窗（建议已存在笔记名）；
5. 自定义 **task 扩展**：点击 `- [ ]` 前的方框切换状态；
6. 自定义 **tag 扩展**：识别 `#tag` 高亮、点击打开 tag 聚合页；
7. **heading 折叠**；
8. （可选）vim 模式 via `@replit/codemirror-vim`。

**保存策略**：

- 编辑器 changed → debounce 500ms → 写文件；
- 写时更新 frontmatter `updated` 字段；
- 写成功后后端 notify 事件会更新索引；
- 关闭窗口前强制 flush 所有未保存缓冲。

### 6.8 命令面板

`Cmd+P` 打开。四种模式：

| 前缀 | 行为 |
| --- | --- |
| 无前缀 | 文件 fuzzy search（匹配 title 和 filename） |
| `>` | 命令 fuzzy search |
| `#` | tag 搜索 |
| `/` | 全文搜索（走 SQLite FTS5） |

命令注册表（前端）：

```typescript
interface Command {
  id: string;
  title: string;
  category: 'file' | 'periodic' | 'lyt' | 'edit' | 'navigation' | 'settings';
  hotkey?: string;
  when?: () => boolean;
  run: () => Promise<void>;
}
```

**LYT 类别必有的命令**：
- Quick Capture to Inbox
- Review Inbox
- Promote Current Note to /notes
- New MOC...
- Open Today's Daily Note
- Open This Week
- Quick Append to Daily Record

**Project 类别必有的命令**：
- New Project...
- Switch to Project... (fuzzy 选项目，打开 index.md)
- Add Note to Current Project
- Mark Project as Done / Archive Project

### 6.9 反向链接 + Tag 聚合页（Phase 1）

**反向链接面板**（右侧栏）显示当前笔记的：

- **Backlinks**：哪些笔记链到我；
- **Outgoing**：我链到哪些笔记；
- **Unresolved**：我引用了但目标不存在的链接（点击按钮生成空笔记）。

**Tag 聚合页**（Phase 1 就做）：

左侧栏有一个 "Tags" 面板，列出所有 tag + 计数（`SELECT tag, COUNT(*) FROM tags GROUP BY tag ORDER BY COUNT(*) DESC`）。点一个 tag 打开聚合页（路由 `/tag/{tag}`），列出所有打了该 tag 的笔记，按 updated 倒序。

### 6.10 图谱视图（Phase 2）

扁平结构 + 双链的知识库，图谱比目录树更能展示全貌。但图谱做得好有难度（节点布局、性能、交互），Phase 1 先不做，Phase 2 专门迭代。

Phase 1 完全可以用"反向链接 + Tag 聚合页"替代图谱的核心用途（找出相关笔记）。

### 6.11 Project 模块

**设计目标**：给"有具体目标、有截止时间、会产生一系列相关笔记"的工作一个容纳空间，同时不破坏 LYT 知识库的纯度。

#### 6.11.1 创建项目

命令面板 `> New Project...` → 弹窗输入：

| 字段 | 必填 | 示例 |
| --- | --- | --- |
| Slug（目录名） | ✓ | `my-notes` |
| Title | ✓ | `MyNotes 笔记应用` |
| Target date | ○ | `2026-06-30` |

提交后后端：

1. 创建目录 `4-projects/{slug}/`；
2. 基于 `templates/project.md` 渲染 `{slug}/index.md`；
3. 打开 index.md 编辑。

Slug 验证：`^[a-z0-9][a-z0-9\-]*$`；不允许已存在同名目录。

#### 6.11.2 项目主页（index.md）

默认模板见 §15.1。关键结构：

- frontmatter 含 `project_slug` / `project_status` / `project_started` / `project_target`；
- 正文段落：**目标 / 里程碑 / 任务清单 / 关键决策 / 相关笔记 / 日志**；
- **相关笔记**段落不手工维护——UI 自动渲染"该项目目录下的其他笔记列表"（从 SQLite 查 `WHERE project_slug=? AND type='project-note'`）；
- **任务清单**用标准 `- [ ] / - [x]`，索引器会解析进 `tasks` 表。

#### 6.11.3 项目内笔记

在项目 index.md 编辑时，命令 `Add Note to Current Project` 在 `4-projects/{slug}/` 下创建新笔记，frontmatter 自动带 `type: project-note` + `project: {slug}`。

**晋升为知识笔记**：项目笔记里如果写了一个**可独立理解**的观点，右键菜单 "Extract to /notes" 把该笔记移动到 `1-notes/` 并留一个链接在项目原位（或直接删除，由用户选）。这是 `note_promote` 命令的变体，源路径从 `0-inbox/` 扩展到 `4-projects/*/`。

#### 6.11.4 侧栏项目面板

左侧栏的"Projects"面板按 status 分组：

```
Projects
├─ ⏵ Active  (3)
│   ├─ MyNotes 笔记应用
│   ├─ 搬家
│   └─ 博士开题准备
├─ ⏸ Paused  (1)
├─ ✓ Done  (5)           （默认折叠）
└─ 🗄 Archived  (12)     （默认折叠）
```

点击展开某项目：显示项目下所有笔记的列表。

#### 6.11.5 跨项目查询

- 首页 Home 视图新增"活跃项目"卡片（至多 3 个 active）；
- 命令面板 `> Switch to Project...` fuzzy search 所有项目；
- 所有项目共有的任务汇总（Phase 2）：从 `tasks` 表按 `project_slug` 聚合，显示未完成任务 + 项目。

#### 6.11.6 与 MOC 的关系

**Project ≠ MOC**：

| | Project (index.md) | MOC |
| --- | --- | --- |
| 范围 | 单个具体项目的上下文 | 一个长期主题的知识索引 |
| 生命周期 | 有开始有结束 | 永恒，随知识积累演化 |
| 位置 | `4-projects/{slug}/index.md` | `2-moc/{topic}.md` |
| 相关笔记 | 机器自动列（同目录） | 人手写 `[[...]]` |
| 状态字段 | active/paused/done/archived | 无 |

但可以互相引用：项目 index.md 可以在"延伸阅读"里链一个 MOC；MOC 也可以反向链接一个已完成项目（作为知识来源）。

#### 6.11.7 归档与清理

项目进入 `archived` 后：

- 默认从侧栏 Active 组隐藏（仍可在 "Archived" 组下展开）；
- 首页和命令面板默认不列出；
- 仍参与全文搜索和 tag 聚合（知识还在里面）；
- 不自动移动文件位置——保留 `4-projects/{slug}/` 原位，避免破坏已建立的 wiki 链接。

（可选：Phase 2 的 "Move to .mynotes/archive/" 动作，彻底归档到用户看不到的位置。）

---

## 7. UI 布局

### 7.1 主窗口

```
┌─────────────────────────────────────────────────────────┐
│ Titlebar / 菜单                                         │
├──────────┬──────────────────────────────────┬───────────┤
│ Sidebar  │ Editor                           │ RightPanel│
│          │                                  │           │
│ ├ Files  │ [frontmatter collapsed]          │ Backlinks │
│ ├ Tags   │ # Title                          │ Outlinks  │
│ ├ Inbox  │ ...                              │ Unresolved│
│ ├ Projects│                                 │           │
│ └ Recent │                                  │           │
│          │                                  │           │
├──────────┴──────────────────────────────────┴───────────┤
│ Status Bar: vault · cursor · words · sync hint          │
└─────────────────────────────────────────────────────────┘
```

侧栏可折叠。命令面板、Quick Capture 均以弹窗居中覆盖。

### 7.2 首页 (Home) 视图

左栏无选中笔记时，右主区显示首页：

- 今日 daily 链接（或"新建今日"按钮）；
- 本周 weekly 链接；
- Inbox 未处理数量 + "Review Inbox" 按钮；
- **活跃项目卡片**（最多 3 个 active project，显示 title + 目标日期 + 未完成任务数）；
- 最近编辑的 5 篇笔记；
- 最近创建的 5 个 MOC；
- 底部一个随机"旧笔记回顾"（激发联想）。

### 7.3 主题与 UI 演进策略

默认跟系统浅色/深色。遵循 **“功能造骨，UI 塑皮”** 的渐进式节奏：

1. **骨架期 (Phase 1 前期)**：保持高冷严谨的 Wireframe 风格。不强求视觉震撼，但必须**立好规矩**——所有色彩、字号、圆角、间距强制抽离为 CSS Variables（例如 `var(--bg-primary)`），为全量化主题引擎打好地基。
2. **打磨期 (Phase 1 尾声 / Phase 2)**：当核心的 LYT 数据流能够顺畅运转时，集中注入现代化视觉质感（Wow Factor）。
   - **毛玻璃与光影 (Glassmorphism)**：为系统的 Ghost Window 全局捕获框、命令面板加入 macOS 级别的高斯模糊与微妙投影。
   - **微动效反馈 (Micro-animations)**：为列表 Hover、双向链接预览片段、Inbox 提炼晋升时的位移加入 Svelte 原生的平滑过渡 (Transitions)，赋予应用生命力。
   - **排版美学 (Typography)**：摒弃默认字体，引入高品质的 Inter 或 Roboto；深度定制 CodeMirror 渲染层，拉开 Markdown 标题字重、引用块等层级对比，创造沉浸式阅读与写作体验。

---

## 8. 同步策略

### 8.1 原则

**MyNotes 本身不实现同步**。用户选云盘/Syncthing/Git 同步 vault 目录。

### 8.2 应用层面的同步友好设计

1. **文件写入原子化**：先写 `foo.md.tmp` → fsync → rename 到 `foo.md`（Rust `tempfile::persist`）。
2. **外部修改检测**：编辑器内有未保存缓冲但文件被外部改了（mtime 变），弹 diff 让用户选。
3. **彻底隔离可云端同步的纯净配置与本地机器派生缓存**：
   - `.mynotes/config.json` 等业务流规则放在 Vault 根目录内进行多端无门槛共享同步。
   - **强烈禁止**将 `index.sqlite`、全文倒排索引或应用实时运行日志放入被接管的 Vault 域内。因为文件指纹和高频锁定状态一旦汇入 iCloud/Syncthing 极大概率爆发锁源故障。App 应自动获取并将其强制放置至各系统专属的应用支撑用户目录 (如 macOS 对应为 `~/Library/Application Support/com.yanghc.mynotes/{VaultHash}/`)。
4. **`attachments/` 支持按需同步**（较大文件不需要在所有设备上都有）——Phase 2 考虑"附件下载按需"机制。

### 8.3 跨端共存

同一 vault 被 MyNotes + Obsidian 同时打开：`.mynotes/` 与 `.obsidian/` 不冲突。同一 `.md` 被两边同时编辑时只做检测不做合并。

---

## 9. 跨平台路线

### 9.1 Phase 1（桌面）

目标：macOS Apple Silicon 优先、Intel 次之、Windows x64 兼顾、Linux 能编译。Tauri 2 本身支持三桌面。

### 9.2 Phase 2（移动）

移动端的根本挑战：vault 文件夹访问。

- **iOS**：Swift 桥 DocumentPicker + Security-Scoped Bookmark 保存权限；
- **Android**：SAF (Storage Access Framework)；
- 两端都要考虑权限被系统回收后的恢复流程。

**移动端沙盒与功能降级重构策略**：
鉴于 iOS DocumentPicker 机制对高频和过量文件的沙盒监听处理极不友好，且与底层 iCloud 云状态交互时极易权限失效崩溃。首版移动端产品架构应摒弃生搬硬套获取 Vault 满状态。
移动端产品定位为轻量级“延展终端”，核心三大特性：
1. **轻量全局 Quick Capture** 即刻录入 Inbox（如技术上阻力过大，甚至可采用自建局域网 HTTP API 或专用同步云桥机制作为捷径）；
2. 追加 Daily Record 记录灵感；
3. **低频静态的笔记阅览**。

遵循第一性原理，移动外设不具有宏观统筹的信息梳理职责。一切大规模、深度的原子笔记结构维护、MOC 手工架构以及复杂搜索留于桌面生态。

### 9.3 Phase 3（Web）

前端代码本身就是 Web 应用，`pnpm build` 产物可作静态站点部署。

Web 版"文件系统"层要重写：

- **File System Access API**（Chrome/Edge/Opera 支持，Safari/Firefox 不支持）；
- **回退**：IndexedDB 存 vault 镜像 + 导入/导出 zip。

Web 版定位"演示 + 临时查看"，不是主力。

---

## 10. 路线图

### Phase 1 — 桌面 MVP（预估 4-5 周）

| 周 | 目标 |
| --- | --- |
| Week 1 | Tauri + SvelteKit 脚手架；vault 打开/初始化（含 LYT + projects 目录）；文件树和基础 CM6 编辑器；读/写文件走通 |
| Week 2 | 模板引擎 + Daily/Weekly 周期笔记（`Cmd+D` / `Cmd+Shift+W` 打得开）；**Quick Capture** (`Cmd+Shift+N`) 和 **Daily Record 追加** (`Cmd+Shift+D`) |
| Week 3 | 索引（SQLite + FTS5）+ 反向链接面板 + Tag 聚合页 + wiki link CM6 扩展 |
| Week 4 | 命令面板 + Inbox Review 视图 + Promote 流程 + MOC 创建命令 + Home 页 + 打磨 |
| Week 5 | **Project 模块**：`New Project` 命令 + 侧栏 Projects 面板 + project index.md 模板 + "相关笔记自动列表"渲染 + "Add Note to Current Project" + "Extract to /notes" |

**Phase 1 完成定义**：

- 能选/初始化 vault；
- Quick Capture 到 inbox 摩擦接近零；
- 能一键打开今日/本周笔记；
- Daily Record 快速追加工作；
- 笔记里 `[[wiki link]]` 和 `#tag` 能高亮、补全、跳转；
- 反向链接 / tag 聚合 / 全文搜索都能用；
- Inbox Review 和 Promote to Note 两个关键工作流完整闭环；
- 能创建/切换/归档项目，项目子笔记能正确识别、"相关笔记"自动列表可用；
- **可以作为日常知识库取代 Obsidian 基本用途**。

### Phase 2 — 提升（2-4 周）

- 图谱视图；
- 图片/附件粘贴与管理；
- 链接重写（笔记重命名时更新所有引用）；
- MOC 辅助建议（基于 tag 自动列出候选）；
- 设置界面（目录、模板、快捷键、中文分词选项）；
- 主题切换；
- 导出（整个 vault 导 zip、单篇导 PDF）。

### Phase 3 — 扩展（按需）

- iOS / Android 版（Quick Capture + 浏览）；
- Web 版（只读浏览）；
- AI 模块（基于 vault 做 RAG，接 OpenAI/Claude API）。

### Phase 4 — 质量工程

- E2E 测试（Playwright）；
- 单元测试覆盖 parser/template/indexer；
- CI/CD + 签名分发。

---

## 11. 项目结构

```
my-notes/
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs
│  │  ├─ commands/
│  │  │  ├─ mod.rs
│  │  │  ├─ vault.rs
│  │  │  ├─ file.rs
│  │  │  ├─ index.rs
│  │  │  ├─ lyt.rs             # inbox/promote/moc 相关
│  │  │  ├─ project.rs         # project create/list/archive/rename
│  │  │  ├─ periodic.rs
│  │  │  └─ template.rs
│  │  ├─ domain/
│  │  │  ├─ mod.rs
│  │  │  ├─ frontmatter.rs
│  │  │  ├─ periodic.rs
│  │  │  ├─ tasks.rs
│  │  │  └─ links.rs
│  │  ├─ services/
│  │  │  ├─ mod.rs
│  │  │  ├─ config.rs
│  │  │  ├─ indexer.rs
│  │  │  ├─ parser.rs
│  │  │  └─ template.rs
│  │  ├─ db/
│  │  │  ├─ mod.rs
│  │  │  ├─ schema.sql
│  │  │  └─ migrations/
│  │  └─ error.rs
│  ├─ tauri.conf.json
│  ├─ Cargo.toml
│  └─ build.rs
│
├─ src/
│  ├─ app.html
│  ├─ routes/
│  │  ├─ +layout.svelte
│  │  ├─ +page.svelte              # Home
│  │  ├─ note/[...path]/+page.svelte
│  │  ├─ tag/[tag]/+page.svelte
│  │  └─ inbox/+page.svelte        # Inbox Review
│  ├─ lib/
│  │  ├─ ipc/
│  │  │  ├─ vault.ts
│  │  │  ├─ file.ts
│  │  │  ├─ index.ts
│  │  │  ├─ lyt.ts
│  │  │  ├─ project.ts
│  │  │  └─ periodic.ts
│  │  ├─ editor/
│  │  │  ├─ Editor.svelte
│  │  │  ├─ extensions/
│  │  │  │  ├─ wikilink.ts
│  │  │  │  ├─ task.ts
│  │  │  │  ├─ tag.ts
│  │  │  │  └─ frontmatter.ts
│  │  │  └─ keymap.ts
│  │  ├─ cmdk/
│  │  │  ├─ CommandPalette.svelte
│  │  │  ├─ QuickCapture.svelte    # Cmd+Shift+N 弹窗
│  │  │  ├─ QuickDailyRecord.svelte # Cmd+Shift+D 弹窗
│  │  │  ├─ registry.ts
│  │  │  └─ commands/
│  │  │     ├─ file.ts
│  │  │     ├─ periodic.ts
│  │  │     ├─ lyt.ts              # inbox/promote/moc
│  │  │     └─ project.ts          # new/switch/add-note/archive
│  │  ├─ sidebar/
│  │  │  ├─ FileTree.svelte
│  │  │  ├─ TagList.svelte
│  │  │  ├─ InboxBadge.svelte
│  │  │  ├─ ProjectsPanel.svelte
│  │  │  └─ RecentList.svelte
│  │  ├─ right/
│  │  │  ├─ BacklinksPanel.svelte
│  │  │  ├─ OutlinksPanel.svelte
│  │  │  └─ UnresolvedPanel.svelte
│  │  ├─ home/
│  │  │  └─ Home.svelte
│  │  ├─ state/
│  │  │  ├─ vault.svelte.ts
│  │  │  ├─ editor.svelte.ts
│  │  │  └─ ui.svelte.ts
│  │  └─ utils/
│  │     ├─ date.ts
│  │     └─ path.ts
│  └─ styles/
│     ├─ app.css
│     └─ themes/
│        ├─ light.css
│        └─ dark.css
│
├─ templates/                       # 打包内置的默认模板
│  ├─ inbox.md
│  ├─ note.md
│  ├─ moc.md
│  ├─ daily.md
│  ├─ weekly.md
│  ├─ project.md
│  └─ project-note.md
│
├─ docs/
│  ├─ design.md                     # 本文件副本
│  └─ adr/                          # 架构决策记录
│
├─ tests/
│  ├─ rust/
│  └─ e2e/
│
├─ package.json
├─ pnpm-lock.yaml
├─ svelte.config.js
├─ vite.config.ts
├─ tsconfig.json
├─ .gitignore
├─ .prettierrc
├─ .eslintrc
└─ README.md
```

---

## 12. 关键决策记录（ADR 摘要）

### ADR-0001：选 Tauri 2 不选 Electron

**理由**：同一技术栈全平台；Rust 后端安全小；Webview 原生节省内存。
**代价**：Rust 学习曲线；Tauri 2 移动端生态新。

### ADR-0002：纯 Markdown 作数据真相，SQLite 作索引

**理由**：用户随时带走数据；兼容 Obsidian 生态；重建成本低。
**代价**：每次启动需索引；需 notify + 兜底全量校验。

### ADR-0003：同步外包给云盘/Syncthing/Git

**理由**：Obsidian 十余年印证可行；自建同步等于做第二个产品。
**代价**：不能保证冲突零。
**缓解**：原子写 + 外部修改检测 + 启动全量校验。

### ADR-0004：选 SvelteKit 不选 React

**理由**：写法清爽、运行时开销小、学习曲线短；个人项目最适合。
**代价**：生态比 React 小。

### ADR-0005：选 CodeMirror 6 不选 ProseMirror

**理由**：偏文本、Markdown 友好；Obsidian 验证可行；可扩展。
**代价**：WYSIWYG 能力弱（刻意的）。

### ADR-0006：目录名默认固定（可覆盖）

**理由**：零配置可用；想自定义的人能改。

### ADR-0007：采用 LYT 工作流，不采用 PARA

**背景**：PARA 是人生管理框架不是知识库框架；用它当 KB 骨架会引入分类焦虑。
**决策**：LYT（0-inbox + 1-notes + 2-moc + 3-journal）。
**理由**：心智负担低；结构通过 MOC 后置浮现；扩展到几千篇仍不乱。
**代价**：新用户需理解 Inbox → Note 的晋升流程（首次启动给 tutorial）。

### ADR-0008：周期笔记只保留 Daily + Weekly

**背景**：LifeOS 五级（Daily/Weekly/Monthly/Quarterly/Yearly）对知识库过重。
**决策**：只固化 Daily + Weekly。
**理由**：Monthly 及以上频率低，手动 New Note 即可；少一个层级，少一套模板和命令。
**代价**：想做年度回顾时没有预设位置（但用户可自己建 MOC 或 note）。

### ADR-0009：Tag 聚合页进 Phase 1

**背景**：没有目录结构做导航主干时，tag 是主要找回机制之一。
**决策**：Phase 1 就做 tag 聚合页，不延后到 Phase 2。
**理由**：没有 tag 聚合页，知识库规模一大就会退化为"只能搜索"。

### ADR-0010：MOC 不自动生成，纯手工维护

**背景**：是否自动把"打了 #xxx tag 的笔记"聚合成 MOC？
**决策**：不自动生成 MOC 内容；MOC 是手写的叙事索引页。
**理由**：MOC 的价值在"人类可读的叙事组织"而非机械列表；机械列表由 tag 聚合页提供。
**代价**：MOC 要用户主动写。
**缓解**：Phase 2 提供"MOC 辅助建议"——基于 tag 列出候选笔记，让用户一键填充作起点。

### ADR-0011：引入 `4-projects/` 作为 LYT 之外的第四块

**背景**：纯 LYT 结构（inbox/notes/moc/journal）只覆盖"知识"视角；"具体项目的上下文"（目标、任务、会议、决策、交付物）无处可放。塞进 `1-notes/` 会污染原子笔记的纯度。
**决策**：新增 `4-projects/` 顶层目录；每个项目一个**子文件夹**，含一个必有的 `index.md`（项目主页）+ 任意 project-note。
**理由**：
1. 项目的"有始有终"特征与知识笔记的"永恒"特征天然不同，应分目录；
2. 子文件夹方式让同一项目的笔记物理集中，方便批量归档；
3. 不引入 PARA 的分类焦虑——**分类对象是"项目"而不是"每篇笔记"**；
4. 项目内部若产出可独立理解的知识，通过 "Extract to /notes" 提升到 `1-notes/`，保持知识库的可复用性。
**代价**：多一个顶层目录、一个 type（`project` / `project-note`）、一套 IPC 命令、一套侧栏 UI。
**备选方案（已否决）**：
- 用 tag `#project/my-notes` 代替目录——无法表达"项目结束后整体归档"的语义，搜索时也不好过滤；
- 每个项目做成一个独立的 MOC——MOC 是永恒主题索引，与项目的生命周期不匹配。

---

## 13. 风险与开放问题

### 13.1 已识别风险

| 风险 | 影响 | 缓解 |
| --- | --- | --- |
| Tauri 2 移动端 bug 多 | Phase 2 进度风险 | Phase 1 不碰；Phase 2 先做 PoC 再决定投入 |
| notify-rs 在 iCloud Drive 漏事件 | 索引不同步 | 每 5 分钟全量校验兜底 |
| CM6 学习曲线 | Week 1 进度慢 | 先用最基础配置跑通，扩展慢慢加 |
| Rust 入门门槛 | 进度慢 | 后端保持 thin，复杂逻辑放前端 |
| iCloud/Syncthing 冲突 | 数据风险 | 原子写 + 外部修改检测 |
| SQLite FTS5 中文分词差 | 搜索效果弱 | Phase 1 用 unicode61；Phase 2 考虑 trigram 或 jieba |
| 用户手写坏 frontmatter | 解析失败 | 解析失败降级：文件原样保留，不写索引，UI 标红 |
| 新用户不懂 LYT 工作流 | 上手困难 | 首次启动给 3 步 tutorial 解释 Inbox → Note → MOC |

### 13.2 开放问题

1. **快捷键是否可配置**：Phase 1 硬编码，Phase 2 加配置。
2. **是否支持 `[[wiki link|显示文本]]` 语法**：倾向支持（与 Obsidian 一致）。
3. **是否支持 `![[embed]]` 内嵌**：Phase 2 决定。
4. **图片放哪**：倾向 `vault/attachments/`（Phase 2 支持粘贴自动保存）。
5. **MOC 是否允许嵌套（MOC 引用 MOC）**：完全允许，不做特殊处理。
6. **是否实现 Templater 式的 JS 表达式模板**：不做。
7. **多语言 i18n**：Phase 3 考虑。
8. **"Promote" 时是否做 AI 辅助标题建议**：后续 AI 模块可以做。

### 13.3 零号决策（编码前定）

- 代码仓库托管：GitHub private repo（推荐）还是 GitLab / 本地？
- 许可证：MIT / AGPL / proprietary（自用先不公开，`LICENSE` TODO）？
- 版本号：SemVer。
- Tauri identifier：`com.yanghc.mynotes` 或 `vip.yanghc.mynotes`。

---

## 14. 开发环境与上手

### 14.1 工具链要求

- Rust ≥ 1.75（稳定版）
- Node ≥ 20
- pnpm ≥ 8
- macOS：Xcode Command Line Tools
- Windows：WebView2（Win10 内置）
- Linux：`libwebkit2gtk-4.1-dev` 等

### 14.2 初始化命令（scaffold 时用）

```bash
# 1. 创建项目
pnpm create tauri-app
# 选 SvelteKit + TypeScript

cd my-notes

# 2. 前端依赖
pnpm add -D @codemirror/lang-markdown @codemirror/state @codemirror/view \
            @codemirror/search @codemirror/commands @codemirror/autocomplete \
            date-fns lucide-svelte

# 3. Rust 依赖
cd src-tauri
cargo add rusqlite --features "bundled"
cargo add notify
cargo add serde serde_json serde_yaml
cargo add chrono
cargo add tokio --features "full"
cargo add tracing tracing-subscriber
cargo add thiserror anyhow
cargo add regex
cargo add rayon
```

### 14.3 第一个可运行里程碑（Week 1 末）

能做到：

1. 启动应用，选一个空文件夹；
2. 确认初始化 → 产生 `0-inbox / 1-notes / 2-moc / 3-journal / 4-projects / attachments / templates / .mynotes` 结构 + 默认模板；
3. 左侧栏文件树可见；
4. `Cmd+D` → 从 `templates/daily.md` 生成今日 daily 并打开；
5. `Cmd+Shift+N` → 弹 Quick Capture 输入框；回车 → inbox 新文件生成；
6. 编辑器能打字、保存，关闭重开内容还在。

---

## 15. 附录

### 15.1 默认模板

**`templates/inbox.md`**：

```markdown
---
type: inbox
created: "{{now}}"
updated: "{{now}}"
---

{{content}}
```

**`templates/note.md`**：

```markdown
---
title: "{{title}}"
type: note
status: draft
created: "{{now}}"
updated: "{{now}}"
tags: []
aliases: []
---

# {{title}}

```

**`templates/moc.md`**：

```markdown
---
title: "{{title}} · MOC"
type: moc
created: "{{now}}"
updated: "{{now}}"
tags: [moc]
moc_scope: "{{title}}"
---

# {{title}} · MOC

> 关于 **{{title}}** 的主题索引。

## 核心笔记

- [[]]

## 延伸阅读

- [[]]

## 待整理

- [[]]
```

**`templates/daily.md`**：

```markdown
---
title: "{{date:YYYY年MM月DD日}} ({{date:ddd}})"
type: daily
period: "{{date:YYYY-MM-DD}}"
created: "{{now}}"
updated: "{{now}}"
tags: [daily]
---

# {{date:YYYY-MM-DD}} {{date:ddd}}

## 🎯 今日计划

- [ ] 

## 📝 Daily Record

## 💭 随记

---

- 上一篇：[[{{prev}}]]
- 下一篇：[[{{next}}]]
```

**`templates/weekly.md`**：

```markdown
---
title: "{{week}} 周记"
type: weekly
period: "{{week}}"
created: "{{now}}"
updated: "{{now}}"
tags: [weekly]
---

# {{week}} 周记

## 📌 本周重点

## ✅ 已完成

## 🔄 未完成 / 下周继续

## 💡 本周新想法

## 📚 本周新增笔记

<!-- 本区域由 App（通过读取 SQLite 索引映射的）于前端侧边或正文底部挂接动态视图。坚守不向 Markdown 文本底层静默强改写入字符数据（防止多端覆盖污染） -->

## 🤔 思考 / 反思

---

- 上一周：[[{{prev}}]]
- 下一周：[[{{next}}]]
```

**`templates/project.md`**（项目 index.md）：

```markdown
---
title: "{{title}}"
type: project
project_status: active
project_started: "{{date:YYYY-MM-DD}}"
project_target: "{{project_target}}"
created: "{{now}}"
updated: "{{now}}"
tags: [project]
---

# {{title}}

> 项目起始 {{date:YYYY-MM-DD}}{{#if project_target}} · 目标日期 {{project_target}}{{/if}}

## 🎯 目标

一句话说清楚这个项目要解决什么问题、交付什么。

## 🗺️ 里程碑

- [ ] M1 — 
- [ ] M2 — 
- [ ] M3 — 

## ✅ 任务

- [ ] 
- [ ] 

## 🧭 关键决策

<!-- 把重要的方向性决策沉淀在这里，日期+理由 -->

## 📎 相关笔记（衍生列表动态展示区）

<!-- 遵守只读交互：由应用前端在渲染试图时直接检索系统子笔记状态附加信息流视图。严禁底层向原文件内植入数据。 -->

## 🔗 相关知识

<!-- 手动链接到 1-notes/ 里可复用的知识笔记 -->

- [[]]

## 📝 日志

- {{date:YYYY-MM-DD}} — 项目启动
```

**`templates/project-note.md`**：

```markdown
---
title: "{{title}}"
type: project-note
created: "{{now}}"
updated: "{{now}}"
tags: []
---

# {{title}}

```

### 15.2 示例 Vault 布局（做一段时间后）

```
vault/
├─ 0-inbox/
│   ├─ 2026-04-18-143012-突然想到的合成生物学方向.md
│   └─ 2026-04-18-165800-读论文联想.md
├─ 1-notes/
│   ├─ Markdown 的长期可维护性优于专有格式.md
│   ├─ 知识库的价值在涌现不在预设.md
│   ├─ LYT 工作流的三态：Inbox-Note-MOC.md
│   └─ 反向链接比目录树更适合知识涌现.md
├─ 2-moc/
│   ├─ 个人知识管理.md
│   ├─ 笔记软件架构.md
│   └─ 分子生物学核心概念.md
├─ 3-journal/
│   └─ 2026/
│       ├─ 2026-04-15.md
│       ├─ 2026-04-16.md
│       ├─ 2026-04-17.md
│       ├─ 2026-04-18.md
│       └─ 2026-W16.md
├─ 4-projects/
│   ├─ my-notes/                    # status: active
│   │   ├─ index.md
│   │   ├─ 架构取舍讨论.md
│   │   └─ 2026-04-18-设计评审.md
│   ├─ 搬家/                         # status: active
│   │   ├─ index.md
│   │   └─ 装修预算表.md
│   └─ 2025-博士开题/                # status: done
│       ├─ index.md
│       └─ 文献综述草稿.md
├─ attachments/
│   └─ 2026-04/
│       └─ some-figure.png
├─ templates/
│   ├─ inbox.md
│   ├─ note.md
│   ├─ moc.md
│   ├─ daily.md
│   ├─ weekly.md
│   ├─ project.md
│   └─ project-note.md
└─ .mynotes/
    └─ config.json
```

### 15.3 参考资源

- Tauri 2 文档：<https://v2.tauri.app/>
- SvelteKit 文档：<https://svelte.dev/docs/kit>
- CodeMirror 6 示例：<https://codemirror.net/examples/>
- Obsidian 社区插件源码（quanru/obsidian-lifeos 是开源版 LifeOS）：<https://github.com/quanru/obsidian-lifeos>
- LYT 方法论：Nick Milo 的 "Linking Your Thinking" workshop 和文章
- Evergreen Notes：Andy Matuschak 的 <https://notes.andymatuschak.org/Evergreen_notes>
- ISO 8601 周编号说明

---

## 16. 变更记录

| 日期 | 版本 | 变更 |
| --- | --- | --- |
| 2026-04-18 | 0.1 | 初稿：基于 PARA + 五级周期笔记的设计 |
| 2026-04-18 | 0.2 | **重大结构调整**：改用 LYT/MOC 工作流；周期笔记只保留 Daily + Weekly；Tag 聚合页 Phase 1 就做；新增 ADR-0007/0008/0009/0010；更新所有目录名、IPC 命令、模板、路线图 |
| 2026-04-18 | 0.3 | 新增 `4-projects/` 顶层目录与 Project 模块（§2.3、§6.11）；frontmatter 扩展 `project` / `project-note` 类型；IPC 增 project_* 命令；SQLite notes 表加 project_slug 列；侧栏加 Projects 面板；Phase 1 扩展到 5 周；新增 `project.md` / `project-note.md` 模板；新增 ADR-0011 |
| 2026-04-18 | 0.4 | **V2 架构演进**：1. 取消 Frontmatter 中的项目跨域冗余声明 (废弃 project_slug)，实施基于文件树路径的精准单源推断 (SSOT)；2. 根除多端死锁隐患——SQLite引擎强制剥离至系统安全目录存储，开启事务加持WAL调优性能；3. 去除一切静默替换破坏原生 Markdown 源文件的占位注释行为，改用高阶抽象渲染衍生层展示；4. 引入无摩擦系统级 Ghost Window 极速捕获机制，及知识分解的重器“Inbox Block Extraction”段落提炼特性。妥破 iOS 端重存储生态。 |
| 2026-04-19 | 0.5 | 从 design.md 同步 §0.1 交付规范 + §17 交付清单（Week 3 收尾 + Week 4 Task 1–6 全部交付条目）；V2 架构约束（无 project_slug frontmatter、无 md 注入）保留不变 |
| 2026-04-19 | 0.6 | Week 5 Task 1 交付：通用 `status` 列 + `project_set_status` IPC + `index_projects_by_status` 查询 + 命令面板 4 个 `> Set project status → …` 命令。V2 约束澄清：`status:` 写入 frontmatter 属于"用户命令触发的编辑"，不违反 V2 "无静默注入" 约束；`project_slug` 字段仍弃用不动 |
| 2026-04-19 | 0.7 | Week 5 Task 2 交付：`src-tauri/templates/project.md` V2 对齐（`project_status` → `status`、`project_started` → `started`、删掉未绑定的 `target` 占位符行）；其它模板盘查确认已对齐，本轮不改 |
| 2026-04-19 | 0.8 | Week 5 Task 3 交付：`> New Project…` 命令 + 复用 New Note modal 的 `4-projects` 分支（slugify 标题 → 建 `4-projects/<slug>/index.md`，套 `project.md` 模板，`title:` 保留原始大小写）；`templateForDir` 加 `4-projects` 分支（`index.md` → project；其它 → project-note）；目录树展开逻辑补完"逐层祖先 expand"，修复嵌套路径新建后中间目录仍折叠的问题 |
| 2026-04-19 | 0.9 | **文档重构**：把 §17「交付清单」全部内容迁到仓库根目录 `delivery_log.md`（历史条目 Week 3 收尾 + Week 4 Task 1–6 + Week 5 Task 1–3 全迁，不做摘要删减）；design_V2.md §17 收缩为 5 行索引指针，只说"去哪看、怎么写回来"。§0.1 规则 3 处对"§17"的引用同步改成 `delivery_log.md`。动机：每条三段式交付 40–50 行 × 周增 3–6 条，design_V2 已逼近 1800 行，再写 2 周就超过了人类一次 Read 的心智上限，也稀释了架构章节的比重 |
| 2026-04-19 | 1.0 | Week 5 Task 3.5 交付：新增 `> Reseed templates from bundled` 命令——IPC `vault_reseed_templates` 按 bundled 字节 diff 决定 added / updated / unchanged 三桶覆盖 `vault/templates/*.md`，不动用户自定义模板；触发前有 `confirm()` 告警。动机：Task 2 的 Known gaps 在作者老 vault 上兑现（老 `templates/project.md` 没被 Task 2 的改动覆盖到），需要一个显式用户命令的迁移通道。V2 合规性：显式用户动作 + confirm 二道口，不是后台静默迁移 |
| 2026-04-19 | 1.1 | Week 5 Task 3.6 交付：修 CodeMirror `RangeError: Decorations that replace line breaks may not be specified via plugins`——把 frontmatter 折叠 chip 的跨行 `Decoration.replace` 从 `livePreview` ViewPlugin 拆成新 `frontmatterCollapse` StateField（加 `block: true`）；`FrontmatterWidget.toDOM()` 外层改 `<div>` 以匹配 block widget 要求。动机：CM 6 规定跨行 replace 只能从 StateField 发，ViewPlugin 发会抛 runtime RangeError |
| 2026-04-19 | 1.2 | Week 5 Task 3.7 交付（两个 hot-fix）：(A) `runReseedTemplates` 守卫字段名错写 `vaultState.rootPath`（实际是 `vaultState.current.path`），导致命令总是误判"未打开 vault"；顺手把 confirm / 结果展示从 `window.confirm` + `saveError`-tooltip 换成 Tauri `ask()`/`message()` 原生对话框。(B) `> Set project status → X` 改磁盘后当前编辑器不刷新——前端没有 `file-changed` 事件订阅通道，watcher 只刷 SQLite，需要在 IPC 成功后对 `openFilePath === index.md` 的情况显式 `fileRead → editorContent` 回填 |
| 2026-04-19 | 1.3 | Week 5 Task 4 交付：侧栏 Projects 面板（`ProjectsSection.svelte`）——4 个 status 子分组（active/paused/done/archived），active+paused 默认展开；数据源 4 路并发 `indexProjectsByStatus`；`refreshToken` bump 点：新项目创建、set-status、auto-save 后。挂载在 TagsSection 之前。样式/色板复用 TagsSection 的 token，无新增 CSS 变量 |
| 2026-04-19 | 1.4 | Week 5 Task 5 交付：项目"相关笔记"自动列表——新后端 IPC `index_project_notes(slug)`（path 前缀匹配，排除 `index.md` 自身，不读 `project_slug` frontmatter，V2 path-SSOT）；`Panel.svelte` 新增「项目笔记」section 放在 backlinks 之前，仅当前文件是 `4-projects/<slug>/index.md` 时渲染；空状态指引 `> Add Note to Project`（Task 6 命令）。子笔记视图暂不显示 siblings list（Known gap） |
| 2026-04-19 | 1.5 | Week 5 Task 6 交付：`> Add Note to Project` / `> Extract from project` 两条命令。Add 复用 New Note modal 的 targetDir 机制，新增 `4-projects/<slug>` 形态分支，slugify 标题生成 `<dir>/<note-slug>.md` + 套 `project-note.md` 模板；Extract 走 fileRead → rewriteFrontmatter(`type: note`) → fileWrite(1-notes/…) → fileDelete(src) 两步流，V2 合规不往 index.md 注入 `[[wiki-link]]`；`invalidateWikiCompletionCache` + `schedulePanelRefresh(200)` 清理 src 旧路径的补全缓存。Extract 碰撞自动 `-1` 递增（同 Promote）；Add 沿用既有"同名报错不覆盖"约定（不对称但故意） |
| 2026-04-19 | 1.6 | 开启 Phase 2：UI 质感美化与体验打磨（UI Beautification）。导入 Google Fonts `Inter` 统一全局字体，升级 `app.css` 提供克制低饱和度的 Premium 色板和层次阴影。在 Svelte 层加入 `backdrop-filter` 搭建 Command Palette 悬浮弹窗与 Modals 的毛玻璃 (Glassmorphism) 层，并全方位铺设 `fly` / `fade` 以及 `translateX` Hover 微动效。
| 2026-04-19 | 1.7 | UI 二步打磨（Claude 式极简美学融合）：接受用户重构输入，摒弃繁杂方块控件。全站调色盘采用三段式递进温润配色（侧边栏 `#F3F1EC` 骨瓷米白，主编辑器 `#F9F8F6` 珍珠白分离区隔）。主编辑器清除传统 IDE 行号及当前行高亮，设定 `780px` 纸张质感宽幅居中。应用界面所有 `button` 转为 Ghost Button，双链与 `#tag` 运用极简下划线 hover 替代胶囊底。右侧信息流通过独立封装重构为白色卡片悬浮样式（Card UI）。 |

---

## 17. 交付清单

逐任务的 **Scope / How to verify / Known gaps** 三段式记录已迁到仓库根目录的 **[`delivery_log.md`](./delivery_log.md)** —— 本章节只保留索引指针，避免 design_V2.md 过度膨胀。

- 新任务启动前：扫读 `delivery_log.md` 顶部最近 2–3 条交付记录（倒序排列）。
- 交付时：把本次任务的 Scope / How to verify / Known gaps 写到 `delivery_log.md` 顶部；同时在 §16 「变更记录」补一行版本号。
- 若某次交付里包含对架构决策的修改：**先改 design_V2 对应章节（§5 / §6 / §10），再回来写 delivery_log**。
- 交付规范（三段式结构、倒序、不复述全局架构等）仍以 §0.1 为准。
