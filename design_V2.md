---
title: MyNotes 设计文档 V2
status: draft
version: 0.8
created: 2026-04-18
updated: 2026-04-21
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

| 不做                          | 理由                                                      |
| ----------------------------- | --------------------------------------------------------- |
| 自建云同步服务                | 等同于做第二个产品；iCloud/Syncthing/Git 已够用           |
| 多人协作                      | 定位是个人；协作必然引入 CRDT + 账号                      |
| WYSIWYG 富文本（Notion 风格） | 坚持纯 Markdown                                           |
| 自研插件生态                  | 初期没必要                                                |
| 兼容 Obsidian 插件格式        | 会严重绑死架构                                            |
| 全文移动端原生编辑体验        | 移动端只管"捕获 + 浏览"，深度编辑留桌面                   |
| PARA/层级分类                 | 参见 §1.4（但保留一个轻量 `4-projects/` 作折中，见 §2.6） |

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

| 文件夹     | 作用                                   | 笔记的共同特征                                         |
| ---------- | -------------------------------------- | ------------------------------------------------------ |
| `0-inbox/` | 捕获区。随手记，不考虑命名和分类       | 通常文件名带时间戳，内容零散、未定型                   |
| `1-notes/` | 已消化的原子笔记。"一篇笔记只讲一件事" | 有 well-formed 标题、自洽、可被其他笔记链接            |
| `2-moc/`   | Maps of Content。手工编写的主题枢纽页  | 通过 `[[...]]` 把 notes 里的相关笔记组织成一个可读索引 |

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

| 问题                                                           | 放哪                                  |
| -------------------------------------------------------------- | ------------------------------------- |
| "Markdown 比专有格式更利于长期保存"（一个独立可读的观点/事实） | `1-notes/`                            |
| "MyNotes 这个项目第二周要做模板引擎"                           | `4-projects/my-notes/index.md`        |
| "关于组件化拆分的选型讨论"（项目内部讨论）                     | `4-projects/my-notes/组件拆分讨论.md` |
| "组件化思想本身的原理"（知识性陈述）                           | `1-notes/` + 被 `2-moc/` 引用         |

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

| 层       | 技术                 | 版本锚           |
| -------- | -------------------- | ---------------- |
| 外壳     | Tauri                | 2.x              |
| 前端     | SvelteKit / Svelte 5 | 2.x / 5.x        |
| 打包     | Vite                 | 5.x              |
| 编辑器   | CodeMirror           | 6.x              |
| 索引 DB  | SQLite + FTS5        | rusqlite bundled |
| 文件监听 | notify (Rust)        | 7.x              |
| 日期     | date-fns / chrono    | latest           |
| 图标     | lucide-svelte        | latest           |

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
│   └─ YYYY/MM/            # 按年月子目录归档（粘贴/拖放时自动建）
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

| 场景           | 约定                                    | 示例                                                                                       |
| -------------- | --------------------------------------- | ------------------------------------------------------------------------------------------ |
| Inbox 条目     | `YYYY-MM-DD-HHmmss-{slug}.md` 或无 slug | `2026-04-18-143012-random-idea.md`                                                         |
| 正式笔记       | `{well-formed 标题}.md`，允许空格和中文 | `为什么-Markdown-比富文本更适合长期保存.md` 或 `为什么 Markdown 比富文本更适合长期保存.md` |
| MOC            | `{主题}.md`                             | `2-moc/个人知识管理.md`                                                                    |
| Daily          | `YYYY-MM-DD.md`                         | `2026-04-18.md`                                                                            |
| Weekly         | `YYYY-Www.md`（ISO 8601）               | `2026-W16.md`                                                                              |
| Project 主页   | `4-projects/{slug}/index.md`            | `4-projects/my-notes/index.md`                                                             |
| Project 子笔记 | `4-projects/{slug}/{任意标题}.md`       | `4-projects/my-notes/架构取舍讨论.md`                                                      |

**命名建议**：正式笔记标题尽量是**一个能独立读懂的陈述/名词短语**（Evergreen Notes 原则）。好标题："Markdown 的长期可维护性优于专有格式"；坏标题："笔记格式想法"。

### 4.3 Frontmatter Schema

```yaml
---
title: string # 可选；缺省从 H1 或文件名推断
type: inbox|note|moc|daily|weekly|project|project-note
status:
  draft|evergreen|archived # type=note：draft/evergreen/archived
  # type=project：active/paused/done/archived（见下）
created: 'YYYY-MM-DD HH:mm'
updated: 'YYYY-MM-DD HH:mm' # 保存时自动更新
tags: [tag1, tag2]
aliases: [other-name] # 用于 wiki link 别名

# 周期笔记特有
period: '2026-W16' # 规范化周期 id

# MOC 特有
moc_scope: '知识管理' # 可选，人类可读的主题描述

# Project 特有（仅 type=project 的 index.md）
project_status: active|paused|done|archived
project_started: '2026-04-18'
project_target: '2026-06-30' # 可选，预期完成日期
project_owner: 'self' # 可选，多人时用


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
title: '{{date:YYYY年MM月DD日}} ({{date:ddd}})'
type: daily
period: '{{date:YYYY-MM-DD}}'
created: '{{now}}'
updated: '{{now}}'
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

// Attachments (Phase 2 Task 3)
attachment_save(bytes: Vec<u8>, original_name: Option<String>, ext: String) -> String
// 把二进制写入 attachments/YYYY/MM/<filename>.<ext>，文件名 = YYYYMMDD-HHmmss-<slug|rand>.<ext>，
// 返回**相对 vault 的** rel_path（前端拿来插到 markdown 里作 `![alt](rel_path)`）。
attachment_read_bytes(rel_path: String) -> Vec<u8>
// 读取 attachments/... 下的二进制字节；前端包成 Blob URL 给 <img src>。
// 路径越界拒绝（只允许 rel_path 以 "attachments/" 开头）。
attachment_list() -> Vec<AttachmentInfo>
// 全量扫描 vault/attachments/**，返回 {rel_path, size, mtime}；不递归进符号链接。
attachment_unreferenced() -> Vec<AttachmentInfo>
// 返回"实际存在但没有任何 md 文件 link_type='embed' 指向"的附件列表
// 实现 = attachment_list() 的 rel_path 全集 减去 SELECT DISTINCT dst_resolved FROM links WHERE link_type='embed'。
attachment_delete_batch(rel_paths: Vec<String>) -> Vec<String>
// 批量删除（orphan cleanup），返回实际删除成功的 rel_paths 子集。每条走 resolve_in_vault 路径检查。

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
          → 提取 wiki link [[...]]         → links 表 link_type='wiki'
          → 提取 markdown image ![...](path) → links 表 link_type='embed'（见 §6.12）
          → 提取 task - [ ] / - [x]
          → 提取 H1 作为 title（缺省时）
          → 写入 notes / tags / links / tasks / notes_fts
```

**Markdown 图片解析**（Phase 2 Task 3 新增）：对 body 里形如 `![alt](rel_or_abs_path)` 的标准 markdown 图片语法，提取其 `path` 为 `dst` / `dst_resolved`（相对 vault 根路径时原样写入，便于 `attachment_unreferenced` 做集合差集）。只在 `path` 不是 HTTP(S) URL 时才算本地附件；远程 URL 不进 links 表。不动 `![[wiki-embed]]`（Phase 2 暂不支持这种语法）。

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

| 入口                     | 交互                                                                                                                                                                  |
| ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 全局捕获 `Cmd+Shift+N`   | （利用 `tauri-plugin-global-shortcut`）呼出系统级 Ghost Window/Spotlight 风格悬浮输入框。即使主界面最小化关闭，输入回车即保存入 0-inbox，将思维捕捉的全局摩擦降为 0。 |
| 命令面板 "Quick Capture" | 同上                                                                                                                                                                  |
| 右键系统托盘（Phase 2）  | 全局可用                                                                                                                                                              |

新建的 inbox 文件格式：

```markdown
---
type: inbox
created: '2026-04-18 14:30'
updated: '2026-04-18 14:30'
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

**交互流程 2：段落提取 (Block-level Extraction)**（Phase 2 Task 6 已交付，知识重构的杀手级捷径）：

适用范围：**任何 .md 文件**，不限于 inbox——长 daily 笔记里提炼一个想法、project spec 里抽一段、1-notes 里的长笔记拆成原子都是合法场景。唯一排除的是 `.mynotes/` 下的内部管理文件。

1. 在编辑器里选中一段文字，或让光标落在某段内部（非空行），按 `⌘⇧E` 或命令面板 `> Extract selection → new note`；
2. 若 selection 为空，Editor 的 `expandToParagraph()` 把光标所在段（以空行为边界向上/向下扩展）设为 selection 并视觉高亮；若光标在空行则 toast 提示 "no block at cursor" 不弹 modal；
3. Modal 弹出：标题输入框（默认值由首行 markdown heading 剥壳后截 80 字符得出）+ 只读 preview 区（前 240 字，pre-wrap 保段落结构）；用户可改标题；
4. Confirm → `extractBlockToNote(title, text)`：`slugifyTitle(title)` 生成 slug，循环找 `1-notes/<slug>[-N].md` 的空位写盘（套 `note.md` 模板，frontmatter `type: note / tags: []`，body `# title` + extracted 原文）→ 返回 `{ dstPath, linkText: "[[title]]" }`；
5. 前端 `editorApi.dispatchReplace(capturedRange, linkText)` 原子替换选区为 wiki-link——**捕获的 range 在 modal 打开时就冻结**，不跟 live selection 走（用户在 modal 里改输入 / 光标游走都不影响 splice 位置）；单个 CM6 transaction 意味着 `⌘Z` 一次就能撤销"新建笔记 + 替换为链接"这整步操作。
6. 任何 IO 失败 → toast error，modal 保留给用户重试，**源文件不动**（先写新笔记成功、再 dispatch 源端 edit——失败回滚只需考虑"孤立新文件"，用 `> Find unused attachments` 的姊妹机制扫即可；严格事务性留作后续优化）。

**后端接线**：复用 Phase 1 已有的 `file_exists` / `file_write` IPC，不引入新命令；frontmatter 生成走现有 `buildExtractedNote(extractedText, title, now)` 纯函数；index 更新走 watcher（`notify-rs` 捕获新文件后自动 insert）。

**与整篇 Promote 的差异**：Promote 是"搬文件 + 改 frontmatter"，源文件消失；Extract 是"复制段落到新文件 + 把段落原位替换为 wiki-link"，源文件保留、体积缩小。两条命令各有适用语境，不重叠。

**已知边界**：

- `linkText` 当前按 `[[title]]` 给出，slug 冲突时 dstPath 是 `<slug>-2.md` 但链接文本仍是 `[[title]]`——通过 wiki-link 解析器可能指向既有同名笔记。严格正确性要 `[[<slug>-2|title]]` 的 alias 形式，下一轮 polish。
- `expandToParagraph` 仅按空行切段，不识别 heading 分界；从 heading 正下方 extract 可能把 heading 一起扩进来。规则简单是故意——这命令的意图是"提炼我眼前这段话"，heading 作为段落一部分被一并提取基本符合预期。

**链接重写**（Phase 2 Task 4 已交付）：`file_move_with_refs` 重写所有 `[[wiki]]` 和 `![](path)` 引用。Extract 本身不触发 rename 链路（新文件从 extract 字面生成，不是 move），所以不需要 RewritePlan。

### 6.4 MOC 创建与维护

**MOC 是纯手工维护的文件**——应用不自动生成 MOC 内容，只提供创建和跳转便利。

**创建 MOC**：

命令面板 "New MOC" → 输入主题名 → 基于 `templates/moc.md` 创建 `2-moc/{主题}.md`。默认模板：

```markdown
---
title: '{{title}} · MOC'
type: moc
created: '{{now}}'
updated: '{{now}}'
tags: [moc]
moc_scope: '{{title}}'
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

**MOC 辅助建议**（Phase 2 Task 7）：基于 tag 一键生成一个 MOC 草稿——把打了该 tag 的每一条笔记预先写成 `- [[title]]` 放进 `## 核心笔记`，用户只需要再决定哪些保留、哪些挪到"延伸阅读"。

触发入口：

- **命令面板** `> Build MOC from tag…`。仅在当前 TagView 已打开某个 tag 时可见（`when: ctx.activeTag !== null`）。
- **TagView 头部** "建 MOC" 按钮。只要 tag 下有笔记就可用；数量为 0 时置灰。

预览 modal（`src/routes/+page.svelte` → `mocBuilder*` 一族状态）逻辑：

1. 打开时调用 `indexNotesByTag(tag)` 取所有带该 tag 的笔记；默认全选，以便"把整个 tag 转成 MOC"是一步操作。
2. 用户在列表里勾选子集 + 填写 MOC 标题（默认与 tag 同名）。
3. 确认后走前端 `buildMocFromTag(deps, { tag, title, noteRefs })`（`src/lib/commands.ts`）：

   ```ts
   const slug = slugifyTitle(title);
   let dstPath = `2-moc/${slug}.md`;
   for (let i = 1; (await fileExists(dstPath)) && i < 100; i++) {
     dstPath = `2-moc/${slug}-${i}.md`;
   }
   await createNoteFromTemplate(dstPath, { title }); // 正常走模板
   const body = await fileRead(dstPath);
   const lines = noteRefs.map((ref) => `- [[${ref.title ?? stem(ref.path)}]]`);
   const next = body.replace(
     /## 核心笔记\r?\n\r?\n- \[\[\]\]/,
     `## 核心笔记\n\n${lines.join('\n')}`
   );
   await fileWrite(dstPath, next);
   // 再 rewriteFrontmatter(..., { moc_source_tag: tag }) 追踪来源
   ```

**关键设计决定**：

- **wiki-link 用 `[[title]]` 而不是 `[[2-moc/slug]]`**——§5.4 的解析器一等支持按 title 查找；裸标题在 MOC 上可读性更好。若两篇同名，解析器按路径字典序挑确定性第一名，用户可重命名其中一篇消歧义。
- **不复用 New MOC 命令逻辑里的模板加载**——`buildMocFromTag` 直接调 `createNoteFromTemplate` 走同一路径，确保模板/frontmatter schema 永远与手工创建的 MOC 一致。stub 替换用正则命中 `## 核心笔记\n\n- [[]]`；若以后模板改形状而正则失配，MOC 仍然被创建，只是不注入——前端走"已创建但未注入笔记"软降级，不抛错。
- **`moc_source_tag` frontmatter**——附加字段，写入来源 tag。目前仅作审计；未来可以据此做"重建 MOC"或在 Panel 显示"该 MOC 来自 #tag"。
- **碰撞命名**：与 Promote/Extract 对称——`2-moc/<slug>-1.md`、`-2.md`……最多尝试 100 次，否则抛错。用户看到"找不到空闲文件名"后应先重命名冲突 MOC。

### 6.5 周期笔记（简化版）

**核心命令**：

| 命令                      | 快捷键            | 行为                                             |
| ------------------------- | ----------------- | ------------------------------------------------ |
| Open Today's Daily Note   | `Cmd+D`           | 打开今日 daily，没有就从 `templates/daily.md` 建 |
| Open This Week            | `Cmd+Shift+W`     | 同上 weekly                                      |
| Open Yesterday / Tomorrow | —                 | 相对今天                                         |
| Previous / Next Daily     | `Alt+[` / `Alt+]` | 在当前 daily 基础上前后翻                        |
| Jump to Date...           | `Cmd+G`           | 输入日期/周数跳转                                |

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

| 变量                               | 值                                         |
| ---------------------------------- | ------------------------------------------ |
| `{{now}}`                          | `2026-04-18 14:30`                         |
| `{{date}}` / `{{date:YYYY-MM-DD}}` | `2026-04-18`                               |
| `{{date:YYYY年MM月DD日}}`          | `2026年04月18日`                           |
| `{{date:ddd}}`                     | `Sat`                                      |
| `{{week}}`                         | `2026-W16`                                 |
| `{{year}}`                         | `2026`                                     |
| `{{title}}`                        | 用户输入的标题                             |
| `{{filename}}`                     | 最终文件名（不含扩展）                     |
| `{{prev}}`                         | 上一篇同粒度周期笔记 id（如 `2026-04-17`） |
| `{{next}}`                         | 下一篇                                     |
| `{{uuid}}`                         | 随机 UUID                                  |

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

| 前缀   | 行为                                        |
| ------ | ------------------------------------------- |
| 无前缀 | 文件 fuzzy search（匹配 title 和 filename） |
| `>`    | 命令 fuzzy search                           |
| `#`    | tag 搜索                                    |
| `/`    | 全文搜索（走 SQLite FTS5）                  |

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

### 6.10 图谱视图（Phase 2 · Task 5）

扁平结构 + 双链的知识库，图谱比目录树更能展示"哪些笔记在说同一件事"。Phase 1 用"反向链接 + Tag 聚合页"挡住了基本需求，Phase 2 Task 5 把图谱做成一等公民。

#### 6.10.1 范围与非范围

**做**：

- 全局图（整个 vault 的已解析链接 + 节点）。
- 局部图（以当前打开的笔记为种子，N 跳邻域 BFS 子图，N ∈ [1,4]）。
- 按 `note_type` 着色、类型过滤、搜索高亮、缩放/平移/节点拖拽、点击节点打开笔记。
- Canvas 渲染，配力模拟（d3-force）——**一次到位选 canvas 而不是 SVG**，避免日后百节点时被迫重写。

**不做（留 Phase 3）**：

- 社群/聚类检测（Louvain / Leiden）；
- 节点位置持久化（`.mynotes/graph-layout.json`）；
- 未解析链接在图里的"幽灵节点"可视化（orphan links 面板已覆盖）；
- 无障碍键盘导航 / 屏幕阅读器镜像；
- 导出图为 PNG / SVG。

#### 6.10.2 数据层：`index_graph` IPC

**签名**：`index_graph() -> GraphData { nodes: GraphNode[], edges: GraphEdge[] }`

```
GraphNode { path, title, note_type, in_degree, out_degree }
GraphEdge { src, dst, link_type }    // dst 永远非空，后端已过滤 dst_resolved IS NULL
```

**一次读全图，不支持增量**。本地 vault（数千节点、数万边）序列化后 <1 MB，全量 pull 比 "open 一次 + 每次改动 diff 推送" 简单得多；前端打开图谱视图时 lazy 加载，后续 `refreshToken` bump 时重新 pull。

**degree 计算在 Rust 侧**：避免对 `links` 表做 `GROUP BY` 第二次扫描，直接一遍 loop 累计，且**两端节点都存在才计数**——indexer 暂时落后（renameded 途中）的悬挂边不应污染 src 的 out_degree。

**不返回未解析边**：`WHERE dst_resolved IS NOT NULL`。原因：这些边没有目标节点能画，backlinks 面板和 "> Unresolved links" 命令已经单独承接"谁还没 link 通"的发现场景。

#### 6.10.3 渲染层：为什么 Canvas 而不是 SVG

|             | SVG                 | Canvas                    |
| ----------- | ------------------- | ------------------------- |
| 节点上限    | ~1k（DOM 节点爆炸） | ~10k（quadtree 命中测试） |
| 每帧改写    | 修 DOM 属性         | 清屏 + 全量 repaint       |
| 命中测试    | 原生事件冒泡        | 手动 quadtree             |
| 代码复杂度  | 低                  | 中                        |
| 从 A 迁到 B | -                   | 300-400 行重写            |

Phase 2 直接上 Canvas。本地 vault 到了 500+ 节点 SVG 就开始掉帧（实测 Obsidian 也是这个分界），再做一次迁移就是白干几天。

**dpr 感知**：`canvas.width = cssWidth × dpr`，ctx 应用同样系数；strokeWidth 除以 zoom k 保持 1 device-px（否则缩放到 5x 时边变成 5 像素糊一团）。

**双坐标系**：

- 节点 + 边走世界坐标（随 zoom 缩放）；
- 标签走屏幕坐标（constant font size，用 `transform.applyX/Y` 手动投影）。
- 目的：避免 12px 字体被 zoom 放大到 48px 糊屏；Obsidian 用同样套路。

**hover 邻域高亮**：未 hover 时边 α=0.6；hover 时全局 α=0.25，该节点的相邻边再画一遍加粗 + accent 色。视觉焦点明确。

#### 6.10.4 力模拟：d3-force 四力

```ts
forceLink.distance(48).strength(0.5)   // 理想弹簧长度 ~48 世界单位
forceManyBody.strength(-180)           // 排斥力
forceCollide.radius(r => 6 + 1.6√deg)  // 重叠避免（与节点视觉半径一致）
forceCenter(0,0).strength(0.04)        // 轻微向心
alphaDecay(0.03)                       // ~100 tick 冷却
```

初始位置按黄金比例 jitter 撒在半径 260 的环上——避免完美圆让 d3-force 多做功（完美对称的系统收敛得极慢）。

拖拽：mousedown 时 `node.fx = node.x; node.fy = node.y` 钉住；mouseup 时两者 `= null` 让节点自然回到模拟。drag 过程中 `layout.reheat(0.4)` 重启模拟让邻居联动。

#### 6.10.5 局部模式：BFS 子图

边按**无向**走（"谁指我 + 我指谁"两边都看），从 `currentFilePath` 广度优先扩 N 跳。seed 不在图里（刚创建的新笔记、indexer 落后）时返回空图而非崩溃。

"隐藏孤儿"开关在局部模式下**仍保留 seed**（"我在这里"）——即使当前笔记没有任何 link 也至少能看到自己的位置。

#### 6.10.6 命中测试：d3-quadtree 懒建

四叉树 **不在每个 tick 重建**——每 tick `markLayoutDirty()` 只置脏位，真正 `pick()` 时才 `quadtree().addAll(nodes)`。每帧 O(n log n) 重建是无效开销（100 fps × 1000 节点 ≈ 10 万次/秒的 tree 操作）。

命中流程：

1. `transform.invert(mouseClientPos)` → 世界坐标；
2. `quadtree.find(wx, wy, 30/k)` 拿半径内最近候选；
3. 精确半径校验（视觉半径 + 2px slop）；miss 返回 null。

#### 6.10.7 d3-zoom × d3-drag 冲突处理

两者都想吃 mousedown。解法：

- `zoomBehaviour.filter` 里调 `renderer.pick(event.clientX, event.clientY)`，命中节点时**返回 false**（把 mousedown 让给 drag）；wheel 永远放行。
- `dragBehaviour.filter` 里反过来：只在 pick 命中时返回 true。

这比"listener 顺序靠手动调"可靠得多。

#### 6.10.8 入口与快捷键

- 命令面板 `> Open Graph View`（hint `⌘⇧G`）。
- 全局快捷键 `⌘⇧G`（`⌘G` 保留给未来的 Jump-to-Date）。
- 点击图中节点 → `activeView = null` 并 `openFile(path)`，"进入节点"的感觉是横向跳转，不是堆栈式打开新 tab。
- 侧栏暂不做"图谱"入口——保持侧栏的一级目录极简。

#### 6.10.9 依赖与包体

| 包           | 用途             | gzip   |
| ------------ | ---------------- | ------ |
| d3-force     | 力模拟           | ~16 KB |
| d3-zoom      | 缩放/平移        | ~7 KB  |
| d3-drag      | 节点拖拽         | ~5 KB  |
| d3-selection | zoom/drag 的前置 | ~10 KB |
| d3-quadtree  | 命中测试         | ~4 KB  |

共约 **42 KB gzipped**。故意不装 `d3-transition`（~12 KB）——图里的动画只有搜索框回车"跳到命中节点"一个场景，改成瞬间跳转，省 12 KB。

#### 6.10.10 V2 原则对齐

- **Path-SSOT**：图里的每个节点 id 都是 vault-relative path；没有任何频道引入"第二个 id 体系"。
- **SQLite 是派生索引**：`index_graph` 只读 `notes + links` 两张表，不写；重建索引后图谱内容自然更新。
- **CSS var 主题一致**：`colorsFromCss` 从 `--color-accent` / `--color-fg-muted` / `--color-warning` / `--color-bg` 等 token 读色，light/dark 切换无需自写调色盘。**当前实现**：图谱 view 已监听 `document.documentElement[data-theme]` 变化，并在 `system` 模式下补 `prefers-color-scheme` 监听；light/dark 切换会原地重绘，不需要重新打开图谱视图。

### 6.11 Project 模块

**设计目标**：给"有具体目标、有截止时间、会产生一系列相关笔记"的工作一个容纳空间，同时不破坏 LYT 知识库的纯度。

#### 6.11.1 创建项目

命令面板 `> New Project...` → 弹窗输入：

| 字段           | 必填 | 示例               |
| -------------- | ---- | ------------------ |
| Slug（目录名） | ✓    | `my-notes`         |
| Title          | ✓    | `MyNotes 笔记应用` |
| Target date    | ○    | `2026-06-30`       |

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

|          | Project (index.md)           | MOC                    |
| -------- | ---------------------------- | ---------------------- |
| 范围     | 单个具体项目的上下文         | 一个长期主题的知识索引 |
| 生命周期 | 有开始有结束                 | 永恒，随知识积累演化   |
| 位置     | `4-projects/{slug}/index.md` | `2-moc/{topic}.md`     |
| 相关笔记 | 机器自动列（同目录）         | 人手写 `[[...]]`       |
| 状态字段 | active/paused/done/archived  | 无                     |

但可以互相引用：项目 index.md 可以在"延伸阅读"里链一个 MOC；MOC 也可以反向链接一个已完成项目（作为知识来源）。

#### 6.11.7 归档与清理

项目进入 `archived` 后：

- 默认从侧栏 Active 组隐藏（仍可在 "Archived" 组下展开）；
- 首页和命令面板默认不列出；
- 仍参与全文搜索和 tag 聚合（知识还在里面）；
- 不自动移动文件位置——保留 `4-projects/{slug}/` 原位，避免破坏已建立的 wiki 链接。

（可选：Phase 2 的 "Move to .mynotes/archive/" 动作，彻底归档到用户看不到的位置。）

### 6.12 附件与图片（Phase 2 Task 3）

**设计目标**：把"写作时插图片 / 拖文件"这一刚需封闭在 vault 内部，不引入云服务；同时给出一个简单的"孤儿附件清理"通道，避免 vault 被冗余文件淹没。

#### 6.12.1 存储结构与命名

- 目录布局：**`vault/attachments/YYYY/MM/`**（按"附件添加的年/月"自动归档）。这样单月文件数可控，也方便用户按时间回溯找图。
- 文件命名：**`YYYYMMDD-HHmmss-<slug|rand>.<ext>`**
  - 粘贴剪贴板图片（没有原文件名）→ 用 6 位随机 hex 作后缀：`20260419-143012-a1b2c3.png`。
  - 拖放文件（有原文件名）→ 用 slugify(原文件名) 作后缀：`20260419-143012-architecture-diagram.png`。
- 扩展名由前端探测（剪贴板 `clipboardData.items[i].type` / 拖放 `file.type` / `file.name` ext）决定，后端只按字符串处理。

**不做**：图像 hash 去重 / 压缩 / 重编码——保留原字节是 V2 "不静默修改用户数据" 的延伸（ADR-0003 精神）。

#### 6.12.2 引用格式：标准 Markdown `![alt](rel_path)`

Markdown 原生语法，兼容 Obsidian / GitHub / 所有通用渲染器。**不用** Obsidian 的 `![[embed]]`——理由：

1. `![[...]]` 是 Obsidian 方言，离开 Obsidian 就渲染不出；
2. 我们的索引器已经处理 `[[...]]` wiki link，复用成本低，但"wiki embed"语义复杂（可指向 md/png/pdf/audio/video 各种类型），Phase 2 不想承担这个复杂度；
3. 标准 markdown 图片语法配合前端 CM6 widget，已能覆盖"看到缩略图"的核心体验。

插入的样式：`![optional-alt](attachments/2026/04/20260419-143012-foo.png)`。alt 文本：拖放用原文件名 basename，粘贴留空。

#### 6.12.3 渲染方案：IPC 字节 + Blob URL

Tauri 2 提供 `asset:` 协议，但需要在 `tauri.conf.json > security.assetProtocol` 里预先静态声明可访问范围，对"任意用户 vault 路径"不友好。**决策**：改走 IPC 读字节 → 前端 `URL.createObjectURL(new Blob([bytes]))` → `<img src="blob:…">`。

- 路径安全：`attachment_read_bytes` 内部用 `resolve_in_vault` 做规范化，越界路径直接 reject。
- 缓存：前端在 widget 生命周期内缓存 `rel_path → blob URL`，widget 销毁时 `URL.revokeObjectURL`。
- 代价：首次加载每张图多一次 IPC round-trip；可接受（单图一般 <1MB，本地调用 <10ms）。

#### 6.12.4 插入时机与交互

| 入口                                     | 触发                                                                                                                         | 行为                                                                                                                                                                                                          |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **剪贴板粘贴（image MIME）**             | 编辑器聚焦 + `Cmd/Ctrl+V`，`clipboardData` 含 image/\*                                                                       | 调用 `attachment_save(bytes, None, ext)` → 拿到 rel_path → 光标处插入 `![](rel_path)` + `\n`，阻止默认粘贴                                                                                                    |
| **剪贴板粘贴（file:// / 绝对路径文本）** | 部分来源（如微信桌面、某些文件管理器）剪贴板里只有 `text/uri-list` 或 `text/plain`，值是 `file://…` 或 `/Users/…` 形态的路径 | 解析文本；若能看出是 image 扩展名，走 `attachment_read_external_bytes(abs)` 读字节 → 再 `attachment_save` 归档 → 插入 `![](attachments/…)`。非 image 路径或普通文本会 fall through 到 CM 的默认粘贴，不吞输入 |
| **拖放文件**                             | 从 Finder/资源管理器拖 image 文件到编辑器 DOM 内                                                                             | 多文件逐个 save，全部插成 `![basename](rel_path)`，用 `\n` 分行                                                                                                                                               |
| **拖放（仅路径文本）**                   | 某些 WKWebView 场景 drag 只带 `text/uri-list`、不带 File                                                                     | 同粘贴回退：外部 IPC 读字节 → 归档 → 插入 `![](attachments/…)`                                                                                                                                                |
| **拖放非 image**                         | 拖 pdf / zip / md 等到编辑器                                                                                                 | 仅处理 image MIME / image 扩展名。非 image 忽略，避免意外把 .md 丢进 attachments/                                                                                                                             |
| **手打绝对路径**                         | 用户直接敲 `![alt](/Users/…/foo.png)` 或 `![alt](file:///…)`                                                                 | 不自动归档（不动用户文本）；widget 走 `attachment_read_external_bytes` 读外部文件渲染缩略图。原文件搬走后预览会变成"⚠ 无法加载图片"                                                                           |

**CM6 集成点**：`EditorView.domEventHandlers({ paste, drop, dragover })`——复用 `wikiLinkClickHandler` 已有的 pattern。dragover 必须 `preventDefault()` 才能让 drop 生效。

**Tauri 原生 drag-drop 拦截**：Tauri 2 默认会在 OS 层捕获拖入窗口的文件事件（`dragDropEnabled: true`），DOM 的 `drop` 事件永远不会被触发，导致"拖图进编辑器"无效。我们在 `tauri.conf.json` 里显式设 `dragDropEnabled: false`，把拖放语义完全交给 DOM——代价是失去 Rust 侧的 drag-drop 事件，但我们用不到，反而更简单。

**外部路径 IPC（`attachment_read_external_bytes`）**：WKWebView 拒绝从 `http://localhost` 起源 `<img>` 加载 `file://` 资源，所以外部图片（手打路径 / 回退粘贴）必须走 Rust 侧读字节再包 Blob URL。后端硬约束：

- 只接受绝对路径；
- 扩展名必须在 image 白名单（png/jpg/jpeg/gif/webp/svg/bmp/avif/heic/heif）；
- 文件大小 ≤ 50 MB；
- 不是 attachment 区专用 IPC——它是个独立命令，语义明确：「读取任意外部图片字节给编辑器预览 / 归档用」。

#### 6.12.5 缩略图预览 Widget

在编辑器里，只要某行**整行都是 `![...](<path>)`**、且 `<path>` 是下面三种之一，下方渲染一张缩略图（block widget，max-width: 520px, max-height: 360px, object-fit: contain, 圆角 6px, 轻阴影）。行内 raw markdown 文本保持可见可编辑——不做 replace，只是"附加"一个 block 在行下方。

| 路径形态       | 正则片段        | 读字节的 IPC                              |
| -------------- | --------------- | ----------------------------------------- |
| 仓库相对附件   | `attachments/…` | `attachment_read_bytes`                   |
| POSIX 绝对路径 | `/…`            | `attachment_read_external_bytes`          |
| `file://` URI  | `file://…`      | 去协议 → `attachment_read_external_bytes` |

远程 URL（`http(s)://`）故意不入——避免编辑器里出现无预期的网络请求。

**CM6 实现要点**：

- 用 `StateField<DecorationSet>` 而非 `ViewPlugin`——块级装饰（widget block: true）必须走 StateField（CM6 硬性约束，`ViewPlugin` 只能发 span 级 replace / mark）。
- 图片加载：widget `toDOM()` 时拿 `path` 问前端"图片字节缓存 map"；cache miss 时按上表选 IPC，收到字节后 `createObjectURL` 再设 `img.src`。
- `file://` 解码：用 `decodeURIComponent`，处理中文 / 空格的百分号编码。
- 重建：doc 变化时 StateField 读取 update.changes，对改动行附近重算装饰；为简化，第一版做"每次 docChanged 全文重扫 embed 行"，性能够用（单笔记一般 <100 张图）。

#### 6.12.6 孤儿附件清理

**命令面板** `> Find unused attachments`：

1. 调 `attachment_unreferenced()`。后端实现：
   ```
   all_files       = walk_dir(vault/attachments/**, only files)
   referenced      = SELECT DISTINCT dst_resolved FROM links WHERE link_type='embed' AND dst_resolved IS NOT NULL
   orphans         = all_files - referenced
   ```
2. 前端弹 Modal 显示 orphan 列表（rel_path + size + mtime），用户勾选（默认全选） → 点击 "Delete N files"；
3. 调 `attachment_delete_batch(rel_paths)`；
4. 成功后 Modal 显示"已删除 N 个文件"。

**边界**：

- 当前编辑器里有未保存的 `![](...)` 插入还没写到磁盘时，那张附件对 DB 来说是 orphan。用户应先保存再跑清理，或在 Modal 里手动取消选中。Phase 2 Task 3 不做主动保护，只在 Modal 顶部加提示 "可能会误删刚插入未保存的附件，请先 `Cmd+S` 保存"。
- 软删除：暂不做。批量删除用 `std::fs::remove_file` 直删。想反悔就从回收站找回（macOS Trash / Windows 回收站会接住，因为 Rust `remove_file` 在桌面环境遵循系统回收站规则？**实际是不会** — `remove_file` 是真删。Phase 2 Task 3 不引入系统 trash 依赖，用户需自行谨慎。Modal 有"全选 → 二次确认"做 UX 兜底）。

#### 6.12.7 V2 合规性自检

- **Path-SSOT**：附件的 `rel_path` 就是文件系统位置，不存第二份真相。
- **无 md 注入**：除用户粘贴/拖放这种显式动作外，不后台改动用户 md。
- **索引可抛弃**：link_type='embed' 行的记录会在 `index_rebuild()` 时重建，不是单独写入。
- **数据自持**：卸载 MyNotes 后，`attachments/` 目录原样留在用户 vault 里，md 里的 `![](attachments/...)` 在 Obsidian 或任意 md 渲染器里都能看。

### 6.13 链接重写（Rename With Refs，Phase 2 Task 4）

**设计目标**：当用户重命名或移动一个 `.md` 文件时，所有指向它的 `[[wiki-link]]` 和 `![alt](path)` 引用都应自动跟随更新，而不是留一堆 dangling links 让用户手工修。Obsidian 默认开启的 "Automatically update internal links"，我们补齐这一块。

#### 6.13.1 范围与非范围

**本版本处理**：

- 单文件 path 重命名：`1-notes/foo.md` → `1-notes/bar.md`；
- 单文件跨目录移动：`0-inbox/foo.md` → `1-notes/foo.md`；
- 同时改名 + 改目录：`0-inbox/foo.md` → `1-notes/bar.md`。

**本版本不处理**（明确留作 Phase 3 或更晚）：

- 目录重命名（`1-notes/` → `notes/`）—— 会影响成百上千文件，需要批量事务 + 进度条 + 回滚，超出本版本复杂度预算；
- 标题（frontmatter `title:`）改名触发的重写 —— 标题改名的语义歧义大（用户可能只是改个 display name，也可能是想改 wiki-link key）；Promote 场景里我们显式 rewrite frontmatter 但不 propagate 到引用方；
- 附件（`attachments/**/*.png`）重命名 —— 附件的 rel_path 在粘贴时就由后端生成，用户通常不会改。如有需求后续专项处理。

#### 6.13.2 后端命令 `file_move_with_refs`

签名：`file_move_with_refs(from: String, to: String) -> RenameResult`

```rust
pub struct RenameResult {
    pub old_path: String,
    pub new_path: String,
    pub rewritten_files: Vec<String>,   // 被改动的引用方 rel_path
    pub rewritten_links: usize,         // 总共改写了多少处
    pub warnings: Vec<String>,          // 非致命警告（如某个文件读不出）
}
```

执行流程：

1. **预检**：源存在 / 目标不存在 / 源不是目录 / 解析都在 vault 内；
2. **查询引用方**：
   ```sql
   SELECT src, dst, link_type
     FROM links
    WHERE dst_resolved = ?1 AND src != ?1
   ```
   —— 复用 Phase 2 Task 3 填好的 `links.link_type`（`'wiki'` / `'embed'`）；按 `src` 分组，每个引用方文件只打开一次。
3. **构造 `RewritePlan`**：针对新旧路径生成最多 3 对 wiki 重写候选（path-with-ext / path-no-ext / stem）+ 最多 1 对 embed 候选（full path）。只有在索引器告知"引用方确实用了这个原文本"时才激活对应的替换——避免把 `[[OldTitle]]`（title-form）误当成 `[[old-stem]]` 动掉。
4. **逐文件重写**：读文件 → 正则替换（wiki 用 `\[\[\s*OLD\s*(\|[^\]]*)?\s*\]\]`，embed 用 `(!\[[^\]]*\]\()\s*OLD\s*(\))`）→ 原子写入（`atomic_write` + tmp + rename）。单个引用方失败就 log + push warning，不阻塞整个操作。
5. **移动文件**：`std::fs::rename`，失败 fallback 到 `copy + remove`。
6. **重建索引**：`scanner::delete_one(old)` + `scanner::reindex_one(new)` + 每个被改写的引用方 `reindex_one` —— 让 `links.dst` / `dst_resolved` 一次到位。

#### 6.13.3 替换算法的正确性保证

正则是 bracket-anchored 的，这避免了 `[[foobar]]` 被误匹配成 `[[foo]]`。几个关键约束：

- **只替换 raw 形式与索引库记录匹配的部分**：`RewritePlan::apply(body, links)` 用 `HashSet<&str>` 收集索引里该引用方实际写下的 raw `dst` 字符串。没被记录的形式不替换（保险）。
- **alias 保留**：`[[path/stem|alias]]` 中 alias 段整体透传不动；
- **alt 保留**：`![架构图](path)` 中 alt 段整体透传不动；
- **大小写 & 路径分隔符**：统一把 `\` 归一成 `/`。Windows 路径不做额外处理（Tauri 的 vault 约定都是 POSIX style）。

在 Rust 单元测试里分别覆盖了：wiki 基本/别名保留/路径形式 / embed 基本/alt 保留 / Plan.from_paths 的三种形变场景 / Plan.apply 的"索引器无记录则不动"保险路径。

#### 6.13.4 前端接入点

| 入口                                                                         | 原实现                                                                   | 新实现                                                                                                                                   |
| ---------------------------------------------------------------------------- | ------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| **命令面板 `> Rename current file…`**                                        | 无                                                                       | 打开 rename modal，输入目标路径 → `fileMoveWithRefs` → 重新打开到新路径 + status bar 显示"重写了 N 处引用"                               |
| **`runExtractFromProject`**（`4-projects/x/note.md` → `1-notes/note.md`）    | `fileRead` + `rewriteFrontmatter` + `fileWrite(dst)` + `fileDelete(src)` | `fileMoveWithRefs(src, dst)` 先把引用重写 + 文件搬家，再 `fileRead(dst)` + `rewriteFrontmatter` + `fileWrite(dst)` 应用 frontmatter 变更 |
| **`archiveInboxNote`**（`0-inbox/foo.md` → `.mynotes/archive/inbox/foo.md`） | 走 dumb `fileMove`                                                       | **保持 dumb**：归档意味着该笔记"退出流通"，指向它的链接变成 dangling 是预期行为，不应重写成 `[[.mynotes/archive/inbox/foo]]`             |
| **`promoteInboxNote`**（title + path 同时变）                                | `fileWrite(new)` + `fileDelete(old)`                                     | **暂不切换**：Promote 涉及 title 改名，本版本不处理 title 形式；且 inbox 笔记鲜有外部引用，改造收益低                                    |

Rename modal 的 UX：输入预填完整路径，选区默认落在 stem 部分（`/` 到 `.md` 之间），让最常见的"同目录改名"只需一次输入。目标必须以 `.md` 结尾、不能落在 `.mynotes/` 下、不能已存在。

#### 6.13.5 失败语义

- **引用方读/写失败**：log + warning，继续处理其它引用方；最终 file move 照做。后果：个别文件会留着旧链接——用户在 status bar 的 warning 计数里能看到，可手工修。
- **move 本身失败**：`rename` 失败时 fallback `copy + remove`，都挂了就整体返回 `AppError::Io`，已重写的引用方不回滚（代价可接受：引用方已指向新路径，但新文件没落位 → 用户会看到 dangling；下次 rename 成功时修好）。
- **index 操作失败**：仅 log warning，用户下次打开 vault 或触发 full scan 会自愈。

#### 6.13.6 V2 合规性自检

- **Path-SSOT**：链接的载体是 md 文本，重写的是 md 文本本身；`links` 表只作为"查询引用方是谁"的索引。
- **索引可抛弃**：删掉 `.mynotes/index.sqlite` 后，下一次 full scan 依然能重建所有 `[[...]]` → `dst_resolved` 解析。
- **无 md 注入**：rewrite 只发生在用户显式 rename 的操作路径里；watcher 不会做任何反向改写。
- **数据自持**：导出或打开到 Obsidian 里，所有链接依然是标准 `[[wiki]]` / `![](path)` 语法。

#### 6.13.7 目录重命名（Phase 2 Task 4.5）

单文件 rename 解决了"搬一个 md 不破坏外链"，但文件夹整体改名（`1-notes/` → `notes/`、`4-projects/foo/` → `4-projects/foo-v2/`）此前无对应命令。`dir_move_with_refs` 填上这块。

**命令签名**

```rust
#[tauri::command]
pub fn dir_move_with_refs(from: String, to: String) -> DirRenameResult;

pub struct DirRenameResult {
    old_path: String,
    new_path: String,
    moved_files: usize,       // 树下所有文件（md + 非 md）
    rewritten_files: Vec<String>, // 仅树外 referrer；树内的走 reindex 走
    rewritten_links: usize,
    warnings: Vec<String>,
}
```

**前置校验（串行 + 快速失败）**

1. `from`/`to` 非空，且 `from != to`。
2. 任一侧以 `.mynotes` 开头 → 拒绝（保留目录对用户隐藏、带运行时状态）。
3. **Self-nesting**：`to == from` 或 `to` 以 `format!("{from}/")` 开头 → 拒绝。使用 `/` 边界判断以区分 `foo` vs `foo-bar`（后者允许）。
4. 源目录存在且是 dir；目标不存在。

**执行流程**

```
walk source tree
  → [FileMove { old_rel, new_rel, is_md }]   （跳过 '.'前缀目录，与 scanner 一致）
build aggregate RewritePlan
  → md 文件贡献 3 wiki + 1 embed pair（stem 未变时自动过滤）
  → 非 md 文件贡献 1 embed pair
query links table: dst_resolved LIKE 'from/%' ESCAPE '\\'
  → 用 like_escape 把路径里的 % / _ 转义（支持 `100%done/` 这种诡异路径）
group by src → { ref_path: [(dst_raw, link_type)] }
foreach (ref_path, links):
  read → plan.apply(body, links) → atomic_write
  if ref_path 在树内 → 写入的是 *旧 rel*，fs::rename 时会被带到新位置
  if ref_path 在树外 → 加入 rewritten_files（UI 显示 + 后续 reindex）
fs::rename(src, dst)
  fallback: copy_dir_recursive + remove_dir_all（跨文件系统时）
reindex:
  for fm in files: delete_one(old_rel) + reindex_one(new_rel)  （仅 md）
  for ref in rewritten_files: reindex_one(ref)
```

**为什么一次聚合所有文件、而不是 N 次 `file_move_with_refs`**：假如 `1-notes/` 下有 100 篇 md、5 篇引用其中 20 篇，N 次调用意味着 100 × 5 次 SQL 查询、每个 referrer 被读写 20 次。聚合后只查一次 SQL，每个 referrer 只读写一次——I/O 复杂度从 O(files × referrers) 降到 O(files + referrers)。

**树内 vs 树外 referrer 的两难**

目录重命名时，referrer 本身可能也在被搬的树内。设 `from=4-projects/foo`, 目录内文件 `4-projects/foo/note-a.md` 引用了 `4-projects/foo/note-b.md`。

选择 A（先 move 再 rewrite）：move 后 `note-a.md` 已到 `4-projects/foo-v2/`，继续按 `4-projects/foo/note-a.md` 读就找不到；也不能按 `4-projects/foo-v2/` 读——`per_file` 的 key 还是旧 rel。
选择 B（先 rewrite 再 move）：rewrite 时 referrer 还在旧位置 → `atomic_write` 成功 → `fs::rename` 时它随树一起搬家。**采用这种**。

**与 `file_move_with_refs` 的接口一致性**：两者都返回 `rewritten_files / rewritten_links / warnings`。UI 对两种成功路径有统一的 banner 模板：「Rename dir: 移动 N 个文件，重写了 M 处引用」。

**前端接入点**

| 入口                                       | 实现                                                                                                                                                                                                                                   |
| ------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **命令面板 `> Rename current directory…`** | 目标 = 当前打开文件的父目录（`parentDirOf(openFilePath)`）。`when` 断言要求 `currentFilePath` 包含 `/`（否则文件在 vault 根，没有"父目录"可改）。`openDirRenameModal` 预填整串路径，选中最后一段，让"改 leaf / 保留父路径"只需一次输入 |
| **侧边栏目录右键 → 重命名**                | 暂不做（V2 Phase 2 结束时侧栏仍是纯展示）。留给 Phase 3                                                                                                                                                                                |
| **跟随已打开文件**                         | 若 `openFilePath` 在被搬目录内，改完后自动 `openFile` 到新路径对应的 rel                                                                                                                                                               |

UI 端 self-nesting guard 与后端对齐（`target === src || target.startsWith(\`${src}/\`)`），给出立即反馈，不等 IPC 往返。

**失败语义**

- 与 6.13.5 相同；额外：`fs::rename` 失败且 `copy_dir_recursive` 也失败时，**可能留下部分目录**（rename 做了一半、或 copy 做了一半）。用户看到的是源目录仍在 + 目标目录部分存在，下一次打开 vault 会触发 full scan 修复索引。设计时接受这个代价——替代方案是两阶段事务 + journal，实现成本远超收益。
- 引用方在树内且读失败：已 warning；就算 rewrite 没做到，`fs::rename` 仍会搬它到新位置，reindex 时 `links` 表会反映"原文还指向旧路径"，用户在 Unresolved 面板能看到并手改。

**V2 合规性自检**

- **Path-SSOT**：`links` 索引仅用于"找谁在引用"，源真相是 md 文本。
- **索引可抛弃**：整个 rename 完成后，`.mynotes/index.sqlite` 被删也能由 full scan 重建。
- **无目录注入**：命令拒绝 `.mynotes/` 两侧、拒绝自嵌套、拒绝目标已存在——三道关卡让用户没法用 UI 路径触发 vault 层级被打乱。
- **可 Obsidian 打开**：重命名后的目录结构和链接形式均为标准 markdown，Obsidian 打开看到的是用户期望的新布局。

#### 6.13.8 侧栏右键菜单（Phase 2 Task 4.6）

**动机**

Task 4 / 4.5 把文件和目录的 rename-with-refs 做到后端，但前端只有命令面板一个入口（`> Rename current file…` / `> Rename current directory…`）——这要求用户必须先"打开"才能"改"。想重命名侧栏里看到但没打开的文件、或想 reveal / 删一个不是当前编辑目标的文件，就得先左键点一下切过去。这条摩擦在"整理笔记"的高频场景下会被放大。

右键菜单补齐这层直接操作：在 Sidebar 树上 `contextmenu` 事件即触发，任意 `DirEntry` 都能当 action target，无需先 `openFile`。

**触发与消散**

- **触发**：`.tree-row-wrap` 上的 `oncontextmenu={(e) => openContextMenu(e, entry)}`。`preventDefault()` 吃掉浏览器默认菜单，`stopPropagation()` 防止触到父层。
- **坐标钳位**：`x = min(clientX, innerWidth - 220 - 8)` / `y = min(clientY, innerHeight - 240 - 8)`。常量取"菜单最大宽×最大高"，避免触发点靠窗口右/下边时菜单被截。锚点只按右下夹，不按左上反推——右键菜单的惯例是"从点击点向右下展开"，只做"不出界"的保底。
- **消散路径** 三条：
  1. `Escape` 键（顶层 `onkeydown={onCtxMenuKey}`）。
  2. 点击菜单外任何位置：透明全屏 `.ctx-menu-backdrop` 捕获 click，`closeContextMenu`。菜单本体 `onclick={(e) => e.stopPropagation()}` 阻止向上冒泡。
  3. 点击菜单项：每个 handler 先 `closeContextMenu()` 再执行实际动作——这样"打开文件"或"删除"过程中的任何异步等待期间菜单都不挂着。
- **backdrop 上也挂 `oncontextmenu`**：右键点到菜单外要关掉当前菜单而不是开第二个（默认浏览器行为或父元素可能响应），因此 backdrop 的 `oncontextmenu` `preventDefault() + closeContextMenu()`。

**菜单内容（按 entry 类型分支）**

| entry.is_dir    | 菜单项（从上到下）                                                                                         |
| --------------- | ---------------------------------------------------------------------------------------------------------- |
| `false`（文件） | 打开 · 重命名… · ─ · 在 Finder 中显示 · **删除**（danger 色）                                              |
| `true`（目录）  | 展开 / 折叠（按当前 `expanded.has(path)` 文案切换） · 重命名… · 在此文件夹新建笔记… · ─ · 在 Finder 中显示 |

**目录不做删除**：`file_delete` 后端显式拒绝目录（"refusing to delete a directory"），动机是一次误点不应该能把一整子树消掉。Phase 3 可以加专门的 `dir_delete` 命令 + 强确认（"即将递归删除 N 个文件 …"），但这次不引入。

**路径**

- 菜单顶部 header 行展示 `entry.rel_path` 完整路径（mono 字体、dim 色、截断省略）——让用户在菜单上能看到"我现在右键的是哪一条"而无需回头看树。高频场景是"在展开的深目录里误点"，header 给个二次确认。

**各 handler**

| handler           | 动作                                                                                                                                                                                                                                                                        |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ctxOpenOrToggle` | `is_dir → toggleDir(entry)`；`is_file → openFile(entry)`。效果等价于左键点，只是放进菜单作为显式动作。                                                                                                                                                                      |
| `ctxRename`       | `is_dir → openDirRenameModal(entry.rel_path)`；`is_file → openRenameModal(entry.rel_path)`。两个 modal 各自走已有的 `file_move_with_refs` / `dir_move_with_refs` 流程。                                                                                                     |
| `ctxNewNoteInDir` | 仅 dir。`newNote(entry.rel_path)` 把 New Note modal 的 `targetDir` 预填成右键的目录，复用 Task 3 的 4-projects 分支机制。                                                                                                                                                   |
| `ctxReveal`       | `pathReveal(entry.rel_path)` 走新增的 Rust 命令（见下）。                                                                                                                                                                                                                   |
| `ctxDelete`       | 仅 file。Tauri `ask(msg, { title, kind: 'warning' })` 原生确认 → `drainPendingSaves` → `fileDelete` → 若当前编辑器打开的就是它，清空 `openFilePath + editorContent` 以防 stale write-back → `invalidateWikiCompletionCache` + `refreshTree` + `schedulePanelRefresh(200)`。 |

**Rename handler 参数化重构**

Task 4 / 4.5 的 `openRenameModal()` / `openDirRenameModal()` 原实现硬绑 `vaultState.openFilePath`，右键场景下要能指定任意 path。本次把签名改为 `openRenameModal(path?: string)` / `openDirRenameModal(dirPath?: string)`——省参时继续用 `openFilePath`（保留命令面板的零参调用），传参时用传入的值。`openDirRenameModal` 进一步分两种场景：

- **explicit 模式**（右键目录）：直接把 `dirPath` 当源目录，modal 预填 `dirPath`，默认选中末段。
- **implicit 模式**（命令面板）：`parentDirOf(openFilePath)` 推父目录，如果文件在 vault 根（父为 `''`）则 modal 不打开并报错——这条是 `> Rename current directory…` 的 `when` 断言已经拦过的，这里是双保险。

好处：命令面板走的和右键走的是同一个 modal + 同一套 confirm 逻辑，只是初始 source 路径不同。未来要加"拖拽到新父目录"这种三入口也一样接进来。

**`path_reveal` 跨平台实现**

新增 Rust 命令 `path_reveal(rel_path: String) -> AppResult<()>`：

```rust
#[cfg(target_os = "macos")]
Command::new("open").arg("-R").arg(&abs).spawn()?;

#[cfg(target_os = "linux")]
{
    let target = if abs.is_dir() { abs.clone() }
                 else { abs.parent().unwrap_or(&abs).to_path_buf() };
    Command::new("xdg-open").arg(&target).spawn()?;
}

#[cfg(target_os = "windows")]
Command::new("explorer").arg(format!("/select,{}", abs.display())).spawn()?;
```

- **macOS**：`open -R <abs>` 在 Finder 里打开父目录并**预选**该项。对文件和目录都有效。
- **Linux**：`xdg-open` 没有"select"动词——只能打开。因此对文件我们回退到"打开父目录"（用户仍然看到目标文件，但不预选）；对目录直接打开该目录。没有引入额外依赖（如 `dbus-send` 给 Nautilus）的必要。
- **Windows**：`explorer /select,<abs>` 是 native 的"在 Explorer 里高亮此项"语法。
- **为什么不用 `tauri-plugin-opener`**：该插件是"打开默认应用处理此 URL"的抽象，没有"select in file manager"语义。我们只需要三条 shell 命令，写在 `#[cfg(target_os)]` 分支里更干净，也避免了在 `Cargo.toml` + `capabilities/default.json` 给 opener 两头注册。
- **spawn 不 wait**：reveal 是"点一下就该看到窗口"的 UX，我们 `spawn()?` 启动子进程就返回。真失败（`ENOENT: 无 open 命令`）会被 `?` 转成 `AppError::Other` 并回到前端 notice 通道。

IPC 注册：`src-tauri/src/lib.rs` 的 `invoke_handler!` 数组追加 `commands::file::path_reveal`。前端 wrapper `src/lib/ipc/file.ts` 新增 `pathReveal(relPath): Promise<void>`。

**CSS 要点**

- `.ctx-menu-backdrop`：`position: fixed; inset: 0; z-index: 120`（比 modal 的 100 高，但比 toast 的 200 低）——右键菜单 overlay 在 modal 之上也能消散自己。
- `.ctx-menu`：`position: fixed; left/top` 由 state 注入；`min-width: 180px; max-width: 220px`；`background: var(--glass-bg); backdrop-filter: blur(12px)`；`border-radius: var(--radius-md)`；`box-shadow: var(--glass-shadow) + var(--pane-border)`——与命令面板一致的玻璃卡片语言。
- `.ctx-menu-header`：mono / uppercase / `--color-fg-dim` / `0.72rem` / padding 上下 6px——与 Panel section 标题同构，强化"这是一条上下文元数据"。
- `.ctx-menu-item`：`text-align: left; width: 100%`；hover `background: var(--color-surface-raised)`；`padding: 6px 12px`；`font-size: 0.85rem`。
- `.ctx-menu-item.danger`：`color: var(--color-danger)`；hover 背景用 `color-mix(in oklch, var(--color-danger) 12%, var(--color-surface-raised))`——不同于普通 hover，提示"这是不可撤销动作"。
- `.ctx-menu-sep`：1px hairline inset，`margin: 4px 8px`，把"浏览类"（打开/重命名/新建）和"系统类"（Finder）及"破坏类"（删除）视觉分段。

**V2 合规性自检**

- **Path-SSOT**：菜单本身不写入任何新状态；rename / delete / reveal 都透传已有 IPC。
- **无静默注入**：Delete 有 `ask()` 确认；rename 走 modal 确认；reveal 是只读动作。
- **索引可抛弃**：Delete 后端 `scanner::delete_one` 同步清索引行，但即便不清，下一次 full scan 也能修——菜单没有依赖索引本身的正确性。
- **Obsidian 兼容**：菜单不碰 frontmatter / 不注入 wiki-link；所有改动都是文件系统动作，Obsidian 打开看到的是一致的新布局。

#### 6.13.9 侧栏文件 drop 导入（Phase 3-A6）

**动机**

Phase 2 Task 8.2 把 Tauri 原生 drag-drop 关掉（`dragDropEnabled: false`），DOM 的 `drop` 事件从此能冒泡到前端。编辑器已经吃住了 `image/*` 拖放（归档进 `attachments/YYYY/MM/…`），但侧栏仍是"纯展示"——用户从 Finder 拖 `.md` / PDF / 图片进左侧树无反应，只能先把文件手动放进 vault 目录再等 watcher 扫进来。P3-A3 候选清单明确把"Sidebar 文件 drop 导入"列成遗留摩擦，本节是它的实现对齐版。

**与编辑器 drop 的分工**

两者语义完全不同，不要合并：

| 路径              | 触发区              | 落点规则                                          | 命名                                           |
| ----------------- | ------------------- | ------------------------------------------------- | ---------------------------------------------- |
| 编辑器 drop       | CM6 编辑区（正文）  | 始终进 `attachments/YYYY/MM/`                     | `YYYYMMDD-HHmmss-<slug>.<ext>`（时间戳重命名） |
| 侧栏 drop（本节） | Sidebar `<ul.tree>` | 用户指向的目录（含按文件 row 推父级的规则，见下） | **保留源 basename**，冲突时 `-1 / -2` 递增     |

侧栏 drop 的语义是"把这个文件**按原名**放进我指向的目录"——这就不能复用 `attachment_save`（后者强制落 `attachments/` 并改名）。因此新增独立命令 `file_import`。

**触发与路径解析**

- DOM handler：`<ul.tree>` 挂 `ondragover/ondragleave/ondrop`，每个 `.tree-row-wrap` 另外挂一套同名 handler。`dragover` 必须 `preventDefault()` 才能让后续 drop 事件触发；同时设 `dataTransfer.dropEffect = 'copy'`，让 Finder 的拖动图标变成 `+`，给用户"这是 import 不是 move"的明确提示。
- 路径来源：Finder 拖文件进 WKWebView 时，`File` 对象本身不暴露绝对路径（不像 Electron 有 `file.path`），但 `dataTransfer.getData('text/uri-list')` / `text/plain` 里会有 `file:///…` URI。`parseDroppedPaths()` 先走 `text/uri-list`（可多行）再回退到 `text/plain` 首行，`decodeFileUri()` 去 `file://` 前缀并 `decodeURIComponent` 还原中文 / 空格；也兼容用户手打 POSIX / Windows drive-letter 绝对路径的情况。**不**做 `file.arrayBuffer()` 读字节的 bytes fallback，原因见"Known gaps"。

**drop 目标三分支**

| 用户拖到                   | dstDir 解析                               | 语义                                             |
| -------------------------- | ----------------------------------------- | ------------------------------------------------ |
| 目录 row（`is_dir=true`）  | `entry.rel_path`                          | "放进这个目录"                                   |
| 文件 row（`is_dir=false`） | `parentDirOf(entry.rel_path)` 或 vault 根 | "放在这篇笔记旁边"——按父目录解析                 |
| `<ul.tree>` 空白区         | `0-inbox/`                                | "不知道放哪"——走 Quick Capture 同一条 inbox 通道 |

root 分支故意选 `0-inbox/` 而不是 vault 根：vault 根在 LYT 里不是自由文件区，默认落根会破坏"按数字目录分工"的结构；落 inbox 符合"不确定的东西先进 inbox 慢慢分拣"的既有工作流。

**命名冲突**

后端 `pick_free_slot` 从 `<stem>.<ext>` 起试，冲突则 `<stem>-1.<ext>`、`<stem>-2.<ext>` … 最多 64 次（与 `attachment_save` 的冲突上限对齐）。`pick_free_slot` 被抽成可测纯函数（见 `commands/import.rs` 单测），输入 vault path + dstDir + stem + ext，输出 `(rel_path, was_renamed)`。`was_renamed = true` 时前端 notice 会把最终 basename 显示出来，让用户立刻知道"原 `foo.md` 变成 `foo-1.md`"，不做静默。

**后端命令 `file_import(src_abs, dst_dir) → ImportedFile`**

- 硬约束：`src_abs` 必须是绝对路径；必须存在；必须是普通文件（目录明确拒，notice 提示"drop individual files instead"）；basename 不得以 `.` 开头或含路径分隔符。
- vault-internal 源拒绝：canonicalize 源路径与 vault 根，若 `src_canon.starts_with(vault_canon)` 则拒——那是 rename/move 不是 import，防止用户从 Finder 里选到 vault 内的 md 又拖回侧栏造成"同内容复制"。
- `dst_dir` 经 `resolve_in_vault` 解析并校验是目录（空串 = vault 根，也允许）；父目录 `create_dir_all`；`std::fs::copy` 走一次，返回 `{ rel_path, original_name, was_renamed, bytes_copied }`。
- **为什么不走 bytes IPC**：`fs::copy` 在 Rust 侧一次拷贝，走系统调用（macOS 有 `clonefile(2)` fast path，APFS 上常常是 O(1)）；若走 bytes 则前端 `file.arrayBuffer()` → 序列化 `Vec<u8>` → IPC → 后端落盘，内存翻倍 + 慢一个数量级。代价：要求源路径能被 `file://` URI / `text/uri-list` 暴露；Finder 100% 命中，浏览器下载气泡等偶发例外见 Known gaps。

**前端反馈聚合**

- `handleSidebarDrop(paths, dstDir)` 循环调 `fileImport`，聚合成 `imported[] / failures[]`；不分开发 N 条 notice。
- 全成功 + 单文件：`已导入 <name> → <dstLabel>`，若 `.md` 则顺手 `openFile()` 打开（符合"drop 进来就想读"的直觉）；若 `was_renamed` 追加 `（重命名为 xxx-1.md）`。
- 全成功 + 多文件：`已导入 N 个文件 → <dstLabel>`，不自动打开任何一个。
- 部分成功：`已导入 K / N 个文件 → <dstLabel>；X 失败：<firstErr>`，`info` 样式更长 TTL（6s）。
- 全失败：`导入失败（N/N）：<firstErr>`，`error` 样式。
- 无论成败，若 `imported.length > 0` 就 `expanded.add(dstDir)` + `refreshTree()` + `schedulePanelRefresh(200)`，确保新文件立刻在树里可见。

**视觉反馈**

- `.tree-row-wrap.drop-target` 加 1px accent outline + 轻 accent bg tint——用 `--color-accent` / `--color-accent-tint`（缺失时给 fallback）。只在拖拽期间出现，drop / dragleave 一结束立刻 reset。
- `.tree.drop-root-active` 加 2px dashed outline，告诉用户"松手会落进 0-inbox"。
- `dragleave` 的 `relatedTarget` contains-check 避免了"从 `.tree-row-wrap` 移到内部 `.tree-row` 再移回"这种合法悬停被错误识别成离开的 flicker。

**失败语义**

- src 是目录、src 在 vault 内、`fs::copy` 失败（权限/磁盘满）、basename 非法——都通过 `AppResult::Err` 冒泡到前端 `failures[]`，聚合 notice 提示。
- 部分成功不回滚：多文件 drop 中第 3 个失败，前 2 个已落盘不会被撤销。这与 `attachment_save` / `file_move` 现有语义一致；V2 的"文件系统真相源"原则允许用户用 Finder / rm 自己清理。
- 后端不做索引 upsert：新文件落盘后走 `notify-rs` 正常通路重新 index，前端 `schedulePanelRefresh(200)` 给 watcher 一点时间追上。与 MOC 创建 / extract 同一模式。

**Known gaps**

- **bytes fallback 留给后续**：浏览器下载气泡、部分不开放 file URI 的第三方应用，Finder 之外的来源可能只有 `File` 对象没有绝对路径。这种情况当前命中 `parseDroppedPaths → []` 分支，notice 提示"无法识别拖入的文件路径，请从 Finder 重试"。真要兜底需加 `file_import_bytes(bytes, name, dstDir)` 与前端 `file.arrayBuffer()` 读取，但会让后端命令翻倍，权衡后留到有真用户反馈再加。
- **目录 drop 不做**：拖目录进来意味着要么递归 copy 整棵子树（Obsidian 也不做），要么弹"导入全部子文件"对话框。后端直接拒，notice 明确告知"不支持拖入目录"。
- **不支持 vault 内部 drag-to-move**：侧栏内部文件间拖动是独立的 rename / move 特性，需要与 `file_move_with_refs` 联动做链接重写，和本节的"外部文件导入"语义完全不同。Phase 3 后续任务。
- **前端无单测 harness**：`parseDroppedPaths` / `normalizeDropDstDir` 都是纯函数，但仓库暂无 vitest，只有手测覆盖。后端 `pick_free_slot` / `split_name` 以 Rust 单测覆盖 8 条场景。

**V2 合规性自检**

- **Path-SSOT**：导入的是物理文件，路径即真相；后端返回的 `rel_path` 是规范化后 vault-relative，直接能喂给 `openFile` / `fileList`。
- **索引可抛弃**：`file_import` 不手动 upsert 索引，`notify-rs` 观察到新文件后自动 reindex；删 `.mynotes/index/*.sqlite` 后下一次 open 走 full scan，新导入的文件一起被拣回索引。
- **无静默注入**：所有冲突都加 `-N` 后缀而不是覆盖；后端目录源 / vault-内部源显式 `Err` 冒泡到 notice；前端不对文件内容做任何改写，`fs::copy` 按字节复制。
- **Obsidian 兼容**：导入后的文件在 vault 里是普通 md / 附件，Obsidian 打开即可；不碰 frontmatter、不写 metadata sidecar。

### 6.14 设置界面（Phase 2 Task 8）

**动机**

Phase 1 之前只有主题一个可调项（状态栏齿轮三向循环），实际上用户还关心：「autosave 延迟是多少」、「模板和仓库自带的分歧，能不能一键重置回默认」。把这几项集中进一个 `⌘,` 打开的 Settings modal，替散落在状态栏/命令面板/隐藏 localStorage 的"调节点"建一条单一入口。

**打开路径**

- `⌘,` 全局快捷键（`installShortcuts` 捕获阶段 listener）——与 macOS 原生设置窗的键位一致，学习成本为零。
- 命令面板 `> Settings…`（`hint: ⌘,`）——在 `PALETTE_COMMANDS` 靠后排位，紧贴主题切换三条命令和导出组，让"不常用但需要时得找得到"的动作集中到一处。

**模态内容（四块，从上到下）**

| 区块         | 控件                                 | 行为                                                                                                                                                                                |
| ------------ | ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 主题         | 三向 radio（跟随系统 / 浅色 / 深色） | onchange 立即 `setTheme(next)`，内部走 `localStorage.setItem(THEME_KEY)` + `applyTheme()`——与状态栏齿轮共用同一 setter，两端状态实时一致                                            |
| 自动保存延迟 | `<input type=number>`，单位毫秒      | oninput 钳位到 `[100, 5000]` + 持久化到 `mynotes:autosave-ms`；`onContentChange` 的 `setTimeout` 直接读 `autosaveDelayMs` 变量，下次编辑立即生效，不需要刷新                        |
| 模板         | "从内置重置 templates/" 按钮         | 先 `ask()` 给警告，然后调 `vaultReseedTemplates()`，结果（新增 N / 覆盖 N / 未改动 N）显示在按钮右侧，不走状态栏——让"我刚按了这个"的反馈留在上下文里                                |
| 中文分词     | 纯说明文字                           | 索引使用 SQLite FTS5 `unicode61` tokenizer，tokenizer 在建表时固定，不支持运行时切换；说明文字显示"改动需要重建索引"——把"为什么没有 toggle"讲清楚，避免用户寻找一个其实不存在的开关 |

**状态管理**

- `settingsOpen: boolean`——与其它 modal 使用相同的 `transition:fade/fly` 背景 + dialog 双层结构，Escape / 背景点击均可关闭。
- `settingsReseedRunning: boolean`、`settingsReseedMsg: string`——复用期间禁用重置按钮、reopen modal 会清掉上一次的 msg（在 `openSettings()` 里重置）。
- `autosaveDelayMs: number`——`$state`，页面顶层持有；`onMount` 从 localStorage 回填；`onContentChange` 里 `setTimeout(..., autosaveDelayMs)` 每次读取当前值，无需重建 timer。
- `resetVaultViewState()` 把 `settingsOpen` 一起清成 false——Close Vault 时一并回归初态。

**不收进这一版**

- **快捷键自定义**：需要读写配置文件 + 冲突检测 + 修改 `installShortcuts` 让它从 map 驱动而不是 `if/else` 硬编码；收益是"高级用户自由度提升"，工作量与这次 batch 预算不匹配，留 Phase 3。
- **Vault 根目录选择**：已经通过欢迎页/`Change vault` 按钮 + recent 列表做得足够——不增加一个多余的"设置里改 vault 路径"引导。

**V2 合规性自检**

- **Path-SSOT**：所有偏好都存 localStorage（主题 / autosave），与 vault 本身解耦；换机器打开 vault 看到的是默认偏好，不破坏文件内容。
- **无 md 注入**：Settings modal 不读写任何 md 文件，只调 `vaultReseedTemplates()`（该命令仅动 `templates/` 下的内置模板文件）。
- **可 Obsidian 打开**：主题只影响 MyNotes 渲染；重置模板后 `templates/*.md` 仍是纯 markdown，Obsidian 读它们没有区别。

### 6.15 导出（Phase 2 Task 8）

**动机**

前面所有动作都是"在 vault 里做"；导出是第一个"把 vault 里的东西带出去"的通道。两类需求：整库离线打包（备份、跨机迁移、交给协作者复制一份），以及单篇分享（邮件、IM、贴博客之前的 markdown 抽取）。PDF 出于"不引入第三方渲染库"的考虑，绕道浏览器 print dialog。

**三条命令**

| 命令面板条目                           | 后端 IPC                                                 | 目的                                                                                                                                                 |
| -------------------------------------- | -------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `> Export vault as zip…`               | `vault_export_zip(dest_abs_path) -> ExportSummary`       | 把整个 vault（排除 `.mynotes/`）打成 zip                                                                                                             |
| `> Export current note (.md)…`         | `note_export_copy(src_rel_path, dest_abs_path) -> ()`    | 把当前打开的 md 文件按原内容复制到任意路径                                                                                                           |
| `> Print current note (→ Save as PDF)` | `note_render_print_html(src_rel_path, theme?) -> String` | Rust 端 pulldown-cmark 渲染成 HTML → `opener::open` 扔给系统默认浏览器，用户在浏览器里 `⌘P` 存 PDF。`theme` 透传 app 当前 light/dark/system（P3-A7） |

**后端 `vault_export_zip` 的设计决策**

- **永远排除 `.mynotes/`**：SQLite 索引是派生件，新环境重建只要几秒；打包进去会让归档体积翻一倍、还要担心 DB 版本差异搞坏协作者侧的打开路径。
- **保留 `attachments/`**：否则导出的 vault 里 `![](attachments/…)` 全是断链，协作者打开只剩骨架。
- **压缩算法用 DEFLATE 不用 ZSTD**：DEFLATE 兼容性覆盖所有桌面 OS 自带的 zip 工具（Finder / Explorer / `unzip`），markdown 压缩率本来就好；引入 `zstd` 会给 `zip` crate 加 feature 并放大二进制。
- **归档内路径强制 `/`**：`rel_path_to_archive_name` 把 Windows 的 `\` 归一成 `/`，让 macOS / Linux 解压后的路径是常规 POSIX 风格。
- **`.part` + rename 原子化**：先写 `<dest>.part`，完成后 `std::fs::rename` 到目标路径——写到一半崩了只会留个 `.part`（肉眼可见），绝不会留半个假合法的 zip 让用户误信。
- **符号链接直接跳过**：`walkdir.follow_links(false)` + 显式检查 `is_symlink()`——避免循环以及"链接指向 vault 外"的意外泄露。
- **权限目录条目**：`add_directory(...)` 把空目录显式写进去（保留 `attachments/2026-04/` 即使下面被删空了的情形），unix perms 0o644。
- **返回 `ExportSummary { dest_path, file_count, bytes_written, skipped_count }`**：状态栏里显示"已导出 N 个文件（uncompressed 大小，跳过 M）"，让用户看一眼就知道完整性。

**单篇导出 `note_export_copy` 的设计决策**

- **在 Rust 侧做 copy**：原因是前端没启用 `@tauri-apps/plugin-fs`——单篇导出一个按钮就加一个插件 + capabilities JSON + 打包体积，不划算；Rust 侧加一条 ~30 行命令更干净。
- **读前先 `drainPendingSaves()`**：前端在命令触发前把 editor 的待写 flush 到磁盘，然后 Rust 命令用 `std::fs::copy(src_abs, dst_abs)`——保证导出的就是"用户此刻看到的文本"，不是落后 500ms 的旧版本。
- **拒绝覆盖**：destination 存在时返回错误；save dialog 已经提示过，但用户手敲路径时的第二道闸。
- **拒绝路径穿越**：`src_rel_path` 走与 `file_read` 一致的 `..` 拒绝 + absolute 拒绝校验，确保不能用"导出"把 vault 外的文件搬到 vault 外。

**PDF 路径：`note_render_print_html` → 系统默认浏览器（2.6 修订）**

初版走 `window.print()` + `@media print`，实测两个硬伤：

1. **Tauri macOS WKWebView 静默吞掉程序化的 `window.print()`**：从 palette 命令 handler / `setTimeout` 非"用户手势"入口触发时，根本没有打印对话框弹出。在原生浏览器里这个调用是可行的；WKWebView 对它的 trust gating 更严。
2. **CM6 视口虚拟化**：CodeMirror 6 只把视窗内的行真正挂到 DOM 上，`@media print` 放开 `height / overflow` 再多也打不出不存在的节点——打印出来永远只有第一屏（如果 #1 绕过去的话）。

因此现版本改走"Rust 端渲染 HTML → 扔给系统默认浏览器"：

- `note_render_print_html(src_rel_path)` 命令：读 `.md` → 去 YAML frontmatter（`---` 包围的首块）→ `pulldown-cmark` 带 tables/strikethrough/task-lists/footnotes 扩展渲染成 HTML → 包在 `<html><body>` 骨架里（内联一套打印友好的 CSS：系统字体栈、最大 780px 阅读列宽、`@page { margin: 0.75in }`、表格/代码块/引用块/任务列表样式、`@media print` 下去掉背景色与 link 染色）→ 带 `<base href="file:///…vault/">` 让 `attachments/…` 相对路径能正确解析到 vault 内的图片（Rust 用 `url::Url::from_directory_path` 生成带百分号编码的 base URL）→ 写到 `<app_support_dir>/print-preview/<sanitized-stem>-<ms>.html` → `opener::open` 调用系统默认"打开 .html"的 handler（macOS `open` / Windows `start` / Linux `xdg-open`，都会选用户的默认浏览器）。
- 前端 `runPrintCurrentNote`：先 `drainPendingSaves()` 把编辑器内的 pending 写回磁盘，再 `noteRenderPrintHtml(path)`，状态栏提示"已在浏览器打开预览（`<file>.html`）。在浏览器中按 ⌘P / Ctrl+P 保存为 PDF"。
- 浏览器里用户按 `⌘P` 走原生打印对话框（从用户手势触发，没有被吞的问题）；选"另存为 PDF"或发到实体打印机都是系统/浏览器功能，我们不参与。
- 依赖增量：`pulldown-cmark 0.11`（`default-features = false, features = ["html"]`，纯 Rust、体积小）+ `opener 0.7`（包一层 `open`/`start`/`xdg-open`）+ `url 2`（生成 `<base href>` 用的 file:// URL；Tauri 自己也依赖它）。
- 产物文件：`app_support_dir/print-preview/<stem>-<ts>.html` 每次调用都写一份新的，方便调试；不自动 GC（KB 级、位置隐蔽）。以后要清就一把 sweep。
- **@media print CSS 保留但降级为 backup**：用户如果手动在主窗口按 `⌘P`（用户手势，能穿过 WKWebView 的门），还能得到一份勉强的打印输出——虽然依然会被 CM6 虚拟化截断，但至少不会印出三栏布局。

**打印 HTML 主题化（P3-A7）**

Phase 2 初版的 HTML 骨架 CSS 是固定亮色（白底黑字），暗色用户在编辑器里已经切到 dark，但点"Print current note"后浏览器里蹦出来的预览仍是纯白——视觉连续性断裂，光是"我是不是点错了"的疑惑就够一次 friction。P3-A7 把这条打完：

- **前端透传 theme**：`noteRenderPrintHtml(srcRelPath, theme?: ThemePreference)` 第二个参数来自 `+page.svelte` 的 `$state<Theme>`，三档：`'light' | 'dark' | 'system'`。`runPrintCurrentNote` 直接把当前 UI theme 塞进去，命令面板调用链上再无分支。
- **后端三分支生成**：Rust 侧新增 `PrintTheme { Light, Dark, System }` enum（`from_option` 对未识别字符串 / None / 空串都收敛到 `System`，做 forward-compat），`wrap_print_html(title, base_href, body_html, theme)` 按 enum 选：
  - `Light` → `<html data-theme="light">` + `:root { color-scheme: light; ... }`；不发 `@media (prefers-color-scheme: dark)`，OS 反向切暗不影响预览。
  - `Dark` → `<html data-theme="dark">` + `:root { color-scheme: dark; ... }` + `:root[data-theme='dark']` 覆盖一套暗色变量；同样不发 media query。
  - `System` → `<html>`（无属性）+ `color-scheme: light dark` + `@media (prefers-color-scheme: dark) { :root:not([data-theme]) { ... } }` 让浏览器按 OS 现场择色。
- **`@media print` 强制走亮色**：三种 preview theme 里，打印（真实出纸 / 存 PDF）都要走亮色——否则暗色背景会连带印成 PDF 里的黑底浪费墨。print 块里 `:root, :root[data-theme='dark'] { ...light vars... }` 把两条分支一次铺平。
- **为什么没用 oklch()**：app.css 用 `oklch()`，在 Chrome 下打印视觉效果很好，但 macOS Preview / 旧 PDF 查看器 / iOS Books 对 oklch 支持不一致；打印产物优先跨工具可读性，这里退回 hex，跟 GitHub 风格靠齐。
- **单测覆盖**：`cargo test` 新增 5 条：`PrintTheme::from_option` 四入一出（包括 unknown → System）、`wrap_print_html` 在 Light/Dark/System 三档下分别含 / 不含 `data-theme` / `@media (prefers-color-scheme: dark)` 的特征字符串、`@media print` 块始终出现且双分支重置 root 变量。

**前端 IPC wrapper 与命令面板接入**

- 新文件 `src/lib/ipc/export.ts`：
  - `exportVaultZip(destAbsPath: string): Promise<ExportSummary>`
  - `noteExportCopy(srcRelPath: string, destAbsPath: string): Promise<void>`
  - `noteRenderPrintHtml(srcRelPath: string): Promise<string>`（返回生成的 HTML 绝对路径）
- `commandRegistry.ts` 的 `PaletteContext` 扩 4 个方法：`runOpenSettings` / `applyThemeChoice` / `runExportVaultZip` / `runExportCurrentNote` / `runPrintCurrentNote`。`PALETTE_COMMANDS` 追加：`open-settings` / `set-theme-system|light|dark` / `export-vault-zip` / `export-current-note` / `print-current-note`。
- 单篇导出和 print 的 `when` 断言：`currentFilePath?.endsWith('.md') && !startsWith('.mynotes/')`——在 Home / Tag / Graph 视图下这两条命令不会出现在面板里。

**失败语义**

- **zip**：任何 I/O 错误在 walk 期间 log warning 继续（`walkdir` entry 失败不 abort），最终 `zw.finish()?` 或 `fs::rename` 失败会回冒到前端；部分产物会留在 `.part` 供用户检查。
- **note copy**：单次 `fs::copy` 失败直接抛给前端，没有部分态——要么整篇到位，要么 dest 根本不存在。
- **print**：`note_render_print_html` 的任何失败（读文件 / 写 HTML / `opener::open`）走 `AppError` 冒泡到前端，状态栏显示 `print preview: <msg>`。`opener::open` 失败通常意味着系统没有默认 `.html` handler——极少见但会被用户看到。成功后在浏览器里的打印对话框属于系统行为，不属于本命令职责。

**V2 合规性自检**

- **Path-SSOT**：导出是"把 md 从 vault 搬出去"的只读操作（对 vault 而言），不改 md / 不改索引 / 不写偏好。
- **数据自持**：zip 里是纯 markdown + 附件二进制；解包后用任何 markdown 工具链（Obsidian、`mkdocs`、vanilla 文本编辑器）打开都是原样。
- **索引可抛弃**：zip 不含 `.mynotes/`；用户解包后首次用 MyNotes 打开会触发一次 full scan，秒级完成。

---

### 6.16 AI 辅助面板（P3-D1）

#### 6.16.1 架构原则

P3-D 的 AI 辅助层严格遵守以下约束（见 `plan_P3.md §1` 启动原则）：

- **Markdown 永远是 SSOT**：AI 产物全部是派生数据，不写入 vault 任何 `.md`。
- **可一键关闭**：`ai_enabled = false` 时前端不发任何 IPC 调用，后端命令也不会被触发。
- **离线优先**：related-notes 当前完全离线——所有打分信号来自本地派生索引 `index.sqlite` +（若已初始化）`embeddings.sqlite`，无外部请求。
- **可审计**：未来 D2+ 的网络调用会写 `.mynotes/ai/usage.log`；D1 无日志需求。

#### 6.16.2 打分模型（当前实现：D2a.5 后）

当前 `ai_related_notes` 基于以下线性加权模型，全部在 Rust 侧、本地索引查询后计算，典型墙时仍保持交互级（< 5 k 笔记 / < 50 k chunks 的 vault）：

```
score(current, candidate) =
    2.0 * tag_overlap        # 共同 tag 数 / min(|tags_current|, |tags_candidate|)
  + 1.5 * direct_link        # 1 if wiki-link exists in either direction
  + 1.0 * co_cited           # 1 if another note links to both
  + 0.5 * embedding_cosine   # cosine over note-level summed chunk embeddings
  - 0.3 * staleness          # days_since_updated / 30, clamped [0, 1]
```

`score ≤ 0` 的候选被过滤掉（无任何正向信号）。

当当前 model 下没有 embedding，或某篇笔记尚未完成初始化时，`embedding_cosine = 0`，其余 links / tags / co-citation 信号照常工作；命令签名保持不变。

#### 6.16.3 IPC 命令

| 命令                        | 签名                                                      | 说明                                                                                             |
| --------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `ai_related_notes`          | `(src_rel_path: String, limit?: u32) -> Vec<RelatedNote>` | 返回最多 `limit`（默认 10，上限 50）条相关笔记，无 vault 时返回空列表                            |
| `app_config_set_ai_enabled` | `(enabled: bool) -> AppConfigSnapshot`                    | 持久化 `ai_enabled` 到 `app-config.json`；`AppConfigSnapshot` 加 `ai_enabled: Option<bool>` 字段 |

**`RelatedNote` 结构**：

```rust
pub struct RelatedNote {
    pub path: String,
    pub title: Option<String>,
    pub note_type: Option<String>,
    pub updated: Option<String>,
    pub score: f64,
    pub signals: RelatedSignals,  // per-signal breakdown for hover tooltip
}
```

#### 6.16.4 UI 集成

- **`Panel.svelte`**：在「未解析」section 之后新增「AI 相关笔记」section（虚线上边框区分），仅当 `aiEnabled === true` 且 `relatedNotes.length > 0` 时渲染。每条 entry 的 `title` 属性携带信号 tooltip（格式：`相关度 2.50 · tag重叠 80% · 直接链接`）。
- **Settings 模态框**：末尾新增「AI 辅助面板」区块，含开/关 checkbox + hint 文字（"完全离线运行"）。
- **命令面板**：新增命令 `> Show Related Notes (AI assist)`，条件显示（须打开一个 `.md` 文件）；执行时若 aiEnabled=false 先自动开启，然后 scroll 到 related-notes section。
- **`aiEnabled` 状态管理**：`+page.svelte` 管持有 `$state<boolean>`，从 `loadAppConfig` 读 `snapshot.ai_enabled ?? true`（`null` 表示首次启动，默认开）；通过 `{aiEnabled}` prop 传入 `Panel.svelte`；Settings checkbox 变化时同步调用 `appConfigSetAiEnabled`。

#### 6.16.5 查询性能约束

- 候选集枚举：一条 `SELECT * FROM notes WHERE path != ?1` 扫全表，O(n) 但不涉及磁盘写。
- 批量 tag 加载：一条 `SELECT note_path, tag FROM tags WHERE note_path != ?1` 替代 N 次单行查询，避免 N+1 问题。
- 直接链接 / 共同引用：两条 UNION / JOIN 查询，均有 `idx_links_src` + `idx_links_dst` 双索引覆盖。
- 未来 D2 embedding 查询走 `embeddings.sqlite`（独立文件），不共用主索引的 WAL 锁。

#### 6.16.6 V2 合规性自检

- **无写入 vault**：`ai_related_notes` 是只读命令，不操作 `vault/` 目录任何文件。
- **不依赖网络**：P3-D1 命令链路中零外部 HTTP 调用。
- **可关闭**：前端 `aiEnabled === false` 时不调用 IPC；后端命令在 `ai_enabled` 为 false 时仍然可被直接调用（权限下放给用户），但不会被 UI 主动触发。
- **索引可抛弃**：D1 不向 `.mynotes/` 写任何文件，重建索引不影响 AI 能力（只是打分数据来源重置）。

---

### 6.17 AI 辅助·Embedding 索引底座（P3-D2a.1）

D2a 的目标是把全 vault 段落 embed 落到 `.mynotes/ai/embeddings.sqlite`，作为 D2b 对话面 RAG 检索的底座 + D1 打分升级（`title_jaccard` → `embedding_cosine`）。D2a 切成多个最小刀；**本章描述的 D2a.1 只落 Rust 库层**（无 IPC、无 UI、无真实 HTTP 调用），D2a.2+ 再分批接消费面。

#### 6.17.1 库层拆分与职责

`src-tauri/src/services/ai/` 三个子模块：

| 模块              | 职责                                                                                                             | 不做                                        |
| ----------------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| `provider`        | `AiProvider` trait（`embed(EmbedRequest) → EmbedResponse`）+ `ProviderError` 枚举 + `MockProvider` 参考实现      | 真实 HTTP（D2a.2）、API key 存储（D2a.2）   |
| `chunker`         | 纯函数 `chunk_markdown(body) → Vec<Chunk>`：跳 frontmatter → 段落切 → 大段按句子二次切；保留绝对 byte offset     | 语义切（代码块 / 标题感知）、token 实际计数 |
| `embedding_store` | `EmbeddingStore` 包住 `embeddings.sqlite`：open / upsert / delete_by_note / note_mtime / search (cosine) / stats | ANN / HNSW（vault > 50 k chunks 再考虑）    |

模块入口 `services/ai/mod.rs` 顶部挂 `#![allow(dead_code)]`，因为 D2a.1 里所有 pub API 的消费者都在 D2a.2+，单测是当前唯一的用户。D2a.2 接 IPC 时立即摘除这个属性。

#### 6.17.2 Provider 抽象要点

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    fn name(&self) -> &'static str;        // "openai" / "ollama" / "mock"
    fn default_dim(&self) -> usize;
    async fn embed(&self, req: EmbedRequest) -> Result<EmbedResponse, ProviderError>;
}
```

- **`async_trait` 而非原生 async fn in trait**：未来 `AppState` 持 `Box<dyn AiProvider>`，必须 dyn-compatible。
- **`ProviderError` 分五类**：`Network` / `Auth` / `RateLimit(retry_after_secs)` / `InvalidRequest` / `Other`。该粒度足够让调用方决策重试策略又不至于强耦合 HTTP 状态码。
- **`MockProvider`**：FNV-1a 滚动哈希 → 192 维单位向量；对同一 input 确定性、对不同 input 高概率不同。**不是语义有意义的**，纯粹为了让下游（chunker → store → search）在无网环境里有真实形状的向量跑联通。

#### 6.17.3 Chunker 策略（v1 启发式）

1. **Frontmatter 剥离**：识别 `---\n…\n---\n`（含 CRLF 变体），从 body 起点开始切。不终止的 frontmatter 被当作普通 body。
2. **段落切**：`\n\n+` 分隔；维护**绝对 byte offset**（回到原始输入的 `offset_start..offset_end`），便于 D2b 的引用高亮。
3. **大段二次切**：单段 `est_tokens > 800`（=4 char/token 粗估）时按 `. ! ? 。 ！ ？` + 后随空白切句；数字 `3.14` 这类不切。
4. **CJK 支持**：U+3002 / U+FF01 / U+FF1F 三个中文终止符编码进 `cjk_terminator_len`。

`est_tokens(s) = ⌈chars / 4⌉` 是偏保守（中文略高估），dry-run 时偏保守可接受——宁可用户看到比实际贵的估算也不被账单偷袭。

#### 6.17.4 `embeddings.sqlite` schema

```sql
CREATE TABLE embedding_chunks (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    note_rel_path TEXT    NOT NULL,
    chunk_index   INTEGER NOT NULL,
    offset_start  INTEGER NOT NULL,
    offset_end    INTEGER NOT NULL,
    text          TEXT    NOT NULL,
    model         TEXT    NOT NULL,        -- provider-specific identifier
    dim           INTEGER NOT NULL,
    vector        BLOB    NOT NULL,        -- little-endian f32 × dim
    note_mtime    INTEGER NOT NULL,        -- 用于增量重 embed 比对
    created_at    INTEGER NOT NULL,
    UNIQUE(note_rel_path, chunk_index, model)
);
CREATE INDEX idx_emb_note  ON embedding_chunks(note_rel_path);
CREATE INDEX idx_emb_model ON embedding_chunks(model);

CREATE TABLE embedding_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- schema_version = '1' written at bootstrap
```

关键决策：

- **与 `index.sqlite` 物理分离**：用户删除整个 `.mynotes/ai/` 目录即可"出厂重置 AI 派生数据"，不伤主索引。两个库独立的 WAL，长时间 embed 批跑不会阻塞前端写。
- **BLOB 存 f32 × dim，内存 cosine 扫描检索**：零新依赖，跟 `index.sqlite` 同构的运维心智；vault < 50 k chunks 下实测 < 50 ms。阈值突破时只换 `search()` 实现（上 `sqlite-vec` 或 `hnsw_rs`），schema 不改。
- **`UNIQUE(note_rel_path, chunk_index, model)` + UPSERT**：同一 chunk 重 embed 就地覆盖；不同 `model` 的向量在同一张表里天然分隔 namespace。
- **`dim` 冗余存储**：理论上每个 `model` 的 dim 固定，冗余存允许 `search()` 在读到 dim 不匹配的行时静默跳过（而非 panic），支持"多 provider 混用"场景。
- **`note_mtime` 字段**：D2a.3a / D2a.4 / D2a.5 都会消费它——watcher 用 `MAX(note_mtime) WHERE note_rel_path = ?` vs 文件系统 mtime 判定是否重 embed，整库初始化与 related-notes 升级也都依赖当前 model 下的向量是否已存在；D2a.1 先把字段存好、读接口(`note_mtime()`)备好。

#### 6.17.5 Search 语义

`EmbeddingStore::search(&self, query: &[f32], model: &str, limit: usize) -> Vec<SearchHit>`：

- 读所有 `model = ?` 的行 → 逐行解包 BLOB → 计算 cosine → 按 score 降序截到 `limit`。
- `query` 长度为 0 时返回 `InvalidRequest` 错误；query 范数为 0 时返回空结果（非错误）。
- `dim` 与 query 长度不匹配的行**静默跳过**（非错误）——允许用户切换 provider 后残留的旧 dim 向量不阻断查询。
- 空 store 返回 `Ok(vec![])`，供 UI 显示"尚未建索引"状态。

#### 6.17.6 测试覆盖（D2a.1 基线 24 条）

- `provider` 5 条：形状 / 确定性 / 互异 / 单位范数 / 空输入错误。
- `chunker` 14 条：`est_tokens` 2 / `strip_frontmatter` 4 / `chunk_markdown` 7 / `split_sentences` 2 / 内部边界 1。
- `embedding_store` 12 条：schema bootstrap / upsert / UPSERT 覆盖 / delete_by_note / note_mtime (missing + present) / search 排序 / model 过滤 / dim 不匹配跳过 / empty query / empty store / pack-unpack 回转 / norm 零向量。

一条"小 dogfood"测试值得强调：`search_returns_hits_in_descending_order` 在 store 里塞三个正交基底向量，query 用近 x 轴方向的混合向量验证排序结果——涵盖了"能找最近邻 + 排序正确 + 正交项接近 0 分"三条语义。

#### 6.17.7 V2 合规性自检

- **无写入 vault**：所有 SQLite 操作目标是 `.mynotes/ai/embeddings.sqlite`，不碰任何 `.md`。
- **不依赖网络（当前切片）**：D2a.1 无 HTTP 客户端依赖；OpenAI impl 留给 D2a.2。
- **可抛弃**：单个 SQLite 文件，删了重跑即可；meta 表的 `schema_version` 为未来 migration 预留。
- **可关闭**：与 D1 同样挂 `ai_enabled` 开关——开关关闭时 D2a.2+ 的 IPC 命令会在入口短路，不会初始化 provider 也不会打开 store。

#### 6.17.8 D2a 后续切片路线图（非本章范围但记录顺序）

| 切片   | 状态      | 交付物                                                                                                                 | 依赖                       |
| ------ | --------- | ---------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| D2a.2  | ✅ 已完成 | `OpenAiProvider`（OpenAI-compatible HTTP）+ API key keychain 存储 + 4 条 provider IPC + Settings UI                    | `keyring` + `reqwest` 依赖 |
| D2a.3a | ✅ 已完成 | `EmbeddingStore` 生命周期挂 AppState + 4 条 embed-note IPC + 命令面板 `> Embed current note` + Settings "AI 索引" 面板 | D2a.2                      |
| D2a.3b | ✅ 已完成 | watcher 挂 30 s debounce 增量 embed（create/modify 入队；delete 即时同步删除）                                         | D2a.3a                     |
| D2a.4  | ✅ 已完成 | Settings "初始化索引" 按钮 + dry-run modal（预估 chunks / tokens / $）                                                 | D2a.3b                     |
| D2a.5  | ✅ 已完成 | D1 打分信号升级（`title_jaccard` → `embedding_cosine`，消费本地 `embeddings.sqlite`）                                  | D2a.4                      |
| D2a.6  | ✅ 已完成 | 失败降级（API timeout / 配额 / 认证 / 网络失败的结构化 UX；原子替换 + 整库 init 提前中止）                             | D2a.5                      |

---

### 6.18 AI 辅助·Provider 接入（P3-D2a.2）

D2a.1 是库层底座，D2a.2 是**对外接第一根真实的水管**：加一个 OpenAI-compatible HTTP provider，把 API key 放进操作系统原生 keystore，在 Settings 里暴露一个能点的「测试连接」按钮。用户第一次真正能"配好一个 provider，看到 ✓"。

#### 6.18.1 依赖新增

| crate     | 版本   | 用途                                                                          | TLS 选择                                                                               |
| --------- | ------ | ----------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `reqwest` | `0.12` | HTTP 客户端                                                                   | `rustls-tls`（默认关闭 `default-features`，避开 OpenSSL / Secure Transport）           |
| `keyring` | `3`    | 跨平台 OS keystore（macOS Keychain / Windows CredMan / Linux secret-service） | 默认 feature 自动选 `apple-native` / `windows-native` / `linux-native-sync-persistent` |

**只加这两个 crate**。不引入 `wiremock` / `mockito` 做网络单测——错误映射在纯函数里验（见下文 6.18.4），真实 HTTP 通过 Settings 的「测试连接」人工走一遍。

#### 6.18.2 `OpenAiProvider`

`services/ai/openai.rs` 实现 `AiProvider` trait，说 `POST {base_url}/embeddings` 这个方言。同一个协议覆盖：

- **OpenAI** · `https://api.openai.com/v1`
- **OpenRouter** · `https://openrouter.ai/api/v1`
- **Ollama**（本地）· `http://localhost:11434/v1`（内置 OpenAI-compat shim）
- **LM Studio / vLLM / Together.ai / Groq / …**

切换后端 = 在 Settings 改 `base_url` 字段，**不需要新写 provider 代码**。

**字段职责**：

| 字段          | 作用                                                                                                                           |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `base_url`    | 尾斜线会被 strip，拼 `/embeddings`                                                                                             |
| `model`       | 默认模型；`embed()` 请求里的 `model` 若为空，落到这个值                                                                        |
| `api_key`     | 空字符串 = 跳过 `Authorization` header（给 Ollama 本地场景）                                                                   |
| `default_dim` | `AtomicUsize`，构造时填 1536（`text-embedding-3-small`），首次 embed 成功后用实际返回向量长度自动覆盖，下游拿 dim 做一致性检查 |

**超时**：默认 60 s；`with_timeout(Duration)` 可覆盖，`test_connection` 走 10 s。

#### 6.18.3 `SecretStore` 抽象

`services/ai/secrets.rs` 定义 trait + 两个实现：

```rust
pub trait SecretStore: Send + Sync {
    fn set_api_key(&self, provider: &str, secret: &str) -> Result<(), SecretError>;
    fn get_api_key(&self, provider: &str) -> Result<Option<String>, SecretError>;
    fn delete_api_key(&self, provider: &str) -> Result<(), SecretError>;
    fn has_api_key(&self, provider: &str) -> Result<bool, SecretError>;
}
```

- **`KeyringSecretStore`**（生产）：zero-size struct，`keyring::Entry::new("com.mynotes.ai", provider)` 即用即弃；**首次调用会触发系统 keychain 授权对话框**，这是 OS 安全模型，不是我们的 bug。
- **`MockSecretStore`**（`#[cfg(test)]`）：内存 `HashMap<String, String>`，7 条单测覆盖 set/get/overwrite/delete idempotency/has/空输入 reject。CI 不碰 Keychain。

**命名空间**：service = `"com.mynotes.ai"` 常量，account = provider kind（`"openai"`）。未来加 `"anthropic"` 不会冲突。

**一条边界**：API key 只有**两个**合法写路径 —— 用户在 Settings 粘贴 + 通过 `ai_provider_set_config` 写进 keystore。**不**暴露 `ai_provider_get_api_key`——secret 一旦存进 keystore 就只由命令层在 `embed` 路径上读出用一次，永不回 IPC / 前端。

#### 6.18.4 错误映射（`classify_http_error`）

HTTP status → `ProviderError`：

| status          | 映射                            | 前端展示                           |
| --------------- | ------------------------------- | ---------------------------------- |
| 401 / 403       | `Auth(msg)`                     | `auth: Incorrect API key`          |
| 429             | `RateLimit(30)`                 | `rate_limit: retry after 30s`      |
| 400-499         | `InvalidRequest(msg)`           | `invalid_request: model not found` |
| 500-599         | `Other("server {status}: ...")` | `other: server 500: ...`           |
| 连接失败 / 超时 | `Network(e.to_string())`        | `network: ...`                     |

`extract_error_message()` 先尝试 OpenAI envelope `{error:{message,type}}`；失败则 fallback 到 raw body 前 400 字符 + `…`（考虑 UTF-8 `…` 是 3 字节）。

#### 6.18.5 IPC 命令

4 条新命令（均走 `commands/ai.rs`）：

| 命令                          | 签名                                                                         | 说明                                                                        |
| ----------------------------- | ---------------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `ai_provider_set_config`      | `(kind, base_url, embed_model, api_key)`                                     | `api_key == ""` 表示不动 keystore（只改 config）；非空覆盖                  |
| `ai_provider_clear_config`    | `()`                                                                         | 清 config + keystore wipe 尽力而为（先 config 后 keystore，半清也好过卡死） |
| `ai_provider_has_api_key`     | `() -> bool`                                                                 | Settings 红/绿徽标专用；不返回 key 本身                                     |
| `ai_provider_test_connection` | `(kind?, base_url?, embed_model?, api_key_override?)` → `ProviderTestResult` | 所有字段可选；显式传入 = 验"未保存的表单"；缺省 = 用持久化值                |

**关键设计**：`ProviderTestResult` 是**结构体而非 Result**，成功失败都能在同一个 notice 里渲染，前端少一道分支判断：

```rust
pub struct ProviderTestResult {
    pub ok: bool,
    pub dim: Option<usize>,
    pub total_tokens: Option<u32>,
    pub error_kind: Option<String>,   // network / auth / rate_limit / invalid_request / other
    pub error_message: Option<String>,
}
```

#### 6.18.6 `AppPreferences.ai_provider` 持久化

`services/config.rs` 新字段：

```rust
pub struct AiProviderConfig {
    pub kind: String,          // "openai"
    pub base_url: String,       // 无尾斜线
    pub embed_model: String,    // "text-embedding-3-small"
}
```

**不含** `api_key`。`app-config.json` 永不可见 secret。

`AppConfigSnapshot.ai_provider: Option<AiProviderConfig>` —— `None` = 用户还没点过保存；前端 Settings 显示空表单。`has_api_key` 的状态走**独立 IPC**（`ai_provider_has_api_key`）查询，因为 `snapshot()` 是同步的、不该触发 keystore IO。

#### 6.18.7 前端 Settings UI

Settings modal 在"AI 辅助面板"区块之后追加"AI Provider（Embedding）"区块，三栏表单 + 三按钮：

- **Base URL** / **Embed model** / **API key**（type=password；placeholder 根据 `aiProviderHasKey` 切"已存储在 keychain，留空以保留"/"粘贴 sk-... 或留空（Ollama）"）
- **测试连接**（绑 `aiProviderTestConnection`）| **保存**（绑 `aiProviderSetConfig`）| **清除**（绑 `aiProviderClearConfig`，带 `ask()` 二次确认）

测试结果 notice 区块展示成功维度 / tokens 或失败 kind / message。**`apiKeyOverride` 只在用户当前表单填了 key 时才带过去**；否则后端回落到 keystore 查询。这保证"只改 base_url 然后测试连接"也能工作。

`pushNotice('AI 设置已保存', 'info')` 做保存成功反馈 —— 与现有设置项的反馈模式一致。

#### 6.18.8 测试覆盖（D2a.2 基线 15 条新增）

- `openai::tests` × 12：
  - `embeddings_url` 两种 base_url 拼接
  - `classify_http_error` 401/403/429/400/500 五种状态
  - `extract_error_message` envelope / 纯文本 / 过长截断（UTF-8 chars count） / 空 body
  - `EmbeddingResponseBody` serde 解析 OpenAI shape + Ollama 无 usage 的 shape
  - `#[tokio::test] empty_inputs` 不触发 HTTP 直接报 `InvalidRequest`
- `secrets::tests` × 7（全部 MockSecretStore）：
  - roundtrip / overwrite / missing → None / delete idempotent / has_api_key / empty provider / empty secret / 多 provider 隔离

**不测的**：

- 真实 keyring 往 macOS Keychain 写入——会弹系统对话框；用户手动验
- 真实 HTTP 到 OpenAI / Ollama——用户点"测试连接"人工验

#### 6.18.9 D2a.2 不做的事

- ❌ **重试 / backoff**：错误直接返回，重试策略由 D2a.3 的 watcher 层决定（embed 批量重跑时机）
- ❌ **batch 拆分**：OpenAI 单请求 96 input 上限，批次合并由 D2a.3 负责；D2a.2 的 `embed` 只是透传
- ❌ **chat / streaming**：留给 D2b；本切片 provider trait 只实现 `embed`
- ❌ **存 key 前测试**：保存和测试是两个动作，允许用户保存错的 key（后续 embed 会失败，届时在 UI 中引导去 Settings 修）

#### 6.18.10 V2 合规性自检

| §V2 原则                 | D2a.2 实现如何符合                                                                                                               |
| ------------------------ | -------------------------------------------------------------------------------------------------------------------------------- |
| 数据本地 · Offline-first | config 存 `app-config.json` 本地；api_key 存 OS keystore 本地；可完全连本地 Ollama 不触网                                        |
| Vault-scoped             | 不污染 vault —— provider config 是 app 级而非 vault 级（用户换 vault 不用重配）                                                  |
| 不自建 auth              | API key 交给 OS keystore，自己不做任何加密层                                                                                     |
| 可逆 · 可关闭            | "清除"按钮 = 删 config + wipe keystore，回到首启动状态                                                                           |
| 结构化错误               | `ProviderError` 五类标签 + `ProviderTestResult.error_kind` 让前端做不同 UX（auth 引导去改 key / rate_limit 延后 / network 重试） |

---

### 6.19 AI 辅助·手动 Embed 管道（P3-D2a.3a）

D2a.2 把"水管接上"（provider 可测试），D2a.3a 把"水龙头装好"（一刀下去能看到一篇笔记从 md → chunks → vectors → sqlite 全跑通）。这一刀的关键取舍是**故意不接 watcher**——先让手动触发完整闭环跑顺，下一刀 D2a.3b 再把 watcher 自动增量接上；这样 D2a.3b 只需要关注"事件→队列→debounce"一件事，不用同时 debug 流水线正确性。

#### 6.19.1 原 D2a.3 为什么拆成 3a + 3b

原计划里 D2a.3 是"IPC + watcher 一刀"。拆开的理由：

- **依赖方向干净**：watcher 集成是 push-driven（事件源驱动），IPC 是 pull-driven（用户/UI 驱动）。先把 pull 跑通后 push 可以复用 `embed_service::embed_note`，代码量比"一次都做"少。
- **可验证面扩大**：手动触发的 IPC 能在 UI 上单步跑，失败定位比 watcher 的 30 s 异步路径容易 10 倍。
- **每刀都有可感反馈**：这与 D2a.1/D2a.2 的节奏一致——用户每个切片都看得到"新增什么能点"。

#### 6.19.2 AppState 生命周期挂载

`EmbeddingStore` 与 `index.sqlite` 对称挂到 `AppState`：

```rust
pub struct AppState {
    // … 原有字段省略
    pub embeddings: Mutex<Option<Arc<Mutex<EmbeddingStore>>>>,
}
```

`vault_open` / `vault_init`（通过 `attach_index`）钩子做三件事：

1. drop 旧 vault 的 `embeddings`（释放文件句柄）
2. 打开 `<vault>/.mynotes/ai/embeddings.sqlite`——**打开失败时 log + continue**，不阻断 vault open。打不开意味着 AI 功能暂停，但主索引/watcher/文件编辑全部正常。
3. 包进 `Arc<Mutex<_>>` 挂进 `state.embeddings`

选择"非致命"而不是"致命"的原因：SQLite 打开失败的真实场景多是磁盘满 / 权限错乱 / iCloud 正在同步 —— 让用户能用 MyNotes 的主功能比"只要 AI 不行就罢工"合理得多。错误在第一次 `ai_embed_note` 被调用时以 `AppError::Other("embedding store unavailable")` 浮现，前端 toast 足够清晰。

#### 6.19.3 `embed_service::embed_note` 流水线

单个函数 orchestrate 整条链路：

```
rel_path
  ↓ std::fs::read_to_string + metadata.modified (unix 秒)
raw markdown + mtime
  ↓ chunker::chunk_markdown
Vec<Chunk>                     ── 若 0 → SkipReason::Empty
  ↓ 查 store.note_mtime(rel_path)
stored_mtime == mtime ?        ── 是 → SkipReason::UpToDate
  ↓ 否
.chunks(MAX_BATCH_INPUTS=64)
  ↓ provider.embed(batch)（可能多批）
Vec<Vec<f32>> + tokens_used
  ↓ 拼 StoredChunk[]
store.delete_by_note(rel) + store.upsert_chunks(…)（事务里原子）
  ↓
EmbedOutcome { chunks_embedded, tokens_used, skipped: None }
```

三个关键设计点：

1. **mtime-based 增量**：skip 判定看的是"文件 mtime == store 中该 note 在**当前 model** 下的最新 mtime"。优点是一次 SQLite 查询即可判定（不需要 hash 文件内容），且换模型后不会被旧向量误判成 up-to-date。缺点是 `touch foo.md` 会引发假阳性重跑——代价可接受；D2a.4 的整库初始化流程（必要时先清空 AI 索引）会在用户期望强制时补齐。
2. **MAX_BATCH_INPUTS = 64**：OpenAI 单次 `/embeddings` 上限 96；本地 Ollama 在某些配置下对超过 ~40 的 batch 会拒绝。选 64 留一点双边余量。单条笔记典型 chunks 数 5–20，基本一次打过去；大型 moc 会切 2 批。
3. **delete_by_note 先于 upsert**：`upsert_chunks` 只按 `(note_rel_path, chunk_index, model)` UNIQUE 覆盖。若用户把笔记从 10 段删到 3 段，index 4–9 会变成孤立 chunk。先 `delete_by_note` 保证增量不留污染。

#### 6.19.4 IPC 命令矩阵（新增 4 条）

| IPC                              | 签名               | 职责                                                                   |
| -------------------------------- | ------------------ | ---------------------------------------------------------------------- |
| `ai_embed_note(rel_path)`        | `→ EmbedOutcome`   | 单文件 embed；路径遍历 reject；provider 未配置时返回 `AppError::Other` |
| `ai_embed_stats()`               | `→ EmbeddingStats` | 聚合计数（chunks / notes / models）；无 vault 时返回全零，调用永不失败 |
| `ai_embed_delete_note(rel_path)` | `→ usize`          | 删除单笔记全部 chunks；返回删除行数                                    |
| `ai_embed_clear_all()`           | `→ u64`            | 清空整个 embedding 表；返回清空前的 chunk_count                        |

`EmbedOutcome` 故意用 `skipped: Option<SkipReason>` 而非抛错表示跳过——`up_to_date` / `empty` 是**成功但无事可做**，UI 应显示灰色 toast 而不是红色错误。这与 D2a.2 的 `ProviderTestResult` 同款思路：IPC 边界返回结构体，前端做单一分支。

`build_configured_provider` 辅助函数把"读 config → 读 keychain → 组装 `OpenAiProvider`"串起来，被 embed 调用复用；配置缺失时返回清晰的 `AppError::Other`，不依赖全局单例。

#### 6.19.5 EmbeddingStore 新增方法

| 方法               | 签名              | 用途                                                                  |
| ------------------ | ----------------- | --------------------------------------------------------------------- |
| `clear_all(&self)` | `→ AppResult<()>` | 非事务单 `DELETE FROM embedding_chunks`；由 `ai_embed_clear_all` 使用 |

`note_mtime(&self, rel_path)` / `upsert_chunks(&mut self, chunks)` / `delete_by_note(&self, rel_path)` / `stats(&self)` 早在 D2a.1 就存在，这刀零改动。

#### 6.19.6 前端集成

- **`src/lib/ipc/ai.ts`**：新增 `aiEmbedNote` / `aiEmbedStats` / `aiEmbedDeleteNote` / `aiEmbedClearAll` + 类型 `EmbedOutcome` / `EmbeddingStats` / `EmbedSkipReason`
- **命令面板**：新增 `> Embed current note (AI index)`，`when` 谓词 = 有开笔记 + `.md` + 不在 `.mynotes/`——和 `> Show Related Notes` 同族
- **Settings 模态**：在「AI Provider」小节下方新增「AI 索引 · Embedding」小节，展示 `已索引 N chunks · M notes · K 模型` + "Embed 当前笔记" / "清空 AI 索引" 两个按钮 + 结果 toast；`openSettings` 触发 `refreshEmbedStats()` 懒加载
- **Toast 语义**：成功 embed 显示"Embedded N chunks · X tokens"（0 tokens 时省略），skip 显示"Up to date"/"Note is empty"，失败展开 provider 错误消息

#### 6.19.7 测试覆盖（新增 6 条）

| 测试                                            | 场景                                        | 验证                                             |
| ----------------------------------------------- | ------------------------------------------- | ------------------------------------------------ |
| `empty_note_is_skip_empty`                      | 完全空文件                                  | `skipped == Some(Empty)`，`chunks_embedded == 0` |
| `frontmatter_only_is_skip_empty`                | 只有 `---\ntitle: a\n---\n`                 | 同上（chunker 剥去 frontmatter 后剩 0 段）       |
| `basic_run_embeds_and_persists`                 | "# hello\n\nfoo\n\nbar"                     | 写入 3 chunks，stats 一致                        |
| `second_run_with_same_mtime_is_up_to_date`      | 连续两次 embed 同文件                       | 第二次 `skipped == Some(UpToDate)`               |
| `edit_reduces_chunk_count_stale_chunks_cleaned` | 3 段改成 1 段 + sleep(1.1s) 确保 mtime 变化 | 重跑后 chunk_count 从 3 变 1（不残留）           |
| `missing_file_surfaces_error`                   | 不存在的 rel_path                           | `AppError::Other` 包含 "read"                    |

所有测试使用 `MockProvider` + `EmbeddingStore::open_in_memory()`，不触网不写盘。sleep(1.1s) 是为了越过 FS mtime 秒级粒度——HFS+/APFS 的 mtime 精度是秒，1 秒内两次写入 mtime 可能相等。

#### 6.19.8 有意**不**做

- ❌ **Watcher 自动增量**：D2a.3b 单独一刀做
- ❌ **Batch cross-note**：`ai_embed_note` 只处理一篇；批量入口留给 D2a.4 "初始化索引" modal
- ❌ **Rate limit 自动重试**：单次失败直接冒泡；D2a.3b 的 debounce 自然构成 "30 s 后再试" 语义
- ❌ **成本估算**：dry-run modal 是 D2a.4
- ❌ **检索 UI**：D2a.5 已开始消费 `embeddings.sqlite` 做 related-notes 打分，但 `EmbeddingStore::search()` 这条 chunk-level 检索能力仍未直接暴露为独立 IPC / UI

#### 6.19.9 V2 合规性自检

| §V2 原则                 | D2a.3a 实现如何符合                                                   |
| ------------------------ | --------------------------------------------------------------------- |
| 数据本地 · Offline-first | embeddings.sqlite 落在 `<vault>/.mynotes/ai/`；删了即可 factory-reset |
| Vault-scoped             | 索引与 vault 绑定；切 vault 自动 drop 旧 store + 重新打开             |
| 可逆 · 可关闭            | "清空 AI 索引" 按钮 = 一次性 DELETE；不 touch 任何 `.md`              |
| Fail soft                | `embeddings` 打开失败时 log + continue；主功能全部可用                |
| 结构化跳过               | `SkipReason` 让 UI 区分"没事可做"和"真失败"，避免假错误噪声           |

### 6.20 AI 辅助·Watcher 增量 Embed（P3-D2a.3b）

D2a.3a 已经证明"单篇 `.md` → chunks → vectors → sqlite"这条流水线是通的，D2a.3b 只做一件事：把它挂回现有 watcher，让 embedding 索引跟着笔记编辑自动追上，而不是继续靠命令面板手动点击。

#### 6.20.1 共享运行时决策

命令层和 watcher 都需要同两件判断：

1. **现在是否允许 auto-embed？**
2. **如何从持久化配置 + keychain 组装出一个 live provider？**

为避免 `commands/ai.rs` 和 `services/watcher.rs` 各自复制一份 provider bootstrap，本刀新增 `services/ai/runtime.rs`：

- `auto_embed_enabled(cfg)`：`ai_enabled == Some(false)` 才真关闭；`None` 继承前端默认语义（即视为开启）。同时要求 `ai_provider.kind/base_url/embed_model` 三字段非空，配置不完整即判定"未就绪"。
- `build_provider_from_config(cfg, secrets)`：从 `AiProviderConfig` + `SecretStore` 组装 `OpenAiProvider`，允许 API key 为空（兼容 Ollama / 本地 OpenAI-compatible 服务）。
- `build_configured_provider(cfg, secrets)`：从 `ConfigStore` 快照里取 provider 配置并复用上面的组装逻辑。

`commands/ai.rs` 继续复用这组 helper；watcher 也只调用这组 helper。这样 D2a.4 以后如果 provider 配置规则调整，只改一处即可。

#### 6.20.2 Watcher 变成"双节奏"队列

SQLite 主索引的 watcher 语义保持不变：`notify-debouncer-full` 仍以 **200 ms** 窗口合并 bursty 文件事件，并对每个 `.md` 执行 `scanner::reindex_one` / `scanner::delete_one`。

D2a.3b 在此基础上追加一条更慢的 AI 队列：

- **消息形态**：`AiWatchMsg::{Upsert(rel_path), Delete(rel_path)}`
- **调度结构**：`AiDebounceQueue { deadlines: HashMap<String, Instant> }`
- **节奏**：
  - `create/modify` → `queue_upsert(rel)`，把该 note 的 deadline 刷到 `now + 30 s`
  - `delete` → `queue_delete(rel)` + 立即 `EmbeddingStore::delete_by_note(rel)`
  - worker 空闲时按最近 deadline `recv_timeout`，到点后批量 `pop_due()`，按路径排序后逐个跑 `embed_service::embed_note`

结果是：

- 用户连敲 20 次保存，只会在最后一次保存后的 30 秒触发一次 embed。
- 真正的删除路径不走网络，不等 30 秒，直接清 stale vectors。
- SQLite 索引和 AI 索引互不阻塞：前者追求"快看到文件树/面板正确"，后者追求"别在编辑风暴里反复打 provider"。

#### 6.20.3 事件过滤与边界语义

AI 队列只消费和首轮扫描一致的 markdown 子集：

- 只看 `.md`
- 跳过任何 hidden path segment（例如 `.mynotes/`）
- 跳过 `attachments/`

另有两个刻意的边界处理：

1. **`create/modify` 仅在 auto-embed ready 时入队**：也就是 `ai_enabled != false` 且 provider 配置完整。配置没配好时不积压历史队列，避免用户一保存就默默累积一堆"将来某天一起打"的隐性状态。
2. **`create/modify` 事件若当前路径已经不存在，按 delete 处理**：这是为 rename / 外部同步器的一些 event 形态兜底。某些 watcher 事件会把"旧路径消失"包在 `Modify` 里而不是 `Remove`，此时如果还去 `Upsert` 会保留脏 embedding；改成 `Delete` 更稳妥。

#### 6.20.4 `AppState` 与线程共享

为了让 watcher 能看见**实时配置**而不是启动时快照，本刀把 `AppState.config` 从 `Mutex<ConfigStore>` 调整为 `Arc<Mutex<ConfigStore>>`。`vault_open / vault_init` 仍在主线程里更新 config，但 watcher 线程现在可以持有同一把锁的 clone：

- `attach_index(...)` 启动 watcher 时把 `state.config.clone()` 和 `state.embeddings_handle()` 一起传入
- 若某个 vault 的 `EmbeddingStore` 打不开，watcher 仍正常跑 SQLite 增量索引，只是不启动 AI worker
- 切 vault 时先 drop 旧 watcher / 旧 embedding store，再挂新 watcher，避免旧 vault 的尾部事件误落到新 store

#### 6.20.5 flush 阶段与失败模式

AI worker 到点 flush 时，先重新读取 config：

- 若此刻 `auto_embed_enabled == false`，直接放弃这批 due notes
- 若 provider 组装失败（例如 keychain 失效 / base_url 为空），记录 warning 并放弃这批 due notes
- 若 provider 正常，针对每个 due note 调 `embed_service::embed_note(store, provider, model, vault, rel)`

`embed_service` 的三类结果在 watcher 中分别处理：

- `SkipReason::UpToDate` → debug log
- `SkipReason::Empty` → debug log
- 真正 embed 成功 → info log（记录 note / chunks / tokens）
- 失败 → warning log

这里故意**不引入 durable retry queue**。D2a.3b 的目标只是把"最近一次稳定内容"自动追进 embedding store，而不是开始做作业系统级任务调度。更强的 backoff / retry / usage 记账留给 D2a.6 与 D2b。

#### 6.20.6 测试覆盖

本刀新增 9 条 Rust 单测：

- `services/ai/runtime.rs` × 6
  - `ai_enabled == None` 仍视为 auto-embed 开启
  - 显式 `false` 会关闭
  - provider 配置不完整不会误启队列
  - provider 允许空 API key
  - 空 `kind` 会报清晰错误
  - 能从 secret store 读回已保存的 key
- `services/watcher.rs` × 3
  - `queue_upsert` 会 debounce 且对同一路径去重
  - `queue_delete` 会取消 pending upsert
  - `watched_markdown_rel` 会稳定过滤 hidden / attachments / 非 markdown

本轮代码层验证基线：

- `cargo test --manifest-path src-tauri/Cargo.toml` → `154 passed`
- `pnpm check` → `0 errors, 0 warnings`

#### 6.20.7 有意不做

- ❌ **Settings 全量初始化 / dry-run 估算**：那是 D2a.4
- ❌ **后台进度 UI / toast**：watcher 自动 embed 只写 log，不在当前桌面 UX 里增加噪声
- ❌ **失败持久重试队列**：provider 不可用时丢弃当前 due 批次，等待下一次文件变更重新入队
- ❌ **跨 vault / 跨目录批量调度中心**：仍坚持"每个 vault 一套独立派生层"的简单模型

### 6.21 AI 辅助·整库初始化（P3-D2a.4）

D2a.3b 之后，系统已经具备"从现在开始自动追增量"的能力，但**旧 vault 的历史笔记仍需要一条显式的整库初始化入口**。D2a.4 的目标就是把这条入口做成安全、可估算、不会一按钮闷头跑远程账单的两步流。

#### 6.21.1 后端：preview / run 两条 IPC

新增两条 IPC：

| IPC                        | 签名                    | 语义                                                                                                                        |
| -------------------------- | ----------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `ai_embed_vault_preview()` | `→ VaultEmbedPreview`   | 纯 dry-run；walk 全 vault markdown，比较当前 model 下的 mtime，估算待初始化 notes/chunks/tokens/成本，不写盘、不打 provider |
| `ai_embed_vault_run()`     | `→ VaultEmbedRunResult` | 确认后执行整库初始化；逐 note 复用 `embed_service::embed_note`，汇总成功 / up-to-date / empty / failed                      |

`services/ai/init_service.rs` 把这两条 IPC 的核心逻辑集中起来：

- `preview_vault_embed(vault, store, provider_cfg)`：
  - 复用 `scanner::walk_vault_md`，沿用 hidden path / `attachments/` 过滤口径
  - 对每篇笔记先 `chunk_markdown`，空笔记计入 `note_count_empty`
  - 非空笔记用 `EmbeddingStore::note_mtime_for_model(rel, model)` 对比当前 model 的 mtime，决定是 `up_to_date` 还是 `to_embed`
  - 只返回前 100 条待初始化路径预览，防止 modal 爆长
- `embed_vault(store, provider, model, vault)`：
  - 同样 walk 全 vault markdown，但执行阶段不再预先筛选，直接逐 note 复用 `embed_service::embed_note`
  - `SkipReason::UpToDate` / `Empty` 归类到汇总结果，真正的 provider / 读文件错误记入 `failure_preview`
  - **不在第一处失败就 abort**，而是尽可能推进整批初始化，让用户拿到"成功多少 / 失败多少"的全貌

#### 6.21.2 成本估算策略

`VaultEmbedPreview` 除了 notes / chunks / tokens，还会返回一组**建议性**成本估算：

- **本地 provider**（`localhost` / `127.0.0.1` / `::1`）→ 直接按 `$0` 估算
- **OpenAI 官方 host + 已知 embedding model** → 用官方公开单价估算
  - `text-embedding-3-small` → `$0.02 / 1M tokens`
  - `text-embedding-3-large` → `$0.13 / 1M tokens`
  - `text-embedding-ada-002` → `$0.10 / 1M tokens`
- **其他 OpenAI-compatible provider**（OpenRouter / vLLM / 自建 proxy / 未知模型）→ 成本显示为"未知"

这里刻意保持保守：只有在**已知 host + 已知 model** 时才给美元数字，避免把第三方 provider 的计费逻辑错当成 OpenAI。

#### 6.21.3 `note_mtime` 改成 model-scoped

为了让 D2a.4 在"用户换了 embedding model 但文件没改"的场景下仍能正确工作，本刀顺手把 `EmbeddingStore` 增补为：

```rust
note_mtime_for_model(note_rel_path, model) -> Option<i64>
```

`embed_service::embed_note` 也改为基于 **当前 model** 判定 `SkipReason::UpToDate`。否则旧模型写下的同 mtime 向量会把新模型的整库初始化误短路，结果是当前 model 在大量旧 note 上根本没有向量可用。

这一改动不会引入多模型共存语义：`embed_note` 成功后仍然 `delete_by_note + upsert_chunks`，等价于"当前 active model 取代旧 model 成为该 note 的唯一 embedding 真相"。

#### 6.21.4 前端：Settings 两阶段 flow

`src/routes/+page.svelte` 的「AI 索引 · Embedding」区在 D2a.4 升级为：

- 按钮区：`Embed 当前笔记` / `初始化索引` / `清空 AI 索引`
- 点击 `初始化索引`：
  1. 先调 `aiEmbedVaultPreview()`
  2. 弹出现有 `.modal-preview` 风格的预览 modal
  3. modal 显示：
     - 待初始化 notes 数
     - total markdown / up-to-date / empty 分桶
     - 预计 chunks / tokens / 成本
     - 前 100 条待初始化笔记路径
  4. 用户再点 `开始初始化`，才调用 `aiEmbedVaultRun()`

执行完成后：

- 成功 → `embedNotice` 汇总 `已写入 N 篇 / X chunks / Y tokens`
- 部分失败 → 同一 notice 带 `失败 M 篇（例如 path — error）`
- 并发保护：`embedBusy` / `embedInitPreviewLoading` / `embedInitRunning` 三态统一折叠成 `embedActionBusy`，避免单 note embed / clear-all / full init 彼此重叠

#### 6.21.5 有意不做

- ❌ **单独的“强制重建全部”开关**：当前 force path 是 `清空 AI 索引` 后再点 `初始化索引`
- ❌ **后台进度条 / 百分比**：整库初始化结束前不做逐 note UI 流水；先用 summary 收口
- ❌ **第三方 provider 精确计费插件化**：本轮只内置 OpenAI 官方 embedding 单价；其他 provider 明确显示未知

#### 6.21.6 测试覆盖

本刀新增 7 条 Rust 单测：

- `services/ai/init_service.rs` × 6
  - preview 统计 `to_embed / up_to_date / empty`
  - preview 按当前 model 判定，不会被旧 model 污染
  - preview 列表稳定排序并截断到 100
  - local provider 成本 = `$0`
  - OpenAI 官方已知 model 成本映射正确
  - 整库执行会正确汇总 success / skip / empty
- `services/ai/embedding_store.rs` × 1
  - `note_mtime_for_model` 按 model 隔离

代码层验证基线：

- `cargo test --manifest-path src-tauri/Cargo.toml` → `161 passed`
- `pnpm check` → `0 errors, 0 warnings`

### 6.22 AI 辅助·related-notes 向量打分升级（P3-D2a.5）

D2a.4 之后，`embeddings.sqlite` 已经能稳定累积整库向量；D2a.5 的目标就是把这份本地派生层真正接回 D1 的用户可见能力里，让 related-notes 从"标题相似"升级为"语义相近"，同时保持**命令签名、面板结构、无网络请求**都不变。

#### 6.22.1 后端：`title_jaccard` → `embedding_cosine`

`src-tauri/src/commands/ai.rs::ai_related_notes` 保持原签名：

```rust
ai_related_notes(src_rel_path, limit?) -> Vec<RelatedNote>
```

但第四个信号从：

- `title_jaccard`

升级为：

- `embedding_cosine`

具体实现：

- `EmbeddingStore::note_cosine_scores(note_rel_path, model)`：按 `model` 扫描本地 chunk 向量，把同一 note 的 chunk vector 做求和聚合，再计算 note-level cosine；负值 clamp 到 0，保持信号范围仍在 `[0, 1]`
- `EmbeddingStore::only_model_name()`：当 Settings 里当前没有有效 provider 配置，但库里只存在一个 model namespace 时，允许 related-notes 继续自动消费已有向量
- `ai_related_notes` 的 model 解析策略：
  - 优先使用当前配置里的 `embed_model`
  - 若配置缺失且 store 中仅有一个 distinct model，则回退到该 model
  - 其余情况不报错，直接让 `embedding_cosine = 0`

#### 6.22.2 前端：类型与文案同步

前端不新增任何新入口，只做语义对齐：

- `src/lib/ipc/ai.ts`：`RelatedSignals.title_jaccard` 改为 `embedding_cosine`
- `src/lib/panel/Panel.svelte`：
  - tooltip 从「标题相似」改为「语义相近」
  - AI badge hover 从「本地启发式打分」改为「本地索引打分」
- `src/routes/+page.svelte` 的 Settings 提示文案改为：
  - 基础信号仍是 tag / direct-link / co-citation
  - 完成 AI 索引初始化后额外叠加语义向量相似度

#### 6.22.3 有意不做

- ❌ **不新增 search / semantic search IPC**：这轮只升级 existing related-notes，不单独开放 chunk-level 检索页
- ❌ **不做 ANN / sqlite-vec**：当前仍是全表内存 cosine，规模阈值未到之前先保持简单
- ❌ **不强制“必须先配置 provider 才能看 related-notes”**：当前策略是"能用已有向量就用，无法确定 active model 时静默退到 0"

#### 6.22.4 测试覆盖

本刀新增 / 调整的重点测试：

- `services/ai/embedding_store.rs`
  - `only_model_name_requires_exactly_one_distinct_model`
  - `note_cosine_scores_aggregate_chunks_by_note`
  - `note_cosine_scores_empty_when_source_missing`
- `commands/ai.rs`
  - 保留 `staleness` 与组合打分纯函数测试，权重语义改为 `embedding_cosine`

代码层验证基线：

- `cargo test --manifest-path src-tauri/Cargo.toml` → `155 passed`
- `pnpm check` → `0 errors, 0 warnings`

### 6.23 AI 辅助·失败降级 UX（P3-D2a.6）

D2a.5 之后，embedding 索引底座已经“能跑”，D2a.6 的目标是把它补到“出错时也可放心使用”：

- 单篇 embed 失败时，UI 要告诉用户该去检查 API key、模型名、网络还是额度；
- 整库初始化遇到 provider 级失败时，不能傻跑完整个 vault；
- 任何失败都不应把已有向量清到一半。

#### 6.23.1 原子替换：避免 delete 之后半路失败

`EmbeddingStore` 新增：

```rust
replace_note_chunks(note_rel_path, chunks) -> usize
```

实现上把 “`DELETE old chunks` + `INSERT new chunks`” 收到**同一个 SQLite 事务**里。这样即使插入阶段报错，整笔事务也会回滚，旧向量不会被先删后丢。

这比 D2a.3a 的 `delete_by_note + upsert_chunks` 两步调用更稳，是真正意义上的“失败不污染索引”。

#### 6.23.2 Provider 错误保留正文与 retry 信息

`ProviderError::RateLimit` 从只带秒数升级为：

```rust
RateLimit { retry_after_secs, message }
```

原因很直接：429 在真实 provider 上既可能是“稍后再试”，也可能是“余额/配额用尽”。如果只把它压成 `retry after 30s`，前端根本没法分辨应该建议用户“等一会儿”还是“去看 billing”。

`describe_provider_error()` 把 provider 错误统一展开为：

- `kind`
- `message`
- `retry_after_secs?`

供测试连接、单篇 embed、整库初始化三条路径复用。

#### 6.23.3 单篇 embed：结构化失败，不再塌成字符串 reject

`ai_embed_note` 的返回改为：

```rust
EmbedNoteResult {
  ok,
  outcome?,
  failure?,
}
```

其中 `failure` 会带：

- `kind`：`network / auth / rate_limit / invalid_request / other`
- `message`
- `retry_after_secs?`
- `store_unchanged`

前端据此把 notice 分成几类：

- 网络失败：提示检查 Base URL / 本地服务 / 网络
- 认证失败：提示检查 API key / provider 权限
- rate limit / quota：区分“稍后重试”和“余额/配额不足”
- invalid request：优先提示模型名或协议不匹配
- 配置缺失：直接引导回 Settings 补全 provider

#### 6.23.4 整库初始化：provider 级失败提前中止

`VaultEmbedRunResult` 新增：

- `note_count_not_attempted`
- `aborted_early`
- `aborted_error_kind?`
- `aborted_error_message?`
- `aborted_retry_after_secs?`

策略是：

- **继续**：单文件 read/stat 之类的 `other` 失败，记入 failed preview，但继续跑后续 notes
- **提前中止**：`network / auth / rate_limit / invalid_request`

也就是说：

- 如果只是某一篇文件坏了，不影响整批初始化继续推进；
- 如果 provider 本身就不可用、鉴权错误、模型错了或额度耗尽，整批任务会在首个明确失败点停下，并把“尚未尝试的 notes 数”和失败原因一起返给前端。

#### 6.23.5 前端 UX 语义

`src/routes/+page.svelte` 这轮不新建 toast 通道，只继续复用现有的 `ai-provider-test-result` / `embedNotice` 区块，但文案升级为：

- 测试连接：显示更接近用户操作的建议，而不是原样打印 `error_kind: message`
- 单篇 embed：失败时明确说明“现有索引未被改坏”
- 整库初始化：
  - 正常部分失败：继续沿用 summary + example preview
  - provider 级失败提前中止：显示“初始化已中止 + 未尝试 N 篇 + 原因 + 可重试建议”

#### 6.23.6 测试覆盖

本刀新增 / 更新的关键测试：

- `services/ai/openai.rs`
  - 429 分类测试改为校验 message + retry seconds 都保留
- `services/ai/embed_service.rs`
  - `missing_file_surfaces_error`
  - `provider_rate_limit_is_classified_and_store_unchanged`
- `services/ai/init_service.rs`
  - `embed_vault_aborts_early_on_provider_failures`
  - success / skip 汇总测试同步校验 `aborted_early == false`

代码层验证基线：

- `cargo test --manifest-path src-tauri/Cargo.toml` → `157 passed`
- `pnpm check` → `0 errors, 0 warnings`

---

### 6.24 AI 辅助·会话数据层（P3-D2b.1）

D2a 把 embedding 索引落成了 RAG 的底座；D2b 是上面跑的对话面板。D2b 同样切细：

| 切片 | 范畴 | 落点 |
| --- | --- | --- |
| **D2b.1** ← 本刀 | 会话持久化：`ChatStore` + 5 条 IPC + 前端 wrappers，无 UI、无 provider 调用 | 后端 + ipc |
| D2b.2 | `AiProvider::chat(ChatRequest) -> Stream<ChatDelta>` + OpenAI SSE 实现 + Settings 联通测试扩展 | 后端 |
| D2b.3 | `Panel.svelte` 改 Tab 布局（Links + AI Chat），接入非流式 ChatPanel v1 | 前端 |
| D2b.4 | 流式响应 + Tauri `emit_all` + 中断按钮 + history 截断 | 前端 + 后端 |
| D2b.5 | RAG 上下文注入 + `[[note-title]]` 引用渲染 + 新建会话 modal | 前端 + 后端 |
| D2b.6 | 弹出独立窗口（`variant: 'docked' \| 'standalone'`）+ AI 关闭时自动关闭 | 前端 |

D2b.1 的本职只有一件事：**在 Provider 调用进来之前，先把会话的落盘/读取/列举语义定死**。这样 D2b.2+ 的流式对话、RAG、脱离窗口都有一个稳定的 source-of-truth 可以写入／读出，而不用边做边改 schema。

#### 6.24.1 落盘格式 · 为什么用 JSONL 而不是 SQLite

候选三条：

1. 另开一个 `.mynotes/ai/chats.sqlite`（和 `embeddings.sqlite` 同父目录）
2. `.mynotes/ai/chats/<session-id>.jsonl` · 每会话一个 append-only 文件
3. 把会话混进现有 `index.sqlite`

最终选 **2**，理由：

- **Append-only 自带崩溃隔离**：对话生成是长跑的流式过程，中断／崩溃可能发生在任意 token 之后。一个 `O_APPEND + sync_data` 把"已落地的上文"与"正在写的那一 token"物理分开——最多丢一条 message，不会丢整段历史。SQLite 要拿到等价保证得强 WAL + 单连接，复杂度高。
- **人类可审计**：AI 侧隐私顾虑远大于索引。用户把 `.jsonl` 拖进文本编辑器就能看到自己发了什么、模型回了什么，不需要 `sqlite3 CLI`。
- **对齐 D2a 的 reset 故事**：`.mynotes/ai/` 下所有 AI 相关状态（embedding / chat / 未来的 usage ledger）都是"可一键删除 + 不影响主索引"。`chats/` 做成目录也让用户可以选择性删掉一个 session。
- **不 block provider 切换**：换 provider / 换 model 不需要 schema migration，只要在新的 meta 里记一下即可。

付出的代价：`list` 要扫每个文件统计 message 数；但 N ≤ 几百量级、每个文件 ≤ 几百 KB，逐行扫仍然 sub-millisecond，不值得为此上数据库。

#### 6.24.2 jsonl schema

每行一条 event，用 `#[serde(tag = "type", rename_all = "snake_case")]` 区分两种形状；首行必须是 `meta`，后续一律 `message`：

```jsonc
{"type":"meta","v":1,"session_id":"chat-20260421T163715-a3f2e814",
 "title":"RAG 选型","created_at":1745256435,
 "related_note":"notes/rag.md"}
{"type":"message","v":1,"id":"msg-1b9f2c80","role":"user",
 "content":"帮我对比一下几种 chunk 大小","created_at":1745256440}
{"type":"message","v":1,"id":"msg-44aa013e","role":"assistant",
 "content":"...","created_at":1745256443}
```

几个关键决策：

- **`v: 1`** 放每行（不是文件头一个），读的时候在每行都能验，方便以后做 schema 升级而不是"整个文件一锤定音"。当前 `ChatStore::load` 在遇到未知 `v` 的 meta 时直接报 `unsupported schema` — 不降级、不猜。
- **`session_id` 形如 `chat-YYYYMMDDTHHmmss-<8hex>`**：前 14 字符是 UTC 时间戳便于肉眼排序；后缀是 `sha256(nanos + pid + seq)` 前 8 hex，加 `AtomicU64` 的 per-process `seq` 兜底——同毫秒 / 同纳秒连续 `create` 也不会碰撞。不引入 `uuid` / `rand` 新依赖（`sha2` 已经为 secrets 引入）。
- **`related_note`** 可空：scratch 会话是合法的。为空就是空，不写 `""`。
- **`message.id`** 独立生成（不从 session_id 派生），D2b.4 流式里一条 message 可能被多次 emit delta，这个 id 用来让前端合并同 id 的 chunk。

#### 6.24.3 `ChatStore` API 与错误形态

```rust
// services/ai/chat_store.rs
pub struct ChatStore { root: PathBuf }

impl ChatStore {
    pub fn new(vault: &Path) -> Self
    pub fn list(&self) -> AppResult<Vec<ChatSessionSummary>>
    pub fn create(&self, title: &str, related_note: Option<String>)
        -> AppResult<ChatSessionSummary>
    pub fn load(&self, session_id: &str) -> AppResult<ChatSessionFull>
    pub fn append(&self, session_id: &str, role: ChatRole, content: &str)
        -> AppResult<ChatMessage>
    pub fn delete(&self, session_id: &str) -> AppResult<bool>  // idempotent
}
```

- **Session id 白名单**：`[A-Za-z0-9_-]`，长度 ≤ 64；前端传来的 id 先过 `validate_session_id` 才 join，堵死 `"../escape"` 类 payload。题外话：即便前端是可信代码路径，路径遍历防御是 vault 边界的最后一道——同样的规则我们在 file / attachment 命令层已经维持了。
- **`create` 用 `OpenOptions::create_new(true)`**：id 冲突罕见（秒 + 8 hex），真冲突宁可报 IO error 也不能覆盖已有会话。
- **`append` 用 `OpenOptions::append(true)` + `sync_data`**：不 `sync_all`，metadata 不关键——我们要的是"写进磁盘的 message 能在重启后读到"，而不是精确的 mtime。
- **`load` 严格**：多条 meta / message 先于 meta / 未知 schema / 行解析失败，一律返回 `AppError::Other("corrupt chat session ... line N: ...")`——UI 拿到之后显示 "reset session" 而不是半截对话。
- **`list` 宽松**：单个文件 corrupt 时 `tracing::warn!` 之后跳过，不让一个坏文件把整个侧栏 blank。用 `file_type` 先过滤目录 / 只接 `*.jsonl`，允许用户在目录里塞 README。
- **`delete` 幂等**：`ErrorKind::NotFound` 返 `Ok(false)`，别的 IO 错照常上抛。

#### 6.24.4 `ChatSessionSummary` 的 last_message_at 怎么来的

sidebar 要显示 "最近 23:17 有一条消息"，就得扫全文；但 load 和 list 的共同代价是"读完文件 N 行"。两条路复用 `BufReader::lines()`，`load` 保留所有 message bodies，`list` 只记 count + 最新 `created_at`，不复制内容。这是一次全扫但没有第二次内存拷贝——N 条 session × 每文件 K 行 = O(N·K)，对 "几十会话 × 几百消息" 级别完全够用。D2b.5 如果要展示"最后一条消息的前 40 字预览"，只需在 `summary_from_path` 里多捕一个 `last_content_head`，不用重走 I/O。

#### 6.24.5 IPC 表面（5 条）

| 命令 | 入参 | 出参 | 备注 |
| --- | --- | --- | --- |
| `ai_chat_session_list` | — | `Vec<ChatSessionSummary>` | 空 vault → `[]` |
| `ai_chat_session_create` | `title`, `related_note?` | `ChatSessionSummary` | `title` 空白 → "Untitled"；`related_note` 传 `".."` → `PathEscape` |
| `ai_chat_session_load` | `session_id` | `ChatSessionFull` | 不存在 / corrupt → `Other` |
| `ai_chat_session_append` | `session_id`, `role`, `content` | `ChatMessage` | 返回刚写下的一行（带 id / created_at） |
| `ai_chat_session_delete` | `session_id` | `bool` | 幂等；true = 实际删除；false = 原本就没 |

为什么 `create` 不让前端传 id？因为 tauri 命令通过 JSON 走字符串，任何"前端生成 + 后端信任"的路径都给了一条直接落地到 `root/<attacker-controlled>.jsonl` 的缝。后端生成 id 之后，即便前端回传 id 做 append，`validate_session_id` 也会把任何带 `/` `..` `.` 的字符串打回，文件永远只能落在 `root/` 下。

`ChatStore` 被做成"按需构造"的薄包装——每条命令在入口处解包 `active_vault` → `ChatStore::new(&vault)`。**刻意不挂进 `AppState`**：embedding store 挂 AppState 是因为它要持续持有 SQLite connection pool；chat store 里只有一个 `PathBuf`，挂了反而要在 vault 切换时另外清状态，邀请 stale-vault bug。

#### 6.24.6 前端 IPC 层（`src/lib/ipc/ai.ts`）

对应 5 个 wrapper + 5 个 type（`ChatRole` / `ChatMeta` / `ChatMessage` / `ChatSessionSummary` / `ChatSessionFull`）。关键 TypeScript 约定：

- `ChatRole = 'user' | 'assistant' | 'system'`（小写，和 serde `rename_all = "lowercase"` 对齐）
- `ChatSessionSummary.last_message_at?: number`；`related_note?: string`——Rust 侧 `skip_serializing_if = "Option::is_none"`，所以走线里字段缺失 = "无"
- 所有时间戳是 **Unix seconds (number)**；前端渲染时再转 locale string

**无 UI 改动**：D2b.3 才真正把这些 wrapper 接到 `Panel.svelte` / `ChatPanel.svelte`。让 D2b.1 只动后端 + 一层 type contract，D2b.3 就有一个稳定的 "调什么函数" 目录可以盯着写组件，不需要同一刀再来回改类型。

#### 6.24.7 刻意不做

- **会话重命名 IPC**（`ai_chat_session_rename`）：留给 D2b.3，那时候有 UI 才知道要不要支持就地改标题。技术上就是 append 一个 `meta_update` line 或 rewrite 首行，二选一不影响当前数据。
- **会话搜索 / 全文索引**：完全不在 D2 范围内。
- **Message 修改 / 删除**：对 append-only 文件来说"删一条" = 复制全文丢掉一行再重写，破坏崩溃隔离；产品需求也没到那一步。
- **版本迁移器**：目前只有 `v: 1`，等真有 v2 需求时再写一个一次性迁移脚本，不在 storage 层预置框架。
- **读写锁 / 多进程并发**：桌面单进程单用户，纯 `append` + `sync_data` 已经够；真要上多窗口同编辑一个会话，那是 D2b.6 弹窗之后的事。

#### 6.24.8 测试覆盖

`services/ai/chat_store.rs` 自带 10 条单测，覆盖四个维度：

- **roundtrip**：create → load 看到 meta + 空 messages / append 3 条 → load 顺序 + 角色正确 / 空白 title 回落 "Untitled"
- **多会话排序 + 聚合**：list 按 `created_at` desc / `message_count` + `last_message_at` 聚合对得上（sleep 1.1 s 跨过 1 秒边界避免同秒回放）
- **异常路径**：load 不存在 → `session not found` / corrupt jsonl → `corrupt chat session ... line N` / 非法 session_id（`""` / `"../escape"` / `"a/b"` / `"has space"` / `"has.dot"` / 65 字符）全部在 `session_path` 被拒
- **宽松 list**：空 root → `[]` / `chats/` 里混 `README.md` / `stray.txt` → 只回会话

加上 `cargo build --lib` 通过 Tauri `#[tauri::command]` 宏展开——**167/167** lib 测全绿，`pnpm check` 继续 `0 errors, 0 warnings`。

---

### 6.25 AI 辅助·Provider Chat 接口（P3-D2b.2）

D2b.1 把持久化锁死之后，D2b.2 负责把 **provider → SSE → 测连接按钮** 拉通——**零 Panel UI 改动**，所有下游刀（D2b.3 非流式 v1、D2b.4 流式 IPC）都能站在稳定的 `AiProvider::chat_stream` trait 上实现。

#### 6.25.1 `AiProvider::chat_stream` trait 方法

```rust
pub struct ChatRequest { pub model: String, pub messages: Vec<ChatTurn>,
                         pub temperature: Option<f32>, pub max_tokens: Option<u32> }
pub struct ChatDelta   { pub content: String, pub finish_reason: Option<String>,
                         pub input_tokens: Option<u32>, pub output_tokens: Option<u32> }
pub type  ChatStream   = Pin<Box<dyn Stream<Item = Result<ChatDelta, ProviderError>> + Send>>;
async fn chat_stream(&self, _req: ChatRequest) -> Result<ChatStream, ProviderError>
```

- **`ChatStream` 用动态 trait 对象**：最终消费者是 Tauri `#[tauri::command]`（必须 object-safe），也是前端 `emit_all`（D2b.4 要 Send）；`Pin<Box<dyn Stream<…> + Send>>` 是最小交集。加 `Box` 不影响热路径——一个 chat session 生命里 stream 只构造一次。
- **trait 级带默认实现**（返回 `ProviderError::InvalidRequest("chat is not supported by this provider")`）：`services/ai/{embed_service,init_service}.rs` 单测里的 `FailProvider` 是 embed-only 替身，没必要为一个它永不会走到的方法写 no-op；加默认实现就自动免除。只有 `OpenAiProvider` 和 `MockProvider` 真正 override。
- **`finish_reason` / `usage` 走同一 `ChatDelta` 结构**：OpenAI 在 SSE 末尾先发一个 `finish_reason: "stop"` chunk，再发一个 `usage` chunk；我们把两者各自映射成一个 `ChatDelta`，`collect_chat_stream` helper 会把 content 累加、finish_reason / token 字段以"最新非 None 胜"合并。避免为 trailer 单独搞一个 enum variant——后续 D2b.4 流式 IPC 也只需一个 event type。

#### 6.25.2 SSE 解析策略（`OpenAiProvider`）

**接收面**：`POST {base_url}/chat/completions` with `stream: true` + `stream_options.include_usage: true`（OpenAI 会在最后吐一个 usage-only chunk；Ollama / LM Studio / vLLM 忽略此字段，零副作用）。`Accept: text/event-stream` 头显式声明；握手错误走 `classify_http_error` 既有的 401/403/429/4xx/5xx 分类。

**解析拆成两个纯函数** 便于单测：

```rust
pub(crate) fn parse_sse_data(payload: &str) -> Result<Option<ChatDelta>, ProviderError>;
pub(crate) fn find_event_end(buf: &[u8])  -> Option<(usize, usize)>;
```

- `parse_sse_data`：`[DONE]` → `Ok(None)`；JSON 解析失败归 `ProviderError::Other` 不是 `InvalidRequest`（后者语义是 caller 错了，实际是 server 回了坏格式）。
- `find_event_end`：**同时吃 `\n\n` 和 `\r\n\r\n`**；返回 delimiter 长度而非固定 2，调用者 drain 时用 `buf.drain(..delim_len)` 就不会把半截 `\r` 漏下来。OpenAI 实际只用 `\n\n`，但 OpenRouter / 某些代理会插 `\r\n\r\n`，通用更稳。

**取消语义**：主 `chat_stream` 拿到握手成功的 response → 开 `mpsc::channel(16)` → `tokio::spawn` 后台 task（读 bytes、累 buf、切 event、push delta 到 tx）→ 返回 `futures_util::stream::unfold(rx)` 包 rx 的 `Stream`。前端 drop 流时 rx 先 drop、tx 关闭、task 下次 `send().await` 拿到 `SendError` 返回，没有 leak。**不引入 tokio_stream crate**——`unfold` 一行 wrapper 就够了。

为什么不用 `futures_util::stream::unfold` 直接把 `bytes_stream` + `buf` 状态一起 unfold？因为 state 需要可变，且解析一次 chunk 可能产生多条 delta；在 unfold 里塞一个"事件队列 buffer"反而更复杂。spawn + channel 是两边职责更清的方案。

#### 6.25.3 `MockProvider` chat harness

为让 chat 流程有一条完全离线的验证路径，`MockProvider` 加两个 `Arc<Mutex<Option<…>>>` 状态：

- `chat_script: Option<Vec<String>>`：`set_chat_script(tokens)` 预装 tokens 数组；`chat_stream` 消费一次后清零。每个 token 映射成一个 `ChatDelta`，最后一 delta 带 `finish_reason: "stop"` + zero tokens usage。
- `chat_error: Option<ProviderError>`：`set_chat_error(err)` 预装下一次 `chat_stream` 要 surface 的错误（consumes on first call）。测试用来断言 rate-limit / auth 分类链路而不用真去挑起 HTTP 429。

未配置脚本时，默认 **三 chunk echo 最后一条 user turn**（`"echo: "` + 内容 + 空结尾带 finish_reason）——既能让"聚合测试"看到多 delta 行为，又不至于像真 LLM 那样 chatty。

#### 6.25.4 `chat_model` 为什么独立字段

`AiProviderConfig` 新增 `#[serde(default)] pub chat_model: String`——`embed_model` 和 `chat_model` **不能复用同一字段**：

- 典型部署是一个小/便宜 embedder（`text-embedding-3-small` / `nomic-embed-text`）+ 一个大 chat model（`gpt-4o-mini` / `llama3.1`）；强行合并会被用户"那我换个大 chat 不就把 embedding 跑贵了？"反问。
- 空串 = chat 停用（embedding 仍可用），让"embeddings-only"场景（离线 RAG、数据集打 vector 但不做 QA）有合法配置状态。
- `#[serde(default)]` 让老的 `app-config.json`（D2a 时期写的）在 D2b.2 启动后直接 forward-compatible，不需要 migration。
- 相应地 `runtime.rs` 新增一组 chat-flavoured helper（`build_configured_chat_provider` / `build_chat_provider_from_config`），validation 盯的是 `chat_model` 而不是 `embed_model`。**D2b.2 暂挂 `#[allow(dead_code)]`**：首个消费者是 D2b.4 的 `ai_chat_stream` IPC，此时把 helper 放到位是为了那一刀零回头路。

#### 6.25.5 Settings「测连接」拆两档

```
┌─ AI Provider ─────────────────────────┐
│ Base URL   · https://api.openai.com/v1│
│ Embed model · text-embedding-3-small   │
│ Chat model  · gpt-4o-mini  (空=停用)   │
│ API key     · (keychain)               │
│                                       │
│ [测试 Embedding] [测试聊天] [保存] [清除] │
│ ✓ Embedding 连接成功 · 维度 1536 · 6 tokens │
│ ✓ Chat 连接成功 · 回复 "OK" · 3 out tokens │
└───────────────────────────────────────┘
```

- **两个独立 banner**：用户能一眼看出是"embedding 那边炸了"还是"chat 那边炸了"，报错定位只跨一层。
- **「测试聊天」按钮在 Chat model 为空时自动禁用**（hover 提示"请先填写 Chat model"）；Save / Clear 在任一测试跑时都禁用——避免测到一半把配置改了看不懂谁测谁。
- Chat 测试命令是专用的 `ai_provider_test_chat_connection`（异步），硬编 `max_tokens: Some(8)` + `temperature: Some(0.0)` 跑 `"Say OK."`——最小 token 开销，最稳定 reply。返回 `ChatProviderTestResult { ok, reply? (≤200 chars), input_tokens?, output_tokens?, error_kind?, error_message?, retry_after_secs? }`。20 秒 timeout（embedding test 是 10 s；chat 首 token 通常多一个冷启动 hop）。

#### 6.25.6 `ChatRole` 统一到 provider 一份

D2b.1 里 `services/ai/chat_store.rs` 和 D2b.2 的 `services/ai/provider.rs` 各自定义过一份 `ChatRole { User / Assistant / System }`（两份都 `rename_all = "lowercase"`）。发现重复立刻收敛——`chat_store::ChatRole` 改成 `pub use super::provider::ChatRole`，消除类型分裂：

- 同一个 JSONL 文件里存的 role 字符串和同一个 chat HTTP body 里 role 字符串字面等价，serde 输出 `"user"` / `"assistant"` / `"system"` 不变。
- 未来 D2b.3 把 `ChatMessage.role` 转成 `ChatTurn.role` 送 provider 时无需任何映射 / `From`。
- 线上对前端 TS 类型零影响（wire format 不变）。

#### 6.25.7 不做事项

- **Function calling / tool use**：`ChatTurn.content` 只支持纯文本；D2b 范围不扩。未来如接 tool，要么加新 enum 分支，要么迁 trait 到 v2，不 monkey-patch 现有结构。
- **Anthropic SSE**：`parse_sse_data` 只认 OpenAI 的 chunk 形状（`choices[].delta.content` + `usage`）。Anthropic 的 SSE 用 `event: content_block_delta` + 分段 type，要走新 provider 实现——不是 parser 里加分支的事。
- **中断 IPC**：这一刀只做"能流起来"；cancel channel + 前端"停止"按钮在 D2b.4。
- **限流 countdown**：`retry_after_secs` 字段已经贯穿到 `ChatProviderTestResult`，但 Settings banner 现阶段只打文本；带 countdown 的 UI 留到 D2b.3 能看到真实 rate-limit 场景再打磨。

#### 6.25.8 测试覆盖

- `services/ai/provider.rs::tests`（+4）：mock script 播放聚合成整段 / 无 script 时 echo 最后一条 user turn / `set_chat_error` 精确透传 `RateLimit { retry_after_secs, message }` / 空 messages → `InvalidRequest`。
- `services/ai/openai.rs::tests`（+10）：`chat_completions_url` 拼接；`parse_sse_data` 覆盖 content delta / finish_reason chunk / usage trailer / `[DONE]` / 非 JSON 五分支；`find_event_end` 覆盖 `\n\n` / `\r\n\r\n` / 半截 buf 三分支；`chat_stream` 空 messages 不发起 HTTP 直接 `InvalidRequest`。
- 总计 **181 passed; 0 failed**（D2b.1 的 167 + 14）；`cargo clippy` 在涉及六文件（`openai.rs` / `provider.rs` / `chat_store.rs` / `runtime.rs` / `services/config.rs` / `commands/ai.rs`）零警告；`pnpm check` 继续 0/0。

### 6.26 AI 辅助·Panel Tab 化 + 非流式 ChatPanel v1（P3-D2b.3）

D2b.1 把会话持久化锁住、D2b.2 把 provider chat 拉通之后，D2b.3 把两层接到右栏 UI——**先非流式**。这一刀的设计目标不是"做一个好用的 chat panel"，而是"**用最小改动跑通完整的发→存→渲染闭环**"；streaming / 中断 / RAG / 弹窗留给 4/5/6。保持每刀可单独验收、每刀可单独回滚。

#### 6.26.1 为什么用"非流式 IPC"而不是直接上 D2b.4

D2b.4 的 `ai_chat_stream` 要带 `emit_all` + 取消令牌 + 前端 event listener——三样一起上一刀 PR 调试复杂度指数上升。先做非流式 `ai_chat_send` 的收益：

- **整条 session 生命周期可一趟 IPC 验**：发送 → persistence → provider → persistence → reload 全跑一次 round-trip，能 catch 的问题（会话文件 append 顺序、provider 配置空字段、失败 classification）在 v1 暴露出来比藏到 streaming 代码里好。
- **UI 层的关键决策（tab 架构 / 气泡布局 / markdown 渲染 / 失败 banner）与传输层解耦**：D2b.4 只改 send 路径（把 `collect_chat_stream` 换成事件监听），transcript 渲染、lastResolvedSessionId 这套 bookkeeping 零动。
- **Stream 取消语义比非流式复杂一截**：cancel 的时机（provider 已 yield 一半 → 落盘哪些？前端 UI 该停在哪？）需要 streaming 到位再谈；强行在 v1 加 cancel 会让 D2b.3 背一个它本来不该背的接口面。

代价：长回复期间用户只看到"三点动画"而不是逐 token 流出——这是明确写进已知缺口的 trade-off，不是 bug。

#### 6.26.2 `ai_chat_send` 后端流程

```
IPC 入口 (ai_chat_send) 的顺序固化：

  1) chat_store(&state)?                 ↙ no vault  → ChatSendFailure { kind=other, user_message_persisted=false }
  2) store.load(session_id)?             ↙ invalid id → ChatSendFailure { kind=invalid_request, user_message_persisted=false }
  3) build_configured_chat_provider      ↙ no chat_model → ChatSendFailure { kind=invalid_request, user_message_persisted=false }
  4) store.append(session_id, User, …)   ← 故意放在 provider 之前
  5) provider.chat_stream + collect      ↙ provider err → ChatSendFailure { kind=<classify>, user_message_persisted=true }
  6) if reply.trim().is_empty() →         ↙                ChatSendFailure { kind=other, user_message_persisted=true }
  7) store.append(session_id, Assistant)  ↙ io err       → ChatSendFailure { kind=other, user_message_persisted=true }
  8) Ok(ChatSendResult { ok: true, assistant })
```

**关键不变式**：user turn **永远先于** provider 落盘。理由是人类 UX 语义：

- 发送按钮点完 → 消息应该"看得见"在历史里。失败时用户第一反应是"我的消息发出去了吗？"；保留消息让重试只是点一下按钮，不需要 re-type。
- 重试不重复：同一条 user 消息可能被发两次；但 append-only JSONL 里出现重复的 user 文本是自然的"我重试了"痕迹，**不是**数据不一致——产品语义也符合 ChatGPT / Claude.ai。
- 失败路径的 `user_message_persisted` 字段告诉前端这次失败是否在步骤 4 之前。UI 因此能区分"没发出去 → 放回输入框让改"和"发出去了但 provider 出错 → 保留气泡，加个重试按钮"（D2b.4）。

**Empty-reply 的单独分支**：某些 provider（tool-call / 被 stop 触发 / Ollama 偶发）会返回空 `content`。把它标记成 `failure { kind: "other", user_message_persisted: true }` 而不是持久化一条空 assistant 消息——空气泡在 transcript 里语义模糊（"AI 有话说吗？还是出错了？"），直接呈现为一条"生成失败"banner 更清晰。

#### 6.26.3 `ChatSendResult` 为何是 struct 不是 `Result<T, E>`

同 D2a.6 的 `EmbedNoteResult` 家族：

- Tauri 的 `Result<T, AppError>` reject 到前端是一个字符串，失败细节（`kind` / `retry_after_secs` / `user_message_persisted`）要么硬编码到 message 要么 parse string——两条路都烂。
- 把"成功 + 失败"并成一个 struct 后，前端 `aiChatSend()` 的调用路径永远 resolve，TypeScript 类型系统能看到两种状态并强制处理；渲染失败 banner 不需要 try/catch 分支。

#### 6.26.4 Panel Tab 架构

`Panel.svelte` 从"单列 section 堆"改成"Tab header + 单 tab 内容"：

```
┌───── panel ─────┐
│ [ 笔记关系 ][ AI 对话 ]  ← tab bar
│                         │
│  (Links sections)        │  ← activeTab === 'links'
│  or                      │
│  <ChatPanel />           │  ← activeTab === 'chat' && aiEnabled
└──────────────────────────┘
```

设计决策：

1. **AI 对话 tab 只在 `aiEnabled === true` 时出现**。关 AI 的用户看不到一个"点了也没反应"的 tab 头，视觉干扰降到零。
2. **`aiEnabled` 在 chat tab 激活时被切到 false → `$effect` 自动把 activeTab 切回 Links**——避免 tab 头消失但内容卡在 chat。
3. **Tab 状态 panel-local**（一个 `$state<Tab>` 在 Panel.svelte 顶层）。右栏太窄、tab 只有两个，值得不得放 global app state；vault 切换时 Panel 重新 mount → 默认回 Links，符合用户对"新环境从默认开始"的直觉。
4. **Links 内容结构零动**：反向链接 / 链出 / 未解析 / 相关笔记 / 项目笔记 全部留在原地。D2b.3 本刀只是把它们包到 `{#if activeTab === 'links'}` 里。
5. **`.panel` 改为 flex column + `height: 100%`** 以便 chat 子组件能做内部 flex（transcript 占满中间空间，composer 贴底）。Links tab 继续用 `section + section` margin 布局，靠 `.panel > :not(.panel-header) { overflow-y: auto }` 的通配让滚动容器天然落在单个 tab 的内容上。

#### 6.26.5 ChatPanel 乐观 UI + 持久化 reload 的协调

首次发送（空 session 自动创建）和普通发送都走同一套状态机：

```
optimistic bubble (user)        ─┐
                                 ├─ 视觉上永不消失
real user msg (from reload)      ─┘
                                  │
typing indicator (sending=true)   │
                                  ↓
assistant bubble (from reload)
```

关键 bookkeeping：一个**非响应式** `let lastResolvedSessionId: string | null = null`。为什么不用 `$state`：

- 这是 UI 层的"磁盘与内存状态是否同步"标记，**不应该**唤醒任何 `$effect`。用 `$state` 会让赋值 tracker 的动作本身触发 effect，形成自循环。
- 场景：`send()` 里自动 create session → `activeSession = { meta, messages: [] }` + `lastResolvedSessionId = newId` + `activeSessionId = newId`。Svelte 把 `activeSessionId` 变化推进 `$effect` 时，effect 里读到 `id === lastResolvedSessionId`，直接 early return，不再从磁盘 reload 空 transcript 覆盖掉刚刚乐观 push 的 user bubble。
- 加载成功后 `loadActiveSession` 再把 tracker 推到最新 id。若 reload 失败 `tracker = null`——下次 `activeSessionId` 变化时一定会 reload。

另一个细节：乐观 bubble 的 `id = "optimistic-${Date.now()}"`，和真正从后端来的 nanoid 格式不重叠；reload 后 keyed `{#each ... (m.id)}` 会干净替换为真实 id 的 bubble，Svelte 不复用同一个 DOM 节点（避免 in-place id 变形导致 focus / 动画状态错乱）。

#### 6.26.6 Composer 的键位约定

Enter 发送 / Shift+Enter 换行 / Cmd|Ctrl+Enter 强制发送——三档行为覆盖了主流 IM 习惯（iMessage / WhatsApp / Slack / ChatGPT 都是 Enter 发送默认）。刻意排除：

- **Esc 取消**：v1 没有 in-flight cancel；Esc 什么都不做避免用户猜错语义。D2b.4 打通 cancel 后 Esc 就可以绑了。
- **输入历史上下键**：shell 里正常，但 chat UI 里不主流；实现要维护 transcript 光标 + 当前输入的区分，ROI 低。
- **文件粘贴 / 拖入**：v1 只认 text；多模态等 D2b 之外。

#### 6.26.7 最小 markdown 渲染

`renderMarkdown(src)` 是一个 ~60 行的纯函数。支持：fenced code block（带可选语言 `data-lang`）/ inline code / `**bold**` / `*italic*` / `_italic_` / 自动链接 `https?://…` / 段落（双换行分段）+ `<br>` 单换行。

安全边界：`renderMarkdown` **必须** 先对整段 src 做 HTML-escape，再应用 regex 生成 `<strong>` / `<em>` / `<a>`——否则 provider 的任意输出就能通过 `@html` 注 script。代码注释里显式标注"This is the security boundary"。

刻意不做：

- **headings（`# foo`）/ lists / tables / blockquote**：chat 回答里这些占比低，v1 先不做，免得引入一个 parser 的 1k 行；D2b.5 / D3 真碰到长 MOC draft 再上 `marked` 或专门 renderer。
- **wiki-link `[[Note]]`**：D2b.5 的核心功能之一（识别 + 跳转 + 高亮 chunk offsets），v1 按普通文本显示。
- **code 高亮**：只 escape + monospace，不接 prism / shiki。

在 tab 切换和 reload 两个路径上多次执行 `renderMarkdown` 没做 memoization——测试下来每条 message 的渲染在 P50 <1ms 级别，缓存层 ROI 不值得。

#### 6.26.8 不做事项

- **流式响应 / 取消按钮** → D2b.4。
- **RAG context 注入** → D2b.5；v1 的 `ai_chat_send` 发给 provider 的 `ChatRequest.messages` 只含用户持久化过的 turns，没额外 system prompt 也没 chunks。
- **独立窗口 / 多实例同步** → D2b.6。
- **会话重命名 / 搜索** → 延续 D2b.1 的决定，不做。
- **会话级并发锁**：当前 `ai_chat_send` 对同 session 并发调用没有 mutex，双请求可能读到 stale history。前端 `sending` 状态已经禁 send 按钮，vault 级并发是另一回事（用户同时跑脚本 ping IPC），v1 不覆盖；真要补就在 `ChatStore` 里加 `Mutex<HashMap<SessionId, Mutex<()>>>`。

#### 6.26.9 测试覆盖

- `cargo test --manifest-path src-tauri/Cargo.toml` → **181 passed; 0 failed**（沿用 D2b.2 的总数，本刀无新单测）。
- 本刀新增路径主要在：
  - `ai_chat_send` 命令：依赖 Tauri `State<AppState>` + 异步 provider + 真实 `ChatStore`。D2b.1 同样的约束下我们把单测放在 storage（`chat_store::tests` 10 条）+ provider（`openai`/`mock` 14 条）+ runtime（`build_chat_provider_from_config`）三层，**不**对 command 函数本身编写假 Tauri state 测试——这些测试长、脆、收益低，同样的模式在 `ai_embed_note` / `ai_embed_vault_run` 里已经延续。
  - `renderMarkdown`：Svelte `<script>` 里的纯函数，现阶段没 vitest 基础设施，不想为单元测试这一点功能引进 vitest 依赖。后续若接流式或 wiki-link 复杂度上来，再把它抽到 `src/lib/chat/markdown.ts` 做独立测试。
- `pnpm check` → **0 errors / 0 warnings**（修掉 `<nav role="tablist">` 的 a11y 提示 → `<div>`）。
- 涉及文件 clippy 无新增 warning（`commands/ai.rs` / `services/ai/runtime.rs` / `services/ai/provider.rs` / `services/ai/openai.rs`）。

---

### 6.27 AI 辅助·流式 Chat IPC + 中断 + History 截断（P3-D2b.4）

#### 6.27.1 为什么现在切流式

- D2b.3 的非流式 `ai_chat_send` 对 300~1000 字的长回答体验很差——用户盯着"三点动画"2~10 秒，没有"它在想"的反馈；网络抖动时无法区分"卡住"与"真的慢"。
- 流式后用户可以边看边判断质量、边读边决定是否中断，把"发送成本感"摊平到整段回答里。
- 之所以留着 `ai_chat_send` 而不是原地改：它是 fallback / 测试路径，供 `ai_provider_test_chat_connection` 复用、供未来"非交互式自动对话"（例如 MOC AI draft）使用，IPC 契约保持稳定。

#### 6.27.2 `AppState::chat_streams` 注册表形状

```rust
pub chat_streams: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
```

- **外层 Arc**：spawn 的 async task 要 `clone()` 拿所有权，不能借 `State<AppState>`（后者需要 `'static`）。
- **Mutex**：insert / remove / get 都是短持有，竞争域极小；没必要上 `tokio::sync::Mutex` 支付异步开销。
- **value 是 `Arc<AtomicBool>`**：cancel flag 要在 task 与 cancel IPC 之间共享，`AtomicBool` 避免再加一层 mutex。
- **key 是 frontend-generated `stream_id`**：v1 frontend 用 `s-<timestampB36>-<rand8>`；后端拒绝空串 + 长度 > 128 + 重复（三档 pre-flight 失败都走 `Result` 而不走事件）。

#### 6.27.3 `ai_chat_stream_start` 两段式控制流

同步段（在命令函数体内运行，命令返回前必须完成）：

1. 校验 `stream_id` 非空且未重复。
2. `chat_store(&state)` → `store.load(session_id)` 读全量历史。
3. `runtime::build_configured_chat_provider` → 拿 `(provider, chat_model)`。
4. `store.append(User, content)` 持久化 user turn——拿到的 `ChatMessage` 直接塞进 `ChatStreamStartResult.user_message`，前端用它 swap 乐观气泡 id。
5. `truncate_history_to_budget` 按字符预算截断。
6. 插入 cancel flag 到 `chat_streams`。

异步段（`tauri::async_runtime::spawn`，可能在命令返回之后）：

1. `provider.chat_stream(req)` → 得 `ChatStream`。
2. 循环 `stream.next().await`：每次 poll 前先检查 cancel flag，避免多 emit 一条。
3. 每个 delta 追加到 `accumulated: String` + `emit("ai:chat-stream:delta", …)`。
4. 循环结束后判定三态：**取消** / **错误** / **正常**；正常则 `store.append(Assistant, trimmed)` 再 emit `done`；取消同路径但 `cancelled: true`；错误直接 emit `error`。
5. 无论哪一支，`cleanup()` 都从 `chat_streams` 里移除自己的 entry——避免内存泄漏 + 防止后续同 id 重复判 "in use"。

这条两段式流水线的关键不变式：**user turn 永远先落盘**，哪怕 provider 构造都失败了；assistant turn **只在实际有非空内容** 时落盘（空回复走 error 通道，不污染 transcript）。

#### 6.27.4 事件协议

三个 channel，都以 `ai:chat-stream:` 开头，便于 frontend 做命名过滤；每个事件**必须**带 `stream_id` 做路由（v1 只有一个 panel 在听，但 D2b.6 会出现主窗 + 独立窗同时监听的场景）：

| 事件名                    | 触发时机                                 | payload 字段                                         |
| ------------------------- | ---------------------------------------- | ---------------------------------------------------- |
| `ai:chat-stream:delta`    | 每收一个 provider chunk                  | `stream_id`, `content`, `finish_reason?`             |
| `ai:chat-stream:done`     | 流式正常结束 / 用户取消                  | `stream_id`, `assistant: ChatMessage`, `cancelled`   |
| `ai:chat-stream:error`    | provider 报错 / 空回复 / 持久化失败      | `stream_id`, `failure: ChatSendFailure`              |

`finish_reason` 透传 provider 的原字段（`stop` / `length` / `tool_calls` / …）——v1 frontend 只读它作为调试信息；将来如果要支持 tool-use，需要根据 `tool_calls` 切特殊 UI，届时再加 handler。

#### 6.27.5 `ai_chat_stream_cancel` 的语义

- **尽力而为**：cancel 返回 `true` 只代表"flag 已 set"，不保证 task 已停；task 下次 poll 时才检查 flag。
- **保留已收内容**：累积的 `accumulated` 在 cancel 后仍然经 `store.append(Assistant, …)` 落盘——用户能看到 AI 写了多少然后自行决定复用。
- **幂等**：同 id 重复 cancel 不会抛；`chat_streams` 里被移除后返回 `false`，前端以此区分"成功发起 cancel"与"已经结束了"。

设计的对照案例是 ChatGPT Web：点 stop 后已生成的部分会变黑留下；Claude.ai 也是同样行为。我们选保留而非丢弃，是因为：(a) 用户可能已经在读它；(b) 让下一轮对话可基于部分回答继续；(c) 丢弃需要额外在 store 层加 "rollback last append"，复杂度不对称。

#### 6.27.6 History 截断的预算算法

`truncate_history_to_budget(messages, max_chars)`：

```
1. 若首条是 System → 作为永不删除的 prefix 单独切出。
2. 从最新到最老遍历剩余 messages，累加 char 和。
3. 每轮判断：加上当前 msg 后的总 char 是否超 max_chars；超且已选 ≥1 条就 break。
4. 例外：若最终只选到一条且它本身就超预算，仍保留（用户刚打的，丢了调用没意义）。
5. 反转选集、拼上 system prefix、map 成 ChatTurn。
```

`CHARS_PER_TOKEN = 3.5` 是英文 ~4 / CJK ~2 / 代码 ~3 的折中。选字符而不是接入 tokenizer：v1 省掉 tiktoken / `tokenizers` crate 的依赖（后者编译一次要 30~60 秒，热 build 开发体验糟糕）；**略保守** 总比"差一点刚好塞爆"更安全。未来 D3 真的要精确 tokens（比如 MOC AI 的长 prompt 要 squeeze 到极限）再替换。

#### 6.27.7 前端流式 UI 协调

`ChatPanel.svelte` 的关键增量：

- **惰性 listen**：首次 send 时才调用 `await listen(…)` 三次，`onDestroy` 里 unlisten；长时间不点 AI tab 的用户不会付 event bus 监听的成本。
- **`activeStreamId` / `streamingContent`** 两个 `$state`：前者做事件路由（`payload.stream_id !== activeStreamId → 忽略`），后者是实时累加 buffer；渲染时 streamingContent 非空就 markdown 渲染 + 尾部闪烁光标，为空就沿用"三点动画"。
- **按钮切换**：sending 且 activeStreamId 非 null → 渲染"中断"按钮（红色）；否则渲染常规"发送"；`onStreamTerminal` 把 sending 和 activeStreamId 一并复位，避免按钮闪切。
- **统一 reload**：`done` 与 `error` 到达时都跑 `loadActiveSession + refreshSessions`——partial-on-cancel / no-assistant-on-error / 正常完成三种态都以后端持久文件为真相源，前端不手工 merge。

#### 6.27.8 为什么错误也走事件

替代方案是：pre-flight 错误走 `Result`，运行时错误也走 `Result`——但后者在流式场景里做不到，因为命令早已返回。再替代方案是"`Result` 返错 + `emit error`"两路都触发——前端要处理两个信号，增加状态机复杂度。

最终选择："命令返回前能知道的错误走 `Result`，之后的错误走事件"，边界清晰：前端看到 `ok: true` 就切到事件驱动；`ok: false` 就按银行界面处理（不需要注册 listener）。

#### 6.27.9 不做事项

- **RAG 注入** → D2b.5。
- **`[[note-title]]` 渲染 + 点击跳转** → D2b.5。
- **新建会话 modal（Related-note picker）** → D2b.5；当前继续用 `window.prompt`。
- **独立窗口 / 多窗同步** → D2b.6；目前 `emit` 是 Tauri 的全局广播，独立窗口一旦开启会收到相同事件流，届时按 `emit_to(label)` 或在 payload 里带 webview 身份区分。
- **会话级并发锁**：D2b.3 已说明；D2b.4 额外引入的是"同一会话同时跑两个 stream"的 case——注册表按 stream_id 隔离，但同 session 两条 stream 并发 append 目前仍靠前端 `sending` 单线；重新发消息功能真要落地前需要 `Mutex<HashMap<SessionId, Mutex<()>>>`。
- **精确 tokenizer**：字符预算足够了，真到 D3 再换。

#### 6.27.10 测试覆盖

- `cargo test --lib` → **185 passed; 0 failed**（从 D2b.3 的 181 → 185，新增 4 条 `truncate_history_to_budget` 单测：全量保留 / 超预算丢老 / 永远保 system / 单条巨长仍保留）。
- 流式 spawn task / 事件 emit / cancel flag 的端到端路径：依赖 Tauri runtime，沿用 D2b.1 / D2b.3 的取舍留给手测（`delivery_log.md` 顶部条目列了 5 步验证脚本）。
- `pnpm check` → **0 errors / 0 warnings**。
- `cargo clippy --lib --no-deps` → 无新增 warning（scanner.rs 的 type_complexity / explicit_auto_deref 是预存项）。

---

### 6.28 AI 辅助·RAG 注入 + `[[wiki-link]]` 渲染 + 新建会话 Modal（P3-D2b.5）

#### 6.28.1 为什么在 D2b.4 之上单独切一刀

D2b.4 把流式 + 中断 + history 截断打通以后，chat 已经"能聊了"——但它依然是**和笔记库脱节的**对话：user prompt 里没有笔记上下文（只有 history），assistant 回答里的 `[[wiki-link]]` 是死文本，新建会话靠 native `window.prompt`。D2b.5 的目标就是把"Chat ↔ 笔记库"的三条粘合线做完，让 Chat 真正成为"对着笔记库的对话"，而不是"能看到 history 的聊天框"。

刀口按**对 session JSONL 格式零改动**来切：RAG 只在 in-memory `full_messages` 层面 prepend system message，不落盘；citations 是前端内存态；wiki-link 解析放在点击时按需走 IPC；modal 纯 UI 替换。因此这一刀**不动 store schema**、**不动 provider trait**、**不动 event 协议**，风险被框在"命令 pre-flight + 前端渲染"这层。

#### 6.28.2 `services::ai::rag` 模块为什么拆成 async + sync 两半

RAG 的天然形状是"embed query → search store → 格式化结果"。但在 Rust + Tokio + Tauri spawn 任务的约束下，**如果把这三步写成一个 async 函数并在中途持有 `std::sync::MutexGuard`，编译就会挂**：`MutexGuard: !Send`，而 `tauri::async_runtime::spawn(...)` 要求 future `Send + 'static`——即便调用方当前没 spawn 这段代码，`ai_chat_stream_start` 马上就要 spawn，早挂还是晚挂的差别。

所以模块拆成两个函数：

- `async fn embed_query(query, provider, embed_model) -> Option<Vec<f32>>`：**不持任何锁**，只做一次 `provider.embed(...)`。调用方可以自由 `.await`。
- `fn search_and_format(query_vec, embed_model, &store: &EmbeddingStore, top_k) -> Option<RagContext>`：**同步**函数，要求调用方已经拿到 `EmbeddingStore` 的 `MutexGuard`。内部做 `store.search(...)` + 字符预算格式化。

调用方（`try_build_rag_context` in `commands/ai.rs`）按 "lock → clone `Arc<Mutex>` → drop guard → await embed → reacquire guard → search+format" 串起来，**每个 `.await` 前面锁都已释放**。代价是调用方要显式写锁 scope，收益是编译器强制你不能违反约束。

#### 6.28.3 `RagContext` / `RagCitation` 数据形状

```rust
pub struct RagContext {
    pub system_message: ChatMessage, // role=System, content="以下是...[1] ... [2] ..."
    pub citations: Vec<RagCitation>,
}
pub struct RagCitation {
    pub note_rel_path: String,
    pub chunk_index: u32,
    pub offset_start: u32,
    pub offset_end: u32,
    pub score: f32,
    pub preview: String, // ≤160 UTF-8 chars，单词边界安全
}
```

`system_message` 直接 unshift 到 `full_messages` 最前；`citations` 跟在 `ChatStreamStartResult` 里返回前端作为 chip 展示。预算：`DEFAULT_TOP_K = 4`（top-K = 4 是 embed dim 768 / 1024 向量库的经验甜点：再多命中开始重复同一笔记）、`MAX_CONTEXT_CHARS = 2400`（≈ 700 tok，留够 4k budget 里大头给 history + user 最新消息 + 回答 stream）。`preview` 软截 160 字符而不是整 chunk，是因为：

1. chunk 本身可能 800 字符，prepend 进 context 会把 budget 吃空；
2. 前端 hover chip 要显示，160 字符正好一屏 tooltip 看完不用滚动；
3. 单字节字符截断会破多字节中文——`truncate_chars` 按 `chars().take(n).collect::<String>()` 走，UTF-8 边界安全。

#### 6.28.4 RAG 的 best-effort 语义

整条链路都是"出错就返 `None`，chat 照走"：

- 未配置 embedding 模型 → `runtime::build_configured_provider` 返 `Err` → `try_build_rag_context` 返 `None`。
- `EmbeddingStore` 未初始化（`app.state.embeddings = None`）→ 返 `None`。
- `embed_query` 失败（provider 挂/网络挂/empty query）→ 返 `None`。
- `search_and_format` 命中为 0 → 返 `None`。

任何一支出事都不发 banner、不打 log（只在 debug build 里 `tracing::debug!`），让 chat 流继续走 raw path。这是**故意**的：RAG 是"质量加成"而不是"功能 gate"，让 embedding 模型挂掉阻断聊天会让产品体验比不上零 RAG 的基线。

#### 6.28.5 `ChatStreamStartResult` 添加 `citations` 的兼容考虑

```rust
#[derive(Default)]
pub struct ChatStreamStartResult {
    pub ok: bool,
    pub user_message: Option<ChatMessage>,
    pub citations: Vec<RagCitation>, // 新字段
    pub failure: Option<ChatSendFailure>,
}
```

- 加 `#[derive(Default)]` 让失败分支用 `ChatStreamStartResult { ok: false, failure: Some(..), ..Default::default() }` 收尾；成功分支显式填 `citations: Vec::new()` 或 `rag_ctx.citations`。
- 前端 `ChatStreamStartResult.citations?: RagCitation[]` 声明成 optional，让老路径（回退到 D2b.3 `aiChatSend`）不报类型错。
- citations 在 pre-flight 同步返回、不走事件，是因为：（a）streaming 期间不会二次 RAG；（b）前端第一时间能把 chip 显示给用户（edge 场景下 stream 需要 10s 才出第一个 token，用户至少看得到 AI 参考了谁）。

#### 6.28.6 `[[wiki-link]]` 渲染的两段决策

**第一段：渲染期不做 IPC 解析，只标记目标。** `renderMarkdown` 在流式 update 期间每个 token 到达都会跑；如果一次 render 里就直接 `await indexResolveWikiLink(target)`，200 token × N wiki-links 会把 IPC bus 打爆（Tauri IPC 有 serialize + channel overhead，每次 ~1ms）。所以渲染阶段只把 `[[target]]` / `[[target|label]]` 转成：

```html
<span class="chat-wiki-link" role="link" tabindex="0" data-wiki-target="<escaped-target>">
  [[label]]
</span>
```

**第二段：点击 / Enter-Space 时按需解析。** `transcriptEl` 上挂一次 `click` + `keydown` 事件委托：`event.target.closest('.chat-wiki-link')` 命中才走 `indexResolveWikiLink(target)`，分三分支：

- 命中 → `onOpenNote(rel_path)`；
- 未命中 → `uiError = \`未找到笔记：${target}\``（banner 3s 自动消失）；
- IPC 本身 throw → `uiError` 展示后端错误字符串。

用事件委托而不是 per-chip listener 的理由：流式回答里 wiki-link 每个 token 都可能产生新的 span，per-chip `onclick` 会在 `@html` 替换后失效（Svelte 不会自动 re-bind @html 下的事件），委托在稳定的父节点上就能一次性覆盖所有动态 span。

#### 6.28.7 `index_resolve_wiki_link` 的两段 precedence

镜像 `indexer::resolve_links` 的"先 title 后 stem"逻辑，但用**独立 IPC 命令 + 独立 SQL**，不跟内部批量解析器耦合：

```rust
#[tauri::command]
pub fn index_resolve_wiki_link(target: String, state: State<AppState>) -> AppResult<Option<NoteRef>> {
    // 1) exact title match
    if let Some(note) = query_first_note(&conn, "SELECT rel_path, title FROM notes WHERE title = ? LIMIT 1", &target)? {
        return Ok(Some(note));
    }
    // 2) filename stem match（SQLite 没原生 stem()，Rust 侧 Path::file_stem() 后过滤）
    // ...
}
```

两点取舍：

- **`LIMIT 1`** 对"同名歧义"的处理很粗暴——两个笔记标题都叫 `Deep Work` 时随机选一个。v1 笔记量小问题不大；V2 笔记多起来再接"歧义弹选择器"。
- **前端解析**而不是在 `ai_chat_stream_start` 里一次性把 assistant 回答扫一遍塞进 `resolved_links` 返前端，是因为 assistant 回答在流式中，"哪些 link 有效"要等流结束才知道；把渲染逻辑留在前端，反而最省事。

#### 6.28.8 新建会话 Modal 的 UX 决策

D2b.3 用 `window.prompt('标题：')` + `window.confirm('关联 ...?')` 拼起来，native 对话框在 macOS 上有两个问题：（1）阻塞 webview 主线程，期间 Tauri IPC event 不派发，流式回答若正好跑着会丢 token；（2）样式不可控，和 app 整体风格格格不入。

Modal 就是一个 panel 内部的 inline 浮层（`.ns-backdrop` `position: fixed` 铺满 viewport），**没用 portal 机制**（Svelte 5 没内置 portal，且 ChatPanel 的滚动容器不在 flex / overflow 里，`fixed` 直接就能覆盖整窗）。字段：

- **标题**（`maxlength=120`）：空 → 走 `defaultTitleForNow()` 降级；Enter 提交，Esc 取消。
- **关联当前笔记**（checkbox）：默认值 `!!filePath`——"打开一个笔记时新建会话，多半是想链它"，免得用户多点一下。空 vault / 没打开笔记时 checkbox 隐藏，显示灰色提示。
- **Busy 态**（`newSessionBusy`）：IPC 跑时所有字段禁用，按钮文案换"创建中…"，点 backdrop 不关。防双重创建。
- **错误态**：后端报错 → `newSessionError` banner 显示 modal 内，不关 modal（让用户改完再试）。

样式故意用 `.ns-*` 前缀自带一套而不蹭 `+page.svelte` 的全局 `.modal` / `.modal-backdrop`——ChatPanel 将来可能被独立窗（D2b.6）单独加载，全局样式依赖会把 standalone webview 弄坏。

#### 6.28.9 Citations 不持久化 & 其他"故意留坑"

- **citations 不落盘**：`citationsByAssistantId` 只存 Svelte state，切 session 或重开面板即丢。理由：embed_store 会随索引重建漂移，今天命中的 chunk index 明天可能已经不对；把 "AI 基于什么回答" 和 "store 当时的 snapshot" 硬绑会让"可追溯"反而变"误导"。D3（MOC AI draft）若要"引用直接贴片段"再考虑 snapshot 方案。
- **RAG 不按 `related_note` 过滤**：目前全库 top-K；related_note 只是 session metadata，不收窄 RAG 搜索范围。cosine 相关性够强时相关笔记的片段会自然排前——简单优先。
- **流式中 citations 不重算**：pre-flight 算一次就定；流里如果 assistant 要了别的笔记不会重跑 RAG。理由：重跑 50~100ms + 改变 context 让 provider 端 cache miss，代价不值。
- **wiki-link 无别名歧义 UI**：`LIMIT 1` 粗暴匹配，v1 先不做。
- **Modal 无 focus-lock**：Tab 能跳出 modal，等全局做统一 focus-lock 时一起治。

#### 6.28.10 测试覆盖

- `cargo test --lib ai::rag` → **4 passed**（`truncate_chars_handles_multibyte`、`format_truncates_previews_to_160_chars`、`format_includes_all_hits_under_budget`、`format_respects_overall_budget`）——字符级预算 + UTF-8 边界 + 排序/编号正确性。
- `cargo check` / `cargo clippy --lib --no-deps` → 无新增 warning。
- `pnpm check` → **0 errors / 0 warnings**。
- 端到端（embed provider 未配置 / store 为空 / 正常检索）三态 + wiki-link title/stem/unresolved 三分支 + modal 三态（新建 / IPC 失败 / busy 期点击 backdrop）依赖 Tauri runtime，留给手测（见 `delivery_log.md` 顶部条目）。

---

### 6.29 AI 辅助·弹出独立窗口 + AI 关闭时自动关闭（P3-D2b.6）

#### 6.29.1 D2b 收尾：为什么把独立窗做最轻

前面五刀（D2b.1–D2b.5）把 Chat 从"会话存储 → Provider → 非流式 → 流式 + 中断 + history → RAG + wiki-link + modal"一路打通，**每一刀都在后端堆结构**。到 D2b.6，手头想要的只是"把已有 ChatPanel 塞进独立 webview，并把关 AI 时自动带走它"。如果这一刀还要动后端 IPC / 事件协议，就没完没了——于是我们刻意把它做成**零后端改动**：Tauri v2 默认允许前端直接 `new WebviewWindow(...)`，capability 把新 window label 加进白名单就行；跨窗通信复用 `emit/listen` 的全局 bus，事件名用 `chat-standalone:*` 前缀规避碰撞。

收益：后端 185 条 test 原封不动（D2b.5 基础上加了 4 条 RAG test 成 189），这一刀 `cargo check` 连"Compiling"都是秒过；所有 session / provider / RAG 行为都是"和主窗 docked 完全等价"。代价：主窗和独立窗同时运行时，两边的 `ChatPanel` 都会订到同一份 `ai:chat-stream:*` 事件——所以下面 §6.29.4 的"docked 占位符"是故意把主窗的 ChatPanel 卸载掉，确保任何时刻只有一份 listener 活着。

#### 6.29.2 为什么把独立窗做成 SvelteKit 路由而不是独立 entry

选项 A：在 `src-tauri/` 里开第二个 entry html，独立 build pipeline；选项 B：SvelteKit 新加一个路由 `/chat-standalone`，共享 adapter-static build 的 `index.html`。

选了 B，理由三点：

1. **共享 CSS / token / 主题**：主窗 docked 和独立窗内的 ChatPanel 看起来不应有任何差异，连 `--color-accent` / `--font-mono` 都要一致。SvelteKit 一份 build 输出一份 `_app/...` chunk，共享 layout + CSS。独立 entry 就得手动把 token 复制一份，两边极易漂移。
2. **零 build 配置改动**：adapter-static 的 fallback 模式已在 D1 就定了，`/chat-standalone` 自然走 SPA 路由。只要 `+layout.ts` 里 `ssr = false; prerender = false;` 继续生效（是），新路由无需新配置。
3. **Tauri v2 的 index.html 兜底行为**：Tauri v2 对找不到的 asset 默认 fallback 到 `index.html`（见 `tauri-apps/tauri#5082` 讨论里"允许禁用"这一诉求——说明默认就是"允许"），所以 `new WebviewWindow(url: '/chat-standalone')` 会加载 `index.html` → SvelteKit SPA 接管 → 渲染我们的路由。整条链路零改动。

缺点：如果 Tauri 未来加了个 `disableIndexFallback: true`，这一刀会坏；真发生时改成 `/#/chat-standalone`（hash routing）或者 `/?view=chat-standalone` + 在 `+page.svelte` 里分发即可，**是个上游跟进，不是架构债**。

#### 6.29.3 跨窗事件协议 `chat-standalone:*`

五条命名事件，都以 `chat-standalone:` 开头便于主窗 / 独立窗做前缀过滤：

| 事件 | 方向 | Payload | 时机 |
| --- | --- | --- | --- |
| `chat-standalone:ready` | 独立 → 主 | `null` | 独立窗 `onMount` 完成订阅后发一次，让主窗触发初始 `file-path` 推送 |
| `chat-standalone:file-path` | 主 → 独立 | `{ path: string \| null }` | 独立窗 ready 时 + 主窗 `filePath` 变化时 |
| `chat-standalone:open-note` | 独立 → 主 | `{ path: string }` | 独立窗里点 wiki-link chip / citation chip，主窗 editor 实际打开 |
| `chat-standalone:close` | 主 → 独立 | `null` | 主窗 `bringBack()` / `aiEnabled` 变 false 时 |
| `chat-standalone:closed` | 独立 → 主 | `null` | 独立窗 `onDestroy` 无条件发 |

为什么不用 `tauri://destroyed`：Tauri v2 的内置 window-lifecycle 事件只在**被销毁的 webview 自己**的事件总线上冒泡，主窗 `listen('tauri://destroyed')` 收不到。自定义 `chat-standalone:closed` 是 10 行代码的自愿协议，和主窗的 `emit/listen` 是同一套广播 bus，不必借道 Rust。

为什么 `open-note` 走事件而不是让独立窗直接调 IPC 打开笔记：**主窗持有 editor 实例**（CodeMirror state、tab bar、undo stack）。让独立窗直接调 `file_read` 然后把内容返回到 webview 里编辑，相当于独立窗多了一个 editor 副本——D2b.6 不想做这个，就保持"独立窗只是 chat panel，open-note 请求主窗接手"。

#### 6.29.4 Docked 占位符为什么必要

Tauri v2 `emit()` 广播所有 webview 意味着：如果主窗的 `ChatPanel` 和独立窗的 `ChatPanel` 同时 mount，两边都会收到 `ai:chat-stream:delta` 事件，都会把 token append 到**自己那份** `streamingContent`。最终谁先跑到 `done` 事件谁就跑 `refreshSessions` + `loadActiveSession`，两边互相覆盖——乱序 reload、乐观 user bubble 可能被清空两次等等。

所以主窗的 docked 视图在 `standaloneOpen` 时**整块切到占位符**——不 mount `<ChatPanel>`。独立窗关闭 → `EV_CLOSED` → `standaloneOpen = false` → 主窗重新 mount docked ChatPanel → `onMount` 里的 `refreshSessions` 从磁盘重读最新 session（独立窗期间的消息全部已经持久化到文件）。这条路径让"独立窗 ↔ docked 切换"天然只有一份 listener 活着。

占位符本身做了三件事：（1）醒目告知"AI 对话已在独立窗口"；（2）`聚焦独立窗口` 按钮帮用户把被其他窗覆盖的独立窗拉到前台（尤其多屏场景）；（3）`取回到此处` 按钮等价于 OS 关闭按钮，但 UX 更明显。

#### 6.29.5 `bringBack()` 的优雅关闭 + 600ms 兜底

关独立窗有两种方式：

1. **主窗 `emit(EV_CLOSE)` → 独立窗 `listen(EV_CLOSE)` → `getCurrentWindow().close()`**：走 Svelte `onDestroy`，`ChatPanel` 的 `aiChatStreamCancel` / `unlisten*` 一路清完，后端 spawn task 在下次 poll 时观察到 cancel flag 退出，不泄资源。**这是主路径**。
2. **主窗直接调 `standaloneWindow.close()`**：绕过 Svelte 卸载，强杀 webview。spawn task 的 cancel flag 不会被 set，流式 IPC 会把已累积的 content 持久化成 assistant turn 后发送 `done` 事件（但没有听众，事件被丢）。不优雅但不会崩，**这是 600ms 兜底**。

兜底存在的理由：独立窗若主线程被卡（比如用户在独立窗里执行了一段 infinite loop 的 `@html`，或被 OS 低内存冻结），`EV_CLOSE` 听不到。UX 上用户按了"取回到此处"等 600ms 没动静就非常难受。600ms 是"听得到就来得及响应、听不到就能察觉异常"的常见阈值。

#### 6.29.6 `aiEnabled` 切 false 自动关独立窗

Settings 里关 "AI 辅助" 的语义是"把 AI 这摊东西藏起来"。tab header 会藏（D2b.3 既有行为），related-notes 会停（D1 既有行为），按理也要把独立窗收掉——否则用户"关了 AI"但还有个独立的 AI 对话窗开着，很难自圆其说。

实现是一条 `$effect`：

```svelte
$effect(() => {
  if (!aiEnabled && standaloneOpen) {
    void bringBack();
  }
});
```

走同一份 `bringBack()`（emit `EV_CLOSE` + 600ms 兜底），独立窗优雅关 + Svelte onDestroy 跑 + 取消任何在流 stream。对称性：开 AI 不会自动重新打开独立窗（用户必须再点 `⧉`），因为"开着→关→再开→自动恢复独立窗" 的状态记忆会让用户困惑。

#### 6.29.7 握手协议：为什么需要 `ready`

主窗 `openStandalone()` 是同步的——`new WebviewWindow(...)` 返回后 webview 还在 loading，SvelteKit 还没 `onMount`，独立窗的 `listen(EV_FILE_PATH)` 还没绑上。若主窗 `emit(EV_FILE_PATH)` 得太早，事件被丢。

解决：独立窗 `onMount` 完成所有 `listen` 后 emit `EV_READY`；主窗 `listen(EV_READY)` 里 emit `EV_FILE_PATH` 作为 response。这是一个"server-first handshake"模式，避免 race。代价是每次开独立窗多一个事件往返（~几 ms），可接受。

#### 6.29.8 Re-mount 恢复

Panel 被 unmount / re-mount（vault 切换最典型）时，独立窗作为一个独立 webview 不会一起被销毁（它走 Tauri 进程生命周期，不是 Svelte 组件生命周期）。如果 re-mount 后 Panel 不主动查一下，就会出现"主窗 docked 里看不见占位符但独立窗还在那"的灵异态。

`onMount` 里 `WebviewWindow.getByLabel(STANDALONE_LABEL)` 查一下，若存在：`standaloneOpen = true` + `ensureStandaloneListeners()` + 补发一次 `EV_FILE_PATH`。让两端重新对齐，独立窗继续用，主窗显示占位符。

#### 6.29.9 Capability 变更的最小面

只加了一个字段：

```diff
-  "windows": ["main"],
+  "windows": ["main", "chat-standalone"],
```

没加任何新 permission。`core:default` 已经包括 `core:webview:default`（含 `allow-create-webview-window`、`allow-set-focus` 等）+ `core:window:default`（含 `allow-close`），对我们需要的 `new WebviewWindow` / `setFocus` / `close` 已经足够。不额外加 permission 的好处：安全面不扩大，AppSec review 不需要重新确认。

#### 6.29.10 不做事项

- **多独立窗**：一个 label 硬编，一次只能开一个。真要"每个 session 一个独立窗"（Notion 那种 tab）要维护 `Map<session_id, WebviewWindow>` + 生成动态 label。v1 不做。
- **位置 / 尺寸持久化**：每次都用默认 720 × 860。接 `tauri-plugin-window-state` 即可但本刀不上。
- **主窗 docked + 独立窗并存**：两边同时跑 ChatPanel 会出事（见 §6.29.4）；想支持得把 stream 事件改 `emit_to(label, …)` 定向 + 在 payload 带 webview 身份。
- **跨窗 session 同步**：主窗和独立窗看同一会话列表，但现在切会话的动作只会发生在一个位置（独立窗开着时 docked 无 ChatPanel，docked 开着时独立窗不存在）。真要并存才有这个同步需求。
- **独立窗内自己的 Settings 面板**：Settings 仍然在主窗。独立窗用户要改 chat_model 得切回主窗。v1 场景够用。

#### 6.29.11 测试覆盖

- `cargo test --lib` → **189 passed; 0 failed**（= D2b.5 的 185 + 4 条 rag 模块测试；本刀无新增后端代码，也无 regression）。
- `pnpm check` → **0 errors / 0 warnings**。
- `pnpm build` → success（SvelteKit adapter-static 把 `/chat-standalone` 路由打进 `_app/` chunk，`build/` 顶层还是只有 `index.html + _app/`——SPA fallback）。
- 跨窗事件流 / Svelte onDestroy 清理 / Tauri v2 webview 生命周期的端到端验证依赖 Tauri runtime，留给手测（`delivery_log.md` 顶部列了 8 步脚本覆盖打开 / 流式 / wiki-link 路由 / file-path 同步 / OS 关闭 / aiEnabled 切 false 联动 / 双打不生成两个窗 / 并发 stream）。

### 6.30 AI 辅助·`ai_complete` 单次补全 IPC（P3-D3.1）

#### 6.30.1 为什么先切一刀"纯管道"

D3 的三条写回命令（summarize / suggest tags / MOC AI draft）本质上都是"给定一段 prompt，拿到一段回复，丢到 diff modal"的同一条管道。先把这条管道抽成 `ai_complete` 独立 IPC 有三个收益：

1. **prompt engineering 与 UI 解耦**：D3.3–D3.5 只需要组装 prompt + 展示结果，不用再重写"建 provider / 算 token budget / 跑 SSE / 处理 cancel"的八十行模板。
2. **modal 骨架（D3.2）可以独立开发**：D3.2 的 `DiffPreviewModal` 在 D3.1 这层已经能 mock `aiComplete()` 做端到端联调，不需要等三条写回命令全部就位。
3. **chat 和写回的 cancel 注册表完全分表**：`chat_streams` 和 `complete_requests` 各占一个 `Arc<Mutex<HashMap>>`，任何一方的 "cancel all" / "关 AI 开关清空 in-flight" 操作不会误杀另一方。

#### 6.30.2 为什么是"非流式"语义（而不是复用 `ai_chat_stream_*`）

写回场景的交付单位是**整段完整结果**——TL;DR 一整段、tags 一整组、MOC 一整套 body。diff modal 需要等结果完整才能跑 diff 计算（流式过程中的半段文本跑 diff 毫无意义）。所以：

- **对前端**：`aiComplete()` 返回 Promise\<CompleteResult\>，等后端 `await` 完成。UI 展示 loader，cancel 按钮走 `aiCompleteCancel(id)`。
- **对后端**：内部仍然调 `provider.chat_stream(req).await`——OpenAI-compatible 协议里"非流式"其实也是一次 SSE 调用，backend 替你聚合 deltas。我们复用同一份 `provider::chat_stream()` 实现（SSE 解析 / usage trailer / cancel flag polling），只是 command 层把 deltas 攒起来再一次性返回。这样 chat 和写回共用一条 transport，不会在两份 parser 之间分叉（Anthropic / Gemini 等新 provider 接入时只改一处）。

唯一多出来的成本是用户没有 "逐字流出" 的视觉反馈，典型写回任务在几秒内完成，loader 足够；若将来用户反馈"长 summarize 等得焦虑"再把 IPC 升级成 `ai_complete_stream` 事件投递即可，向前兼容。

#### 6.30.3 `CompleteResult` / `CompleteFailure` 形状

```rust
pub struct CompleteResult {
    pub ok: bool,
    pub reply: Option<String>,          // trimmed on success
    pub input_tokens: Option<u32>,       // when provider reports usage
    pub output_tokens: Option<u32>,
    pub cancelled: bool,                 // orthogonal to ok
    pub failure: Option<CompleteFailure>,
}

pub struct CompleteFailure {
    pub kind: String,                    // "network" | "auth" | "rate_limit" | "invalid_request" | "other"
    pub message: String,
    pub retry_after_secs: Option<u64>,
}
```

- 和 `ChatSendResult` / `ChatSendFailure` 刻意**风格一致但结构不同**：前端的 banner renderer 可以复用（一个 `kind` + `message` + 可选 retry-after 的三档文案），但 `CompleteFailure` **不带** `user_message_persisted`——写回命令从来不写 chat 存储，多带一个字段只会误导调用方。
- `cancelled` **与 `ok` 正交**：流式过程中用户点 cancel 但已累积到 200 字 TL;DR → `ok: true` + `cancelled: true` + `reply: "..."`。前端可以提示"已取消但已有结果，保留 / 丢弃？"。`ok: false` + `cancelled: true` 代表"点太快，什么都没拿到"。
- `reply` 始终 trim 后传出，避免 provider 尾随空行污染 diff 视觉。空 reply（provider 返回 zero content）**降级为失败**——写回场景下一个空字符串没法生成 diff，返回成功只会误导。

#### 6.30.4 Cancel 注册表分表

```rust
pub struct AppState {
    // ... 其他字段
    pub chat_streams: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,        // D2b.4
    pub complete_requests: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,   // D3.1 (本刀)
}
```

两表独立的理由：

- **语义隔离**：chat stream 和 write-back completion 是两种**用户期望行为**不同的操作（chat 期望看到逐字；write-back 期望看到整段 diff）。共享一个注册表意味着调用 `"cancel all inflight AI"` 无法区分要只取消哪一侧。
- **request_id / stream_id 命名不冲突**：前端生成 id 时不需要跨两种 IPC 全局去重，各自 nanoid 即可。
- **pre-flight 并发安全**：duplicate-id 检查在各自表内做，两个命令之间完全无锁竞争。
- **成本几乎为零**：多一条 `Arc<Mutex<HashMap>>`，空状态下三行代码。

#### 6.30.5 pre-flight 验证清单（代码顺序决定了"什么错误优先暴露"）

1. `request_id.trim().is_empty() || len > 128` → `invalid_request` "invalid request_id"——保护注册表键空间。
2. `user_prompt.trim().is_empty()` → `invalid_request` "user_prompt is empty"——空 prompt 是调用方 bug，不浪费 API 配额。
3. `complete_requests.contains_key(&request_id)` → `invalid_request` "request_id already in use"——同 D2b.4 的 duplicate-stream guard。
4. `build_configured_chat_provider(cfg, secrets)` → 各种 `invalid_request` 包装（"no AI provider configured" / "chat_model is empty" / …）。这一步失败**不会注册 cancel flag**，因为根本没到能 cancel 的状态。
5. 注册 cancel flag（`complete_requests.insert(request_id, Arc::new(AtomicBool::new(false)))`）。
6. 调 `provider.chat_stream(req).await`——**这一步失败也会立刻 `cleanup()` 摘掉 flag**，避免孤儿条目（见 §6.30.6）。

顺序很关键：任何"在注册 flag 之前"失败都走纯 `Ok(CompleteResult { ok: false, ... })` 返回，不污染 AppState。

#### 6.30.6 清理语义 & Cancel 的边界情况

`cleanup()` 闭包在函数体内定义一次，所有终结路径都调：

| 路径 | `cleanup()` 时机 |
| --- | --- |
| provider 构造失败（pre-flight 5）| 不需要 cleanup（还没注册） |
| `provider.chat_stream(req).await` 返回 `Err` | 调 cleanup → 返回失败 |
| stream loop 期间 cancel flag 翻 true | break + cleanup（保留已累积 reply） |
| stream loop 期间拿到 `Err(e)` | break + cleanup → 返回失败 |
| stream 正常结束 `next().await` 返回 `None` | cleanup → trim + 判空 reply |

cancel 在 stream loop 的**两处**检查：
- 进 loop 时先 `if cancel.load(...) { break; }`——快取消场景（发送后立刻点 cancel）。
- 退 loop 后再检查一次 `if cancel.load(...)`——规避"cancel 命令在 break 之后才到但之前 flag 已 true"的罕见竞态。

空 reply（cancelled 前没收到任何 content 或 provider 返回空）→ `ok: false`，`failure.message` 根据是否 cancelled 分两档（"cancelled before any content arrived" / "provider returned an empty reply"），UI 不需要特殊处理，都按 banner 渲染。

#### 6.30.7 `ai_complete_cancel` 的幂等 & race 窗口

```rust
pub fn ai_complete_cancel(request_id: String, state) -> AppResult<bool> {
    let guard = state.complete_requests.lock().unwrap();
    match guard.get(&request_id) {
        Some(flag) => { flag.store(true, Relaxed); Ok(true) }
        None => Ok(false),
    }
}
```

- **幂等**：同一个 id 连调 100 次都 `Ok(true)`，flag 已经 true 只是再 store 一次。
- **找不到 id 不是错误**：前端 UI 双击 cancel 按钮、或 cancel 刚好在命令返回后抵达，两种情况都 `Ok(false)`——让前端不必 try/catch。
- **不 emit 事件**：和 `ai_chat_stream_cancel` 一样尽力而为。实际的"取消成功 / 有部分结果 / 什么都没拿到"信息在 `aiComplete` 的返回值里（`cancelled: true` + `reply: Some/None`）。
- **无冲突排查**：如果 cancel 在 `ai_complete` 注册 flag 前就到（极窄 race），前端会拿到 `Ok(false)`。正确处理：UI 等 `aiComplete` 返回，若 `ok: true + cancelled: false` 就是"cancel 太晚了"，直接展示完整 reply——这是用户可接受的。

#### 6.30.8 与 chat 侧差异对照表

| 能力 | `ai_chat_stream_start` (D2b.4) | `ai_complete` (D3.1) |
| --- | --- | --- |
| 入参会话 | 需要 `session_id`（附加到现有 chat 会话） | 不需要——写回场景没有多轮 |
| 用户消息持久化 | 是（`ChatRole::User` 写 jsonl） | 否 |
| 助手消息持久化 | 是（stream 结束时写 `Assistant`） | 否——调用方决定是否写回 markdown |
| RAG 注入 | 是（pre-flight `try_build_rag_context`） | 否——prompt 由调用方完全掌控 |
| History 截断 | 是（`truncate_history_to_budget`） | 否——调用方负责裁 prompt |
| 流式事件 | 是（`ai:chat-stream:{delta,done,error}`） | 否——命令 `await` 期间聚合 |
| 失败 struct | `ChatSendFailure { …, user_message_persisted }` | `CompleteFailure { kind, message, retry_after_secs? }` |
| Cancel 注册表 | `AppState::chat_streams` | `AppState::complete_requests` |
| temperature / max_tokens | 内部硬编 `None` | 调用方显式传 |

两条命令**共享**：`runtime::build_configured_chat_provider`、`ChatRequest` / `ChatTurn` / `ChatRole` 类型、`classify_provider_error()` 翻译、SSE transport（都走 `provider.chat_stream`）。

#### 6.30.9 刻意不做

- **Prompt 长度硬上限**：caller 塞 100K 字符的 user_prompt 会让 provider 直接返回 `InvalidRequest (context_length_exceeded)`，错误链路已有处理。加后端硬上限反而会让 D3.3 的"用整个 note body 做 summarize"的路径变得反直觉——caller 更清楚自己的 prompt 形状。
- **流式事件（增量 delta）**：见 §6.30.2。保持非流式直到用户反馈不够。
- **批量 `ai_complete_batch`**：三条命令 v1 都是"对当前 note"或"对单个 tag"的单次操作，不需要批量入口。P3-D4 的"对整个 tag 做摘要"路径会走 `ai_complete` × N，不共享 in-flight。
- **Prompt template 也住后端**：三条命令的 prompt 模板放在前端（`commandRegistry` / 写回命令文件），因为模板变更频繁、和 UI 文案高耦合。后端只做"transport + classify + cancel"，保持窄接口。
- **Cancel 全局 API**：不提供 `cancel_all_complete_requests` 之类 batch。"关闭 AI 开关"路径目前是 D2b.6 的 `bringBack()` 风格——让 UI 侧在各 in-flight id 上 `aiCompleteCancel(id)` 逐个取消。未来若有大量并发需要，再加 batch API。

#### 6.30.10 测试覆盖

- `cargo test --lib` → **189 passed; 0 failed**（D3.1 本身不增加单元测试——与 `ai_chat_stream_start` 同策略：命令体深度依赖 `State<AppState>` + Tauri runtime 不适合纯 unit，provider-层的 SSE 解析 / cancel flag 语义已有 MockProvider + OpenAI parser 单测覆盖）。
- `cargo check` → 零 warning。
- `pnpm check` → **0 errors / 0 warnings**。
- `pnpm build` → success（前端 TS wrapper 类型接入）。
- 端到端验证留到 D3.2 的 diff modal 手测（那时才有真实触发路径）。

---

### 6.31 AI 辅助·Diff 预览 modal（P3-D3.2）

#### 6.31.1 为什么先切一刀"只做 UI 壳"

D3.3 / D3.4 / D3.5 三条写回命令都会遵循同一个交互节律：

> **发命令 → 组 prompt → 调 `aiComplete` → 拿到候选内容 → 用户确认 → 落盘**

第四、五步（"把候选内容展示给用户并等确认"）在三个命令里几乎一模一样——差别只有标题、prompt 模板、以及 accept 回调里该写哪个字段。把这层 UI 抽成共享组件（`DiffPreviewModal.svelte` + `diffLines.ts`），三条命令各自只需要写"自己的 prompt + 自己的 accept"两段，剩下的 modal 生命周期 / 快捷键 / error banner / loading 态都共享。

D3.2 刻意**不接任何命令**——只产 `src/lib/ai/{DiffPreviewModal.svelte, diffLines.ts}`；`+page.svelte` / 命令面板 / `ChatPanel` 都不改。D3.3 接入时才会在 `+page.svelte` 里加 `summarizeModalOpen` 状态 + 挂载 `<DiffPreviewModal … />`。

#### 6.31.2 行 diff 算法为什么自写而非引 `jsdiff`

- **bundle 代价**：`diff` 包 unpacked ~40 KB + 需要额外装 `@types/diff`。LCS 行 diff 算法本身 30 行，放在一个独立文件里反而更易审计。
- **场景有界**：write-back 目标永远是单篇笔记（甚至只是 frontmatter 一行），`m × n` 的 DP 表最大也就几百万 cell，几毫秒完成。Myers 这种针对"超长文件"的优化在这里是过度工程。
- **tie-break 语义**：`dp[i+1][j] >= dp[i][j+1]` 让删除优先，连续的 remove 聚团、连续的 add 聚团，视觉上更整齐；若用 `diff` 的默认行为得自己再 post-process。
- **可测性**：算法独立成纯函数 `diffLines(a, b) → DiffPart[]`，可以在没有 Svelte runtime 的 Node 里直接跑验证（本次已手验六条边界：identical / insertion / deletion / replacement / empty→text / text→empty）。

#### 6.31.3 组件形状：三态渲染 + props 与 `CompleteResult` 映射

Modal 的 body 永远处于三态之一，直接映射 `aiComplete` 的返回值：

| UI 态 | 触发条件 | 内容 | Footer |
| --- | --- | --- | --- |
| **loading** | `proposed === null && loading === true` | `<dpm-spinner>` + 「AI 正在生成…」| 可选「取消生成」按钮（调 `onCancel` → `aiCompleteCancel`）|
| **error** | `error !== null` | banner（`labelForKind(error.kind)` 中文标题 + `error.message` + 可选 `retry_after_secs` 提示） | 仅「放弃」（accept 按钮隐藏）|
| **diff** | `proposed !== null && !error` | `+N / -M` stats 徽标 + 行级 diff 列表；零变化时显示「无变化（AI 建议与原文一致）」 | 「放弃」+「应用」 |

调用方**不需要**把 `CompleteResult` 拆开——直接往 props 上喂：

```ts
<DiffPreviewModal
  open={modalOpen}
  title="AI 摘要预览"
  description="将覆盖 frontmatter.summary"
  original={noteBody}
  proposed={result?.ok ? result.reply! : null}
  loading={inFlight}
  error={result?.ok === false ? result.failure! : null}
  onAccept={writeBackSummary}
  onDiscard={closeModal}
  onCancel={() => aiCompleteCancel(requestId)}
/>
```

「`proposed === null` 表示 loading」与「`error !== null` 表示 error」构成一个显式的 discriminated mode switch，组件内部不需要额外布尔来表达"到底在哪一态"。

#### 6.31.4 快捷键 & 防误操作

| 键 / 动作 | loading 时 | diff 时 | error 时 |
| --- | --- | --- | --- |
| **Esc** | `onCancel()`（若有） | `onDiscard()` | `onDiscard()` |
| **Cmd/Ctrl+Enter** | 忽略 | `onAccept()`（async 会 latch `accepting` 避免重入） | 忽略 |
| **Backdrop 点击** | `onCancel()`（若有，不让误点丢失 in-flight） | `onDiscard()` | `onDiscard()` |
| **accept 按钮禁用** | N/A | `accepting \|\| stats.added + stats.removed === 0` | 永远禁用 |

`accepting` latch 是为了扛"用户看到 diff 满意之后双击应用"这种常见动作——第一次点进入 `应用中…`，第二次点直接被 disabled 状态吞掉；如果 `onAccept` 是写盘 + 重建索引这种几百毫秒的异步，不做 latch 就会触发两次写回。

#### 6.31.5 样式命名：`.dpm-*` scoped

遵循 `ChatPanel` 的 `.ns-*`（新会话 modal）前缀模式，避免跟 `+page.svelte` 里全局的 `.modal-*` 冲突（Svelte 组件 scope 不保证全局 class 可用）。颜色全部走 design tokens：`--color-surface` / `--color-border` / `--color-accent` / `--color-danger` / `--color-bg-hover`；diff 高亮用 `color-mix(in oklch, #2f7d32 10%, transparent)` 保持与系统配色的色感一致（绿加 / 红减是跨工具的共识色，不走 token）。

#### 6.31.6 与 `ns-modal` / 以后的 `tag-merge-modal` 的关系

| 需求 | 承载组件 | 备注 |
| --- | --- | --- |
| 新建 AI 会话（标题输入 + 关联笔记 checkbox） | `ChatPanel.ns-modal`（内联 markup）| 形状特殊，不复用 |
| Summarize / MOC draft 的候选内容审查 | `DiffPreviewModal` | **本刀** |
| Suggest tags 的候选集合勾选 | **尚未决定**：D3.4 选型要么给 `DiffPreviewModal` 加 `body` snippet prop，要么起单独的 `TagMergeModal.svelte` | checklist 合并不是行 diff 能表达的语义，先不在 D3.2 里强行兼容 |

这个分层的关键在于：**不强行把所有 AI 写回 UI 塞进一个组件**。`DiffPreviewModal` 负责"有 before / after 两段文本、要给用户看差异"的子集；checklist / 合并 / 多选的场景会以自己的组件处理，只共享 `ai_complete` / `CompleteFailure` / `labelForKind` 这些 IPC 层的抽象。

#### 6.31.7 刻意不做

- **虚拟滚动**：当前写回目标（一行 summary / 几个 tag 名 / 几十行 MOC）远低于卡顿阈值。真写长篇再接 `svelte-virtual-list`。
- **word-level / char-level diff**：summary 一般是整段重写，行级 diff 展示就是整块 `-旧 +新`，看着够清楚；以后做"AI 微调段落"类命令再接 `fast-diff` / `diff-match-patch`。
- **动画过渡**：跟 `ns-modal` 对齐（瞬发出现 / 瞬发消失）。加 `transition:fade` 一行的事，现在不加是避免"某些 modal 有动画、某些没有"的不一致。
- **diff 行内的 syntax highlight**：markdown 写回内容不需要语法高亮，保持纯文本 mono 字体即可。

#### 6.31.8 测试覆盖

- `diffLines` 六条边界手验（Node 内联版），输出与预期一致；tie-break 规则保证 remove 早于 add。
- `pnpm check` → **0 errors / 0 warnings**（含 svelte-kit sync + svelte-check）。
- `pnpm build` → success。
- `ReadLints` 干净（两文件）。
- 端到端手测留到 D3.3——届时会在 `+page.svelte` 真正挂载，跑通 loading / diff / error / cancel 四条路径。

---

### 6.32 AI 辅助·`> Summarize current note` 三档写回命令（P3-D3.3）

#### 6.32.1 为什么"三档独立命令"而非"一档 + modal 选 target"

命令面板的 fuzzy-search 本身就是最便捷的 target picker。用户敲 `> sum front` / `> sum top` / `> sum clip` 一气呵成进入对应流程，比"敲 `> sum` 开 modal → 看到三个单选框再选一个 → 再点开始"少一次 round-trip。

更重要的是 **clipboard 档天然不适配 modal**：

- `frontmatter` / `top` 档要展示"将如何修改文件"——diff preview 是最合理的确认手段。
- `clipboard` 档不修改任何文件——diff 指向谁都不对；拿到 reply 就该直接写剪贴板，弹 modal 反而让用户多一次点击。

所以分三条命令后：

| 命令 | 流程 | UI |
| --- | --- | --- |
| `summarize-to-frontmatter` | 读 body → `aiComplete` → `rewriteFrontmatter(body, { summary })` → 用户 accept → `fileWrite` | `DiffPreviewModal`（D3.2） |
| `summarize-to-top` | 读 body → `aiComplete` → `insertTldrAtTop(body, summary)` → 用户 accept → `fileWrite` | `DiffPreviewModal`（D3.2） |
| `summarize-to-clipboard` | 读 body → `aiComplete` → `navigator.clipboard.writeText(reply)` | toast only |

三条命令共享**同一个** prompt、**同一次** `aiComplete`——只在"拿到 reply 之后怎么处理"分支。

#### 6.32.2 Prompt 住前端的理由

系统提示：

```
You are a concise-summary writer for a personal knowledge base.
Write a faithful, information-dense TL;DR in the user's language
— if the note is Chinese, reply in Chinese; otherwise English.
Output ONLY the summary paragraph, without any heading, bullet,
quote, or markdown decoration. Keep it to 1-3 sentences, under
120 characters when possible.
```

几个显式决策：

1. **输出形状硬约束**：`Output ONLY the summary paragraph, without any heading / bullet / quote / markdown decoration`——因为拿到 reply 后立刻 `rewriteFrontmatter(body, { summary: reply })`，若模型返回 `# 摘要\n\n- 第一点\n- 第二点`，会把 frontmatter 写成多行 YAML 搞坏 scalar 语义。同样 `insertTldrAtTop` 自己加 `> **TL;DR**` 前缀，模型再多加一个 `> ` 会出现 `> > **...** > ...` 这种套娃。
2. **语言对齐**：`match the note's language — if Chinese, reply in Chinese`。不做前端 CJK 比例检测，省一次额外分支；模型在 99% 的单语言笔记上表现稳定。
3. **长度"软"上限**：`under 120 characters when possible` 而不是硬裁断。前端如果收到 300 字的 reply 再客户端截断只会让句子被砍半句，不如让用户看到长摘要后自己决定是否 accept。
4. **前端而非后端**：prompt 模板频繁调优（语气 / 长度 / 语言指令），跟 UI 文案高耦合，不值得让后端 Rust 重编。后端只做 transport（`ai_complete`）+ classify（`classify_provider_error`）+ cancel（`AtomicBool`），保持窄接口。

`buildSummarizePrompt(body)` 做的事：调 `stripFrontmatter(body)` 剥掉 YAML 再 `trim()`，避免模型被元数据分心。空 body 会在 caller 侧（`runSummarizeCurrentNote`）预先拒绝，prompt helper 不重复校验。

#### 6.32.3 `applySummaryToBody` 纯函数 & 为什么要纯

```ts
export function applySummaryToBody(
  body: string,
  summary: string,
  target: SummarizeTarget  // 'frontmatter' | 'top'
): string;
```

- `frontmatter` 分支**复用** `$lib/commands` 里已有的 `rewriteFrontmatter(body, { summary })`——那是 P3-A 阶段就稳定的 regex-based YAML writer，覆盖"无 frontmatter 块时 prepend / 已有 key 替换 / 新 key 追加"三种情况。
- `top` 分支走新加的 `insertTldrAtTop(body, summary)`：识别 `^---\n…\n---\n?` frontmatter 块后把 `> **TL;DR** …\n\n` 插在块后 + 正文前，前后各留一空行；无 frontmatter 的 note 直接 prepend 到开头。
- `summary.trim().replace(/\s+/g, ' ')` 规范化：把多行 / 多空格压成单行。因为 frontmatter 是单行 scalar 容不下换行，blockquote 虽然可以多行但单行更紧凑。

**纯函数的好处**：`DiffPreviewModal.proposed` 在 `+page.svelte` 里就是 `$derived.by(() => applySummaryToBody(original, reply, target))`。未来若要加"用户可编辑 reply 再 apply"的 affordance，只要把 `reply` 换成一个 textarea `$state` 即可，diff 自动联动；纯函数不污染任何外部状态。

#### 6.32.4 "top" 档刻意不检测旧 TL;DR 的理由

`insertTldrAtTop` 永远插入，哪怕笔记顶部已经有 `> **TL;DR** 旧摘要`。原因：

- **精准检测困难**：什么才算"旧 TL;DR"？`> **TL;DR** X`？`> TL;DR X`？`> **总结** X`（中文笔记用户可能手写）？`> > **TL;DR** X`（被嵌套引用）？用硬 regex 会误判；用 LLM 再做一次 classifier 太贵。
- **误删 > 冗余**：若误判把用户原有的 blockquote 当旧 TL;DR 干掉，破坏性远大于多一行可见的新 TL;DR。
- **Diff 让用户自救**：`DiffPreviewModal` 会把原有 `> **TL;DR** 旧摘要` 展示在 `same` 区（因为它并没被修改），新行出现在 `add` 区。用户看到"两个 TL;DR"就明白情况，可以 Discard 后手动删除旧的再 re-run，一次操作就解决。

作为对冲：**如果日后用户反馈"每次都要手删旧的好烦"**，再加一条"smart replace"选项（要么 `insertTldrAtTop` 接一个 `replaceExisting: boolean` 参数，要么独立 `replaceTldrAtTop` 函数）。目前保持最简行为。

#### 6.32.5 `runSummarizeCurrentNote` 的状态机 & race guard

关键状态（全部放 `+page.svelte`，和其他 palette-driven modal 同家）：

```ts
let summarizeOpen = $state(false);
let summarizeLoading = $state(false);
let summarizeError = $state<CompleteFailure | null>(null);
let summarizeReply = $state<string | null>(null);
let summarizeOriginal = $state<string>('');
let summarizePath = $state<string>('');
let summarizeTarget = $state<SummarizeTarget>('frontmatter');
let summarizeRequestId: string | null = null;  // 注意：不是 $state
```

`summarizeRequestId` 故意**不是 `$state`**——它只是用来判断"当前 in-flight 请求是不是我这次的"，做成响应式会让每次赋值触发 `$effect` 无意义抖动。

流程：

1. 入口校验：markdown 文件 / 非 `.mynotes/` / `aiEnabled`；任一失败 toast 后 return。
2. `await drainPendingSaves()` 先刷 autosave——用 `fileRead(path)` 拿 canonical body 而不是 `editorContent`，避免未保存改动干扰；写回也对 path 不对缓冲区，防止 autosave 和我们的 `fileWrite` 互相覆盖。
3. Clipboard 档到此分叉：`aiComplete` → `navigator.clipboard.writeText` → toast。
4. Frontmatter / top 档：先 `summarizeOpen = true; summarizeLoading = true; summarizeReply = null; summarizeError = null`——modal 秒出 loading 态，不等 `aiComplete` 的网络 round-trip。
5. 生成 `requestId = makeSummarizeRequestId()` + `summarizeRequestId = requestId`，然后 `await aiComplete(...)`。
6. resolve / reject 后做**三段 stale-request guard**：
   - 成功：`if (summarizeRequestId !== requestId) return;`——避免用户 discard + re-run 期间旧请求的 reply 盖掉新请求 state。
   - 异常：同样 guard。
   - finally：`if (summarizeRequestId === requestId) { summarizeLoading = false; summarizeRequestId = null; }`——若用户已经 cancel（下面的路径会把 `summarizeRequestId` 抢为 null），这里不再重置 loading flag，防止 "discard 后 modal 意外闪一下 loaded 态"。

Cancel 路径：

```ts
async function cancelSummarizeInFlight() {
  const rid = summarizeRequestId;
  summarizeRequestId = null;   // 先抢走 id，让 resolve 分支的 guard 命中
  if (rid) { await aiCompleteCancel(rid); }
  closeSummarize();
}
```

"先抢 id 再 cancel"顺序重要——`aiCompleteCancel` 是 async，中间几十毫秒里如果不先抢 id，`aiComplete` 可能已经 resolve 并跑到 `summarizeLoading = false` 那支，导致 modal 闪现成 loaded 态再关闭。抢 id 后 resolve 那支命中 guard 直接 return，干净利落。

#### 6.32.6 写回 + 编辑器重载的一致性

```ts
async function applySummarize() {
  const path = summarizePath;
  const newBody = summarizeProposed;
  if (!path || newBody == null) return;
  await fileWrite(path, newBody);
  // 若当前打开的还是同一文件，强制 editor 重载：
  if (vaultState.openFilePath === path) {
    const fresh = await fileRead(path);
    editorContent = fresh;
    pendingSave = null;
  }
  pushNotice(/*...*/, 'success');
  closeSummarize();
}
```

`editorContent = fresh; pendingSave = null` 这一对是**已知模式**——参考 `runSetProjectStatus` 的注释：watcher 只重建 SQLite 索引，不会把文件内容推回 Svelte 层。没有这个重载，用户看到的 editor buffer 还是写回前的内容，除非点别的文件再点回来触发 `openFile → fileRead`。

`pendingSave = null` 是关键——如果此时有尚未触发的 autosave timer，它会把 stale 的 editor buffer 写回去盖掉我们刚刚写入的 summary。

#### 6.32.7 `paletteCtx` 的 `aiEnabled` gate 与命令可见性

旧状态：`runShowRelatedNotes` / `runEmbedCurrentNote` 不 gate `aiEnabled`，用户 AI 关了还能看到命令，触发后才知道要先开 AI。这是历史遗留——P3-D1 时没想清楚。

D3.3 的三条 summarize 命令**全部 gate `ctx.aiEnabled`**：`when: (ctx) => ctx.aiEnabled && markdown && !.mynotes/`。AI 关的时候命令在 palette 里直接不出现——比 runtime 报错更好的 UX，符合"不让命令出现在不能用的上下文里"的原则（对照 `extract-from-project` 非 project-note 时不出现 / `promote-current` 非 inbox 文件时不出现）。

未来把 `runShowRelatedNotes` / `runEmbedCurrentNote` 也 gate `aiEnabled` 是一刀小改动，但不在 D3.3 范围——跟 P3-D3 主题无关。

#### 6.32.8 与 chat / related-notes 的共存语义

| 场景 | `aiComplete` 用量 | `chat_streams` 注册表 | `complete_requests` 注册表 |
| --- | --- | --- | --- |
| ChatPanel send | 0 | 1 | 0 |
| Summarize 命令（任一档） | 1 | 0 | 1 |
| 并发（同时 send chat + 跑 summarize） | 1 | 1 | 1 | 
| AI 关闭 | 两边 in-flight 靠 `ChatPanel.bringBack` + `closeSummarize`（手动关 modal 也算） |

D3.1 §6.30.4 设计的**分表 cancel 注册表**在这里第一次展现价值——chat stream cancel 不会误杀 summarize in-flight，反之亦然。相同 `request_id` 在两张表里互相没关系，不会命名冲突（chat 用 nanoid，summarize 用 `sum-<base36>-<rand>` 前缀，肉眼区分也方便调试）。

#### 6.32.9 刻意不做

- **"重新生成"按钮**：reply 出来后用户只能 accept / discard。真需要的时候再运行一次命令足够，多一个按钮是 modal 膨胀的开端。
- **Prompt 编辑入口**：目前 system prompt 写死在 `summarizePrompt.ts`。用户想改写"用更正式的口吻" / "一律英文" 等，只能改代码。若做成 Settings 里可编辑，需要处理 template placeholder / default fallback / 版本迁移——不是 D3.3 范围。
- **chunked summarization for 长笔记**：provider 的 `context_length_exceeded` 会从 `CompleteFailure.invalid_request` 路径冒出来并展示在 modal banner。真正要做"10K 字长文分段 summarize 再 summarize"留到 P3-D4。
- **`clipboard` 档的 diff preview**：见 §6.32.1。
- **Editor 内 inline preview**：有些笔记软件会把 AI 摘要直接嵌到编辑器里做 marker。对 MyNotes 来说这会破坏"markdown 是 source of truth"的原则——摘要要么是 frontmatter 字段、要么是 body 里的真实文本。

#### 6.32.10 测试覆盖

- `pnpm check` **0 errors / 0 warnings**；`pnpm build` success；`ReadLints` 干净（`summarizePrompt.ts` / `commandRegistry.ts` / `+page.svelte`）。
- `rewriteFrontmatter` + `insertTldrAtTop` 的 Node 内联手验三条路径：
  1. `summary` 追加到 `---` 块末尾（既有 key 不受影响）；
  2. `> **TL;DR** …` 插在 `---` 后、`# Title` 前，前后各一空行；
  3. 无 frontmatter 的 note 直接 prepend + 原内容尾部保留。
- 命令可见性手验：AI 关掉后三条 summarize 命令在 palette 里不出现；非 markdown / `.mynotes/` 文件打开时也不出现。
- stale-request race：代码审阅已覆盖"discard + 立刻 re-run"场景；真实网络环境端到端测试留到后续用户验证。

---

### 6.33 AI 辅助·`> Suggest tags for current note` 命令（P3-D3.4）

D3.4 是 D3 系列里第一个写回 **YAML list 语义**（而不是 scalar / 文本块）的 AI 命令：把 AI 建议的 tag 候选 + 现有 tag 合并进 `frontmatter.tags`。它与 summarize 的主要差异是交互形状——checkbox merge，不是 text diff——所以引入独立 modal `TagSuggestModal.svelte`，而不复用 `DiffPreviewModal`。

#### 6.33.1 为什么不复用 `DiffPreviewModal`

- **语义不匹配**：`tags: [a, b]` → `tags: [a, b, c, d]` 在行级 LCS 下只会呈现一整行红绿交换，既损失"分项接受"的能力，也看不到每个候选的"已存在 / 复用 / 新建" taxonomy 归属。
- **交互就是合并**：用户的真实意图是"这些候选里我要哪几个、我现有 tags 里我想撤哪几个"。checkbox list 是直接把这个意图打到 UI 上；diff viewer 是事后校对，对多选行为是绕远路。
- **代价可控**：`TagSuggestModal` shell（header / footer / loader / error banner / 键位）与 `DiffPreviewModal` 重合约 40 行；此刻 DRY 成 `ModalShell.svelte` 的形状还不稳定（D3.5 MOC 是第三个写回，若其 shell 诉求也吻合再抽，两个样本太早）。

#### 6.33.2 Prompt 的关键约束

- **Kebab-case + 无 `#` 前缀 + 无标点**：系统 prompt 里硬约束输出形状，`parseSuggestedTags` 只需 csv / JSON / hashtag 三分支容错；更松的 "any format goes" 会让解析层变成语义猜测。
- **注入 vault taxonomy**：`Most-used tags in the vault: a, b, c, …`（top 40，上限定住 prompt 长度）。这是 **soft few-shot**——告诉模型"优先复用"，但允许在现有 tag 明显不覆盖主题时最多新建 2 个。硬约束"只能从 vault 现有 tag 里选"会让 green-field notes 全部无候选。
- **注入 existing tags**：`Existing tags on this note: …`（最多 50 个）。防止模型把"笔记当前已经有的 tag"又当新建议塞回来；也顺手暗示模型"这些方向算 baseline，再围绕它们扩展"。
- **数量上限 3–8**：下限 3 防止"AI 只给 1 个安全 tag"，上限 8 防止"taxonomy 刷屏"。未遵守就靠 modal 的 checkbox 自救（用户可以撤掉多余的）。
- **CJK 白名单**：`normaliseTag` 的 clean regex 包含 `\u4e00-\u9fff`，使 `知识管理` / `图数据库` 这类中文 tag 与 latin tag 并存——中文 knowledge base 场景刚需。

#### 6.33.3 `parseSuggestedTags` 三档容错 & 为什么不按空格切 csv chunk

实际模型输出的常见形状：
1. `"graph-db, knowledge-management, notes"` —— 系统 prompt 实际引导下的主形状；
2. `'["ai","tags","prompt"]'` —— 更严格的小模型偶尔会给 JSON；
3. `"#ai #notes #reading"` —— 有些小模型默认走 hashtag；
4. `"- foo\n- bar baz"` —— 违规的 bullet 输出；
5. `"1234, a really long sentence…"` —— 幻觉噪声。

关键设计决策：**csv 分片不按空格再切**。理由——
- 系统 prompt 已 enforce kebab-case：正常输出里 csv chunk 内的空格应当是 0（多词以 `-` 连）。
- 当 chunk 真的含空格时（模型违规），`normaliseTag` 会把空格折叠为 `-`，得到 `bar-baz` 这种既符合 kebab-case 又语义合理的结果——比拆成 `bar` + `baz` 两个假 tag 更对；
- 而真正的噪声（整段长句）靠 `cleaned.length > 40` + 纯数字过滤兜住，不会 pollute 候选列表。
- **例外是 hashtag 形状**：`"#a #b #c"` 本身靠空格分隔，这里显式多加一条 `\s*#\s*|[,;\n]+` 分支。

#### 6.33.4 `mergeTagsIntoFrontmatter` 的写回策略

- **输入格式三态兼容**（`parseExistingTags`）：
  - flow sequence `tags: [a, b]`；
  - block sequence `tags:\n  - a\n  - b`；
  - 逗号 scalar `tags: a, b`；
- **输出永远是 flow sequence 一行**：`tags: [a, b, c, d]`。
  - 理由：后端 indexer 对三种形式语义等价，round-trip 原格式需要记录"原本是哪种 + 原本缩进"等元数据，工程上不值得；统一形状还顺便让 git diff 可读。
- **无 frontmatter 时 prepend 最小块**：只携带 `tags:` 一个字段，不凭空造 `title` 或 `updated`——这些字段另有自己的命令负责。
- **没有 `rewriteFrontmatter` 复用**：那个工具只能写 scalar（会把 `"[a, b]"` 当字符串双引号包一层）；tag 合并必须自己管 list 语义。两个工具刻意分开。
- **去重 + 顺序稳定**：existing 先保留、然后接 newTags，`normaliseTag` 归一后按首次出现顺序去重。modal 上勾选顺序直接对应文件里最终顺序，便于用户读图。

#### 6.33.5 `TagSuggestModal.svelte` 的 UI 决策

- **三态徽章**：`已存在`（中性）/ `复用`（绿色：vault 里别的笔记有过）/ `新建`（琥珀：AI 首次提出）。这让用户一眼看出 taxonomy drift——是否在制造噪声 tag。
- **existing-first 预勾**：note 当前已有的 tag 全部在列表顶部预勾，用户体验上是"看到现状 → 增减"，而不是"从零勾一遍又确认保留"。取消勾选现有 tag = 删除该 tag，`removedCount` 会计入。
- **候选预勾 + 用户自由撤销**：AI 候选默认勾选（它建议的都想要），用户撤掉不想要的。替代方案"默认不勾"会让 90% 的勾选动作变成 manual labor，不划算。
- **`selected` 用 `$effect` 增量 seed**：新行到达（候选从 loading 切到 loaded）时扩 map，已有的用户勾选不动；这样用户可以"先在 existing 上撤掉几个"的同时等候 AI 候选到达，不会被 reset。
- **计数行 `+addedCount / -removedCount`**：与 `DiffPreviewModal` 的 `+/-` 行视觉一致，降低学习成本。
- **键位镜像**：`Cmd/Ctrl+Enter` = accept / `Esc` = discard（loading 态转 cancel）/ 双击防锁 `accepting` 标志——同 `DiffPreviewModal`，全套写回命令共享同一套键盘肌肉记忆。
- **`.tsm-*` scope**：避免与 `.dpm-*` / 全局 `.ns-*` 撞样式；shadow DOM 不必要，单入口 `TagSuggestModal` 的 scope 已足够防污染。

#### 6.33.6 `runSuggestTagsForCurrentNote` 的状态机

流程与 `runSummarizeCurrentNote` 几乎同构，差异点：
- **并行取 vault taxonomy**：`indexTags()` 在 `fileRead` 之后、`aiComplete` 之前 await。失败 **不** block：`vaultTagNames` 保持空，prompt 里 vault 片段显示 `(none)`，modal 徽章全变 `新建`。这比"先出 error → 用户重试"更适合 vault 刚打开时 indexer 还没写完的 race。
- **`aiComplete` 传 `temperature: 0.2`**：tag curation 是 convergent task——用户多次运行同一个笔记应该得到近似结果，低温比 summarize 的 0.3 更合适。
- **stale-request guard 三段**：await 后校验 `suggestTagsRequestId !== requestId` → 跳过；catch 同校验；`finally` 只在 match 时清 loading 标志。与 summarize 完全同构，`Arc<AtomicBool>` 在后端单独维护 `complete_requests` 表（§6.30.6），跟 chat stream 互不误杀。
- **`suggestTagsRequestId` 故意非 `$state`**：它只是 routing token，声明为 `$state` 会让每次赋值都 invalidate 依赖它的 `$effect` / `$derived`，而这些都不该因"id 变了"刷新。

#### 6.33.7 `applySuggestTags(finalTags)` 的一致性

- modal 给出的 `finalTags` 是**最终意图清单**，不是"增量"——已包含用户想保留的 existing + 勾选的候选 + 排除掉的 existing 撤销。
- `mergeTagsIntoFrontmatter(suggestTagsOriginal, finalTags)` 里的 `existing` 参数会再次解析原 body，与 `finalTags` 求并——这是**刻意冗余**：防止 modal 某条边缘路径漏了某个 existing tag；`mergeTagLists` 的去重使得冗余无副作用。
- 写盘后若当前 open file 是同一路径，走 `fileRead` → `editorContent = fresh; pendingSave = null`——同 §6.32.6 的理由，watcher 只重建 SQLite 索引。

#### 6.33.8 与其它 AI 命令的共存语义

- 面板命令 **一条**：`suggest-tags`。不分档（写回目标唯一）、不开子菜单（checkbox modal 自己是 picker）。这与 summarize 的"三档独立命令"对比：summarize 的三个 target 之间行为差异大（写 frontmatter / 插 TL;DR / clipboard），而 tags 只有一个 target。
- 共享 `aiEnabled` gate：AI 关掉 → 面板中不出现，与 summarize 行为一致。
- 后端 `complete_requests` 注册表已按 request_id 分离，summarize / suggest-tags / chat stream 三条并发互不 cancel 串台。

#### 6.33.9 刻意不做的事（留给后续）

- **AI 候选的置信度 / 排序分数**：目前只按模型输出顺序展示，没有 score；小模型可能给出无关 tag，需要靠用户筛。后续考虑让 prompt 强制按相关性降序 + 输出短 rationale。
- **tag 重命名 / 编辑**：modal 只有 "勾 / 不勾" 两态，不能改 `graph-db → graphdb`。系统级 tag rename 工具留给 Phase 4 tag 管理 UI 做。
- **inline `#tag` 合并**：D3.4 不扫正文里的 `#hashtag` inline tag（indexer 会识别 + 聚合到 tag view，但 `parseExistingTags` 只读 frontmatter）。未来若要，需要新命令"promote inline tags to frontmatter"而不是在 suggest 里混做。
- **"保留 tag" 黑名单**：某些 vault 有 `_draft`、`status/active` 之类不希望 AI 动的特殊 tag；目前没有 UI 标记，靠模型自觉。待 tag 管理 UI 落地时再补。
- **chunked note summarization / suggest**：超长笔记（>100k chars）目前直接整段塞 prompt；后续可能要按 H2 分段+映射。属于 D3 之后的 polish。
- **clipboard / preview-only 档**：tag 场景没有需求（建议本质上就是要落盘），刻意不做。

#### 6.33.10 测试覆盖

- `pnpm check` **0 errors / 0 warnings**；`pnpm build` success；`ReadLints` 干净（`suggestTagsPrompt.ts` / `TagSuggestModal.svelte` / `commandRegistry.ts` / `+page.svelte`）。
- `parseSuggestedTags` Node 内联 7 条 case 验证（csv / json / bullets / hashtags / CJK / garbage / multi-word-bullet）全部收敛到期望列表。
- `mergeTagsIntoFrontmatter` 五路径 Node 内联手验：flow / block（源块转 flow） / 无 `tags:` 键追加 / 无 frontmatter 整体 prepend / scalar 逗号形态规范化——输出均符合预期。
- UX 推演：候选 `graph-db, knowledge-management, notes` + 现有 `[ai, notes]` → modal 列 4 行、用户撤 `notes` → 写入 `tags: [ai, graph-db, knowledge-management]`，counts `+2 / -1`。

---

### 6.34 AI 辅助·`> Draft MOC from tag (AI)` 命令 · D3 收官（P3-D3.5）

D3.5 是 D3 系列的收官刀，把"AI + 写回"的管线扫过 MOC 场景：让 AI 对 `#<tag>` 下的笔记按主题分组生成 H2 小节 + `[[title]]` bullets，走 `DiffPreviewModal` 预览后通过既有 `buildMocFromTag` 管线落盘。重点是**复用而非另起**——UI 共用 mocBuilder picker、downstream 共用非 AI 版的 `buildMocFromTag`，整条路径只加一个"AI entries 生成 + 清洗"的侧支。

#### 6.34.1 为什么共用 mocBuilder picker 而不新开一个

- **三段输入重合**：tag（`activeTag`）+ title + picked notes 在 AI 和非 AI 路径完全一致；把输入阶段 duplicate 一份 modal 会让"切到 AI 后改 title 不同步"成为新 bug 源头。
- **命令面板仍保留两条入口**：`build-moc-from-tag`（无 AI gate）和 `draft-moc-from-tag`（`aiEnabled` gate）。fuzzy search 能直达任一路径，而两条都 `run: runBuildMocFromTag()` 打开同一 picker。用户若已在 picker 里也能看到"用 AI 草拟…"按钮——两个入口同一终点。
- **Modal 底部 fork 按钮**：`{#if aiEnabled}<button onclick={confirmBuildMocWithAi}>用 AI 草拟…</button>{/if}`。位置在"取消"与"创建 MOC"之间，次级按钮样式（不 `.primary`）以不抢 non-AI 路径焦点——非 AI 才是"默认"行为，AI 是可选增强。

#### 6.34.2 `buildMocFromTag` 的签名扩展哲学

- 加可选 `entriesMarkdown?: string`：非空时覆盖 `lines.join('\n')` 的扁平 rendering，其他流程（template materialise / sentinel 注入 / `moc_source_tag` 盖章 / 面板刷新 / 打开文件）**一行不动**。
- `insertedCount` 仍用 `params.noteRefs.length`——AI 漏题不改这个数值，toast 另走 `droppedCount` 分支提示；保持 `insertedCount` 恒等于"用户选了几条笔记"的定义，避免调用方被"AI 漏几条 = 实际注入几条"搞晕。
- 对所有现有调用零回归：调用处不传 `entriesMarkdown` → 三元分支 `params.entriesMarkdown?.trim() ? ... : lines.join('\n')` 走 else，产物与扩展前字节一致。

#### 6.34.3 Prompt 的硬约束：反"模型丢题"

LLM 在"从输入列表中分组"任务下最常见的失败模式是**悄悄丢题**（~10-20% 的 title 消失到"杂项"或直接不出现）。systemPrompt 里两条硬约束：

1. **"Every title MUST come verbatim from the provided list. Do NOT invent titles, do NOT rephrase them."** —— 反"幻觉新标题"。
2. **"Every title from the provided list MUST appear exactly once across all sections (no duplicates, no omissions)."** —— 反"丢题 / 重复归类"。

即使 prompt 约束了，`sanitizeDraftMoc` 仍要 allowlist 校验：hallucinated `[[title]]` 被降级为 `- <title>  <!-- AI 生成，非选中笔记 -->` 而不是删掉——不删是因为用户可能想看到"AI 试图归的类"，注释化是为了不污染 vault graph（wiki-link 解析器只识别 `[[…]]` 包裹的内容）。

另两条形状约束：
- **2–6 个主题**：下限反"一个主题兜所有"、上限反"一笔一节"。
- **只输出 `##` 小节 + `- [[title]]` bullets，无 prose 段落 / blockquote / code fence / frontmatter / H1**：这让输出可以 drop-in 到 `injectMocEntries` 的 sentinel 位置而不污染模板其他小节（如"## 参考"）。

#### 6.34.4 `sanitizeDraftMoc` 的四条清洗动作

1. **剥 code fence**：有些模型违规给 ```` ```markdown ``` ```` 包裹，regex `^```[a-zA-Z]*\n…\n```$` 一次性剥掉。
2. **丢 preamble**：模型偶尔以"Here's the grouping:" 开头。找到第一个 `## ` 的位置、把之前的内容切掉。
3. **`[[title]]` allowlist 校验**：`bullet.match(/^(\s*-\s*)\[\[([^\]|]+)(?:\|[^\]]*)?\]\]\s*$/)` 抓 bullet、title 若在 allowlist 里保留 `[[title]]`、不在则降级为 `- <title>  <!-- AI 生成，非选中笔记 -->`。支持 `[[title|alias]]` 语法的 alias 部分被丢弃（canonical title 已足够）。
4. **合并连续空行 + 200 行上限**：blankRun > 1 时吞掉多余空行，output 只保留 200 行内的——既保 diff 干净、也兜住 prompt 爆量时的回填。

返回 `{ markdown, sectionCount, bulletCount, linkedTitles }` 四字段：前三个供 toast / 日志用（量化 AI 是否跑了指定形状）、`linkedTitles` 让调用方算 `droppedCount = picked.length - new Set(linkedTitles).size` 并 surface 给用户。

#### 6.34.5 Diff 的粒度选择：entries block，而不是整份 MOC body

- `DiffPreviewModal.original` = `buildFlatEntriesMarkdown(picked)`（扁平 `- [[title]]` 列表），即**非 AI 路径会落盘的 entries block**。
- `DiffPreviewModal.proposed` = `sanitizeDraftMoc(reply, allowed).markdown`（AI 分组后的 `##` + bullets 块）。
- 两者都是 **entries block**（没有模板壳 / frontmatter / 其他小节），因为 diff 的兴趣点本就是"怎么分组"。如果把整份 MOC body diff，模板的 H1 / 其他 boilerplate 会占据大部分行数、稀释 AI 带来的真正变化。

#### 6.34.6 状态机与 race guard

流程与 summarize / suggest-tags 同构：
- `draftMoc*` 全套 `$state`（`Open / Loading / Error / Reply / Tag / Title / Picked / Flat`）+ 非响应式 `draftMocRequestId`。
- `confirmBuildMocWithAi()` 里**先 snapshot 再关 picker**：把 `{tag, title, picked}` 拷到 `draftMoc*`、清 mocBuilder 的 `list/selected`、再切 `draftMocOpen`。避免"用户按 AI 按钮后立刻再点取消" 使 in-flight 丢失 context。
- stale-request guard 三段：`await aiComplete` 返回后 `draftMocRequestId !== requestId` → skip；catch 同校；`finally` 只在 match 时清 loading。`aiCompleteCancel` 先抢 `draftMocRequestId = null` 再发后端 cancel，与 D3.3 / D3.4 逻辑字节对齐。

#### 6.34.7 temperature = 0.4 的权衡

- summarize 用 0.3（低温、输出相对稳定）、suggest-tags 用 0.2（convergent task、几乎零 creativity）、draft-moc 用 0.4——theme naming 天然需要少量 creativity（"方法论 / 工程 / 评估" 之类命名），但温度再高会诱发 title 幻觉（`[[编造出来的标题]]`）。sanitiser 的 allowlist 是最后防线，但"防"比"治"便宜。

#### 6.34.8 Toast 分档

写回完成后三种可能：
- `strategy === 'none'`（模板既没 sentinel 也没 legacy 锚点）→ error 红色提示"文件已建、entries 未注入，需要手动贴或 reseed 模板"。
- `droppedCount > 0` → error 红色提示"AI 漏 N 条已标注为注释"。
- 正常 → success 绿色"AI 分组 · N 条"。

没有 `NoticeKind = 'warning'`（当前 notice 系统只有 `info | success | error`），故第二条走 error + 7 秒 TTL 代替；长 TTL 让用户有时间读"漏题量化"这条较长消息。

#### 6.34.9 刻意不做的事（留给 Phase 3-D4 Polish 或 Phase 4）

- **rebuild from tag (AI)**：`moc_source_tag` frontmatter 已经写入；可以在 palette 里加 `Rebuild MOC from tag (AI)`，`when: currentFile 是 MOC 且 frontmatter.moc_source_tag 有值`。未落地。
- **section-level 部分接受**：`DiffPreviewModal` 当前 all-or-nothing。若要"保留 AI 的 #方法论，但保留 baseline 其它部分"，需要在 modal 上加 section checkbox（接近 `TagSuggestModal` 的多选）。需求未验证、先 YAGNI。
- **AI 结果缓存**：每次重跑都发全文到模型；`notes` 集合稳定时重复付费。可以写 `.mynotes/ai/drafts.json` 缓存，但 MOC 重建往往伴随 tag 下 note 集合变化，缓存命中率可能不高。
- **AI 生成描述段落**：systemPrompt 明令禁止；需要改 sentinel 设计才能放开（比如 `<!-- moc:section-summary -->`）。P3-D4 再看。
- **note body / summary 送 prompt**：目前只送 title。如果两 note title 近似，模型只凭 title 分不清语义。D3.3 落地了 `frontmatter.summary` 写入，下一步自然是 prompt 里 for each note 附上 summary——留给 Polish 阶段。
- **题目非 title 的 bullet**：AI 偶尔会在 bullet 里塞非 `[[…]]` 文字（如 `- （主题说明）`）。当前 sanitiser 让非 bullet 行原样通过、非 allowlist bullet 降级为注释——边界情况目前不多，未来若发现模式化的污染再补 drop-line 规则。

#### 6.34.10 测试覆盖 & D3 收官状态

- `pnpm check` **0 errors / 0 warnings**；`pnpm build` success；`ReadLints` 干净（`draftMocPrompt.ts` / `commands.ts` / `commandRegistry.ts` / `+page.svelte`）。中间一次 `NoticeKind` 把 `'warning'` 当合法 kind 的编译错误 → 改走 `'error'` + 7s TTL 后通过。
- `sanitizeDraftMoc` Node 内联 4 路径（normal / fenced / preamble / hallucination）全部产出预期结构化输出（section / bullet / linkedTitles 计数精确）。
- `buildMocFromTag` 扩展签名无回归：所有既有调用不传 `entriesMarkdown` → `params.entriesMarkdown?.trim() ? ... : lines.join('\n')` 走 else 分支、扁平 rendering 字节与扩展前一致。
- **D3 收官**：D3.1（`ai_complete` IPC + cancel）→ D3.2（`DiffPreviewModal`）→ D3.3（summarize 三档）→ D3.4（suggest-tags checkbox merge）→ D3.5（draft-moc AI 分组）五刀贯通；`ai_complete` 通道被三条写回命令共享、`complete_requests` 注册表按 request-id 分离彼此不串台；`DiffPreviewModal` 被 summarize 与 draft-moc 两条复用、`TagSuggestModal` 处理 checkbox 语义独立存在；命令面板三条 `summarize-*` + `suggest-tags` + `draft-moc-from-tag` 共 5 条 AI 写回命令全部以 `aiEnabled` 为 gate。下一步移交 **P3-D4 Polish** 或 Phase 4 新起点、等用户选向。

---

### 6.35 AI 写回流的 failure / cancel / retry UX hardening（P3-D4.1）

P3-D4 的第一刀不再开新命令，而是把 D3 已落地的三条 AI 写回流补到“失败时也不慌”的状态：`Summarize current note`、`Suggest tags for current note`、`Draft MOC from tag (AI)` 都已经能用，但 D3 收官时仍有三类体验债没收口：

1. **失败口径不统一**：有的流显示 raw provider string，有的走 `formatAiFailureText()`，有的把 `"cancelled before any content arrived"` 原样透给用户。
2. **取消路径太硬**：loading 态一按 `Esc` / cancel 就直接关 modal，若 provider 已经吐出部分 reply，会把可用结果一起丢掉；若 cancel IPC 自己失败，用户只看到弹窗消失，不知道发生了什么。
3. **重试成本偏高**：用户遇到余额不足 / 网络抖动 / 半截内容后，得自己重新走命令面板或 picker 才能再来一次。

因此 D4.1 的目标是：**不动后端协议、不改写回语义，只把 modal 生命周期和失败态收口成一套稳定的 UX。**

#### 6.35.1 为什么这刀放在 D3 之后而不是更早

- D3.1 ~ D3.5 解决的是“先把 AI 写回通道跑通”，其验收标准是**成功路径**可闭环：发 prompt、拿 reply、给预览、用户确认后写盘。
- D4.1 解决的是“当 provider 不稳定、用户反悔、reply 半途被截断时，产品是否仍可预测”。这类 polish 需要先有三条真实命令落地，才能总结出哪部分值得抽 shared UX。
- 若在 D3.2 时就把 retry / advisory / canceling 态一起做，会把 `DiffPreviewModal` 过早设计成一个“什么都想包”的 super modal；等 D3.4 的 checkbox merge 与 D3.5 的 picker fork 进来后，真实共性才更清楚。

#### 6.35.2 Shared shell 扩展：`DiffPreviewModal` / `TagSuggestModal` 先对齐外壳，再谈逻辑

两套 modal 在 D3 结束时已经有接近的 shell：header / loading / error / footer / 快捷键。D4.1 先扩它们的共有 props，而不是在 `+page.svelte` 分别拼三套散装 UI：

- `statusNote?: string`
  用于展示 advisory note，例如“已取消生成；以下是取消前拿到的部分结果，可直接采用或重试”。空字符串时不渲染。
- `showRetry?: boolean` + `onRetry?: () => void | Promise<void>`
  让 error 态与 partial-result 态都能在 footer 直接出现 Retry，而不是把“重试”留给命令面板。
- `cancelBusy?: boolean`
  loading 态下用户点击取消后，按钮文案可切成“正在取消…”，正文 loadingText 也能同步改成“正在取消生成…”，避免用户误以为按钮没反应。
- `loadingText?` / `cancelLabel?` / `retryLabel?`
  让三条流在文案层能保持一致，但仍留少量命令级定制空间。

这里的原则是：**只共享 modal 壳的 affordance，不共享每条命令自己的状态机**。shell props 一致，summarize / suggest-tags / draft-MOC 仍各自掌握 request-id、源路径、reply 清洗与 accept 行为。

#### 6.35.3 失败文案归一化：`normalizeCompleteFailure()`

`formatAiFailureText()` 在 D2a / D2b 已经存在，但 D3 的三条写回流并没有完全一致地使用它。D4.1 抽 `normalizeCompleteFailure(failure, fallbackMessage)`，把 `CompleteFailure` 统一变成用户可读文本：

- provider / transport / auth / invalid_request / rate_limit 继续走 `formatAiFailureText()`；
- `"cancelled before any content arrived"` 单独翻译为“已取消生成，尚未产出可用内容”；
- 空 failure 或 reply 缺失时回落到调用方给的 `fallbackMessage`。

这样做的意义不是“少写几行判断”，而是保证三条写回流在同一 provider 错误下给出**同一口径**。用户不需要记忆“为什么 summarize 说网络错误，而 suggest-tags 直接把 raw JSON 打出来”。

#### 6.35.4 取消语义改成两阶段：先进入 `cancelBusy`，再由真实结果决定落点

D3 阶段的取消逻辑基本是：

1. 抢走 request-id
2. 调 `aiCompleteCancel(rid)`
3. 立即 `closeModal()`

这个行为简单，但会吞掉两类信息：

- provider 已经在 cancel 前吐出了一部分可用 reply；
- cancel IPC 本身失败，说明请求可能还在后台继续跑。

D4.1 把它改成：

1. `if (!rid || canceling) return;`
2. 先 `canceling = true`
3. 调 `aiCompleteCancel(rid)`
4. 不立即关窗，等 `aiComplete()` resolve/reject 的真实结果

随后分三种结果：

- **取消前完全没拿到内容**：进入 error/notice 态，文案是“已取消生成，尚未产出可用内容”
- **取消前已拿到部分内容**：保留 partial reply，modal 顶部挂 advisory note，用户可 Accept 也可 Retry
- **cancel IPC 自己失败**：取消中状态撤销，modal 保持打开并展示“取消失败：…”；不静默 close

这套语义的核心是：**取消不等于丢弃一切，它只是告诉系统“尽快停止继续生成”；最终保留什么，由真实已经拿到的 reply 决定。**

#### 6.35.5 Retry 语义：从“重新走入口”改成“modal 内就地重跑”

Retry 的实现刻意很轻，不引入结果缓存或任务中心：

- summarize：`retrySummarize()` 关闭当前 modal，然后直接重跑 `runSummarizeCurrentNote(summarizeTarget)`
- suggest-tags：`retrySuggestTags()` 关闭当前 modal，再跑 `runSuggestTagsForCurrentNote()`
- draft-MOC：抽 `startDraftMocAi(tag, title, picked)`，首跑和 Retry 都走它；这样 retry 不需要把 mocBuilder picker 再开一遍，也不会丢失用户刚才选好的 note 子集

这里有一个明确边界：**Retry 不是“从半截结果续写”**。每次都是新的 `request_id`、新的 `aiComplete()` 调用，旧结果只作为用户可见参考存在，不参与合并。

#### 6.35.6 Advisory note 的产品语义：部分结果不是“错误副产品”，而是候选内容

当 provider 在取消前已吐出可用 reply，D4.1 不把它视为“脏数据”，而是提升成一种明确状态：

- summarize：说明这是取消前生成的部分摘要，可直接采用，也可重试
- suggest-tags：说明这些 tag 候选来自取消前已完成的部分生成，可继续勾选，也可重试
- draft-MOC：说明当前 diff 基于取消前已产出的分组草案，可直接创建，也可重试

这比“直接当作成功”或“直接当作失败”都更准确：它不是完整成功，但也不该被静默扔掉。用户是否采用，应由用户在预览里决定。

#### 6.35.7 刻意不做

- **不改 `ai_complete` / `ai_complete_cancel` 协议**：D4.1 纯属前端 UX 硬化，不新增后端状态机与 schema。
- **不做 countdown / auto-retry**：虽然 `retry_after_secs` 已有，但目前只归一化到文案，不在 modal 里跑倒计时、不做自动重试。
- **不做 ChatPanel 同构改造**：右栏聊天流仍保持 D2b 的失败/取消口径；D4.1 只覆盖三条写回命令。
- **不做结果缓存**：partial result 只存在当前 modal 生命周期里；关掉即丢，不落盘、不进后台任务中心。

#### 6.35.8 验证边界

- `pnpm check` → **0 errors / 0 warnings**
- `pnpm tauri build --bundles app` → success，打包产物正常生成
- 桌面插件手测确认：
  - palette → AI 命令 → modal 打开路径未回归
  - modal 打开后焦点在 dialog，自带键盘快捷键可直接使用
  - summarize / suggest-tags / draft-MOC 的基础成功路径仍可走通

剩余未完全打透的是**慢路径手测**：当前 vault 里的真实笔记很短，provider reply 返回太快，不容易稳定命中“正在生成时取消”分支；这被明确留作 D4.1 之后的验证补项，而不是假装已经全覆盖。

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

| 周     | 目标                                                                                                                                                             |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Week 1 | Tauri + SvelteKit 脚手架；vault 打开/初始化（含 LYT + projects 目录）；文件树和基础 CM6 编辑器；读/写文件走通                                                    |
| Week 2 | 模板引擎 + Daily/Weekly 周期笔记（`Cmd+D` / `Cmd+Shift+W` 打得开）；**Quick Capture** (`Cmd+Shift+N`) 和 **Daily Record 追加** (`Cmd+Shift+D`)                   |
| Week 3 | 索引（SQLite + FTS5）+ 反向链接面板 + Tag 聚合页 + wiki link CM6 扩展                                                                                            |
| Week 4 | 命令面板 + Inbox Review 视图 + Promote 流程 + MOC 创建命令 + Home 页 + 打磨                                                                                      |
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

**状态**：已完成。截止 Phase 2 收口时，图谱、附件/图片、Rename With Refs、MOC 辅助建议、设置/主题、导出均已落地，桌面端主工作流闭环成立。

- [x] 图谱视图；
- [x] 图片/附件粘贴与管理；
- [x] 链接重写（笔记重命名时更新所有引用）；
- [x] MOC 辅助建议（基于 tag 自动列出候选）；
- [x] 设置界面（主题、autosave、模板重置；快捷键自定义延后，中文分词受 FTS5 建表时固定，见 §6.14）；
- [x] 主题切换（状态栏齿轮 + Settings radio + 命令面板三条）；
- [x] 导出（整个 vault 导 zip、单篇导 md、Print→Save as PDF；见 §6.15）。

### Phase 3 — 扩展（按需）

**状态**：已启动且主线已进入 `P3-D`。`P3-A1 ~ P3-A7` + `P3-A2` Must-fix sweep 已全部收口；AI 线已落地 `P3-D1` + `P3-D2a.1 ~ P3-D2a.6`（整个 Embedding 底座完成）+ `P3-D2b.1`（会话数据层），下一刀进入 `P3-D2b.2`（Provider chat trait + SSE 流式）。Phase 3 不再以"补齐 Phase 2 功能"为目标，而是以"扩平台 + 提升可配置性 + 引入智能能力"为目标。

**启动原则**：

1. **先稳桌面内核，再开新端**。Phase 2 虽然功能完成，但仍留有若干 desktop-hardening 项：快捷键自定义、图谱 a11y / 更深的性能压测、导出可定制性、TagView 的多条件筛选与排序等。Phase 3 的第一批工作应优先消化这些"用户每天会撞到"的摩擦。
2. **新端优先做"窄而深"的最小版本**。移动端先做 Quick Capture + 浏览，不追求完整编辑；Web 先做只读浏览，不追求全量 Tauri 能力搬运。
3. **AI 能力建立在稳定索引层之上**。AI 模块不是独立产品，而是对现有 vault / SQLite 派生索引的增量利用，应复用现有 Markdown SSOT 与 links/tags/search 结构。

**建议分 4 条工作线推进**：

- **P3-A · Desktop Hardening / Config**：快捷键配置、更多设置项、Tag 交/并集筛选、排序选项、图谱 a11y 与性能压测、打印/导出主题化、重命名 dry-run / 更强确认、命令反馈通道清理等。
  当前已落地 P3-A1 ~ P3-A7 共七刀 + 一次 P3-A2 补坑 sweep：
  1. **P3-A1**（app-level config + 快捷键自定义）：config 持久化到 `app-config.json`，不再只靠 `localStorage`；Settings 里可直接录快捷键组合并做冲突检测；`installShortcuts` 从 keymap 驱动而不是 `if/else` 硬编码。同属 P3-A1 的 TagView 支持附加 tag 过滤、交/并集切换与结果排序。
  2. **P3-A3**（Graph hardening）：GraphView 补上键盘焦点 roving、屏阅镜像只读语义、本地空态提示、大图 `forceCollide` preset + `data-theme` 自动重绘 hook。
  3. **P3-A4**（Rename hardening）：文件与目录 rename 都走两阶段 modal（dry-run 预览 → 影响列表 → 二次确认）；后端 `rename_preview` 命令一次返回所有受影响 referrer 及变更摘要。
  4. **P3-A5**（命令反馈 notice stack）：把 graph load / extract / export / rename / project commands / unused attachments 等反馈从 `saveStatus / saveError` 通道剥离，页面内独立 notice stack 自动消失 / 手动关闭，状态栏只保留 autosave 语义。
  5. **P3-A6**（Sidebar drop 导入）：侧栏吃 `drop` 事件，按目录 row / 文件 row 取父 / 空白区落 inbox 三分支分流；Rust 侧 `file_import` 硬约束绝对路径 / 非目录 / 拒 vault-内部源；`-N` 冲突递增；notice 聚合四档文案。
  6. **P3-A7**（打印 HTML 主题化）：`note_render_print_html` 签名加 `theme?`；后端 `PrintTheme { Light, Dark, System }` 三分支 HTML + 显式 `color-scheme`；`@media print` 强制走亮色；调色板退回 hex 以保证跨 PDF viewer 一致性。
  7. **P3-A2 补坑 sweep**（2026-04-21 日加入）：MOC 模板 stub 解耦（sentinel + legacy fallback + `strategy` 字段）、`schedulePanelRefresh(200)` 消除 MOC/extract 之后的 indexer race、`preprocess_wikilinks` 让打印输出真正的 `<a>` 锚点链接、`EMBED_LINE_RE` 支持 Windows drive-letter 绝对路径。
     P3-A 剩余摩擦已收敛到 Phase 4 范畴（前端 vitest harness、大图 5k+ 节点 benchmark、跨平台 Windows/Linux 真机冒烟），不再强行塞进 P3-A8+。
- **P3-B · Web（只读浏览）**：在不依赖本地文件系统写入的前提下，把 vault 索引结果和 Markdown 渲染输出成只读浏览器体验，优先支持 Home / Note / Tag / Graph 的浏览。
- **P3-C · Mobile（Quick Capture + Browse）**：面向 iOS / Android 的窄功能客户端，优先满足随手捕获、查看今日/本周、轻量浏览知识库，不承诺完整编辑器体验。
- **P3-D · AI Module**：基于 vault 做 RAG / related-notes / MOC draft / project summarization，接 OpenAI/Claude API，但保持"可关闭、可审计、不污染 Markdown 真相源"。

**建议的进入顺序**（2026-04-21 修订：原 A → B → C → D 调整为 **A → D → C → （B 延后或不做）**）：

1. **`P3-A` Desktop Hardening**（已完成 A1 ~ A7 + A2 sweep）：自用日增量最直接，迭代快，风险最小。这是已经落地的基线。
2. **`P3-D1` AI Module PoC**：索引层（SQLite + links + tags + FTS5）已稳，RAG / related-notes / MOC draft 直接建在上面，**无需新平台工程**；在桌面内加开关试点，失败回退成本低；价值上限最高。
3. **`P3-C1` Mobile Quick Capture PoC**：真实痛点（"灵感在外面产生要能落下来"），但 Tauri Mobile 生态不成熟，需真机 + Xcode/Android Studio 调试。放 D 之后 capture 可直接复用 AI 的自动归类/摘要，避免重复基础设施。
4. **`P3-B` Web 只读（可选，或不做）**：§1.3 明确"多人协作、全文移动端原生编辑"是非目标——Web 主要服务"分享给别人看"的分发语义，本项目不追求。除非后续出现"vault 对外发布主页"的需求，否则这条可永久延后。

**和 §10 初版的偏离**：B 与 D 对调，并把 B 降级成可选；A 与 C 的相对顺序保持。重新决策的理由见 `delivery_log.md` 2026-04-21 · Phase 3-A2 条目，以及 `delivery_log.md` 尾部 changelog 表的 2.14 行。

**本阶段完成判定**：

- `P3-A` desktop-hardening 已达"可长期使用"状态（A1 ~ A7 + A2 sweep 落地）；
- `P3-D` AI PoC 跑通一个最小闭环（RAG 或 related-notes 任一），即视为完成 Phase 3 的平台扩展方向验证；
- `P3-C` Mobile 与 `P3-B` Web 不再是硬性完成条件。

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

| 风险                             | 影响             | 缓解                                                |
| -------------------------------- | ---------------- | --------------------------------------------------- |
| Tauri 2 移动端 bug 多            | Phase 2 进度风险 | Phase 1 不碰；Phase 2 先做 PoC 再决定投入           |
| notify-rs 在 iCloud Drive 漏事件 | 索引不同步       | 每 5 分钟全量校验兜底                               |
| CM6 学习曲线                     | Week 1 进度慢    | 先用最基础配置跑通，扩展慢慢加                      |
| Rust 入门门槛                    | 进度慢           | 后端保持 thin，复杂逻辑放前端                       |
| iCloud/Syncthing 冲突            | 数据风险         | 原子写 + 外部修改检测                               |
| SQLite FTS5 中文分词差           | 搜索效果弱       | Phase 1 用 unicode61；Phase 2 考虑 trigram 或 jieba |
| 用户手写坏 frontmatter           | 解析失败         | 解析失败降级：文件原样保留，不写索引，UI 标红       |
| 新用户不懂 LYT 工作流            | 上手困难         | 首次启动给 3 步 tutorial 解释 Inbox → Note → MOC    |

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
created: '{{now}}'
updated: '{{now}}'
---

{{content}}
```

**`templates/note.md`**：

```markdown
---
title: '{{title}}'
type: note
status: draft
created: '{{now}}'
updated: '{{now}}'
tags: []
aliases: []
---

# {{title}}
```

**`templates/moc.md`**：

```markdown
---
title: '{{title}} · MOC'
type: moc
created: '{{now}}'
updated: '{{now}}'
tags: [moc]
moc_scope: '{{title}}'
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
title: '{{date:YYYY年MM月DD日}} ({{date:ddd}})'
type: daily
period: '{{date:YYYY-MM-DD}}'
created: '{{now}}'
updated: '{{now}}'
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
title: '{{week}} 周记'
type: weekly
period: '{{week}}'
created: '{{now}}'
updated: '{{now}}'
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
title: '{{title}}'
type: project
project_status: active
project_started: '{{date:YYYY-MM-DD}}'
project_target: '{{project_target}}'
created: '{{now}}'
updated: '{{now}}'
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
title: '{{title}}'
type: project-note
created: '{{now}}'
updated: '{{now}}'
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

历史 changelog（日期 / 版本 / 变更 三列表）已整体迁移到 [`delivery_log.md` · 「版本变更总览（Changelog，历史索引）」](./delivery_log.md#版本变更总览changelog历史索引) ——design_V2 只记"为什么这样做"（架构、原则、章节化决策），流水账统一住 `delivery_log.md`。

新任务交付时，除了把 Scope / How to verify / Known gaps 写到 `delivery_log.md` 顶部，还要在该文件尾部的 changelog 表里追加一行版本号——而不是再回来动本章节。

---

## 17. 交付清单

逐任务的 **Scope / How to verify / Known gaps** 三段式记录已迁到仓库根目录的 **[`delivery_log.md`](./delivery_log.md)** —— 本章节只保留索引指针，避免 design_V2.md 过度膨胀。

- 新任务启动前：扫读 `delivery_log.md` 顶部最近 2–3 条交付记录（倒序排列）。
- 交付时：把本次任务的 Scope / How to verify / Known gaps 写到 `delivery_log.md` 顶部；同时在 `delivery_log.md` 尾部「版本变更总览（Changelog，历史索引）」表里追加一行版本号。
- 若某次交付里包含对架构决策的修改：**先改 design_V2 对应章节（§5 / §6 / §10），再回来写 delivery_log**。
- 交付规范（三段式结构、倒序、不复述全局架构等）仍以 §0.1 为准。
