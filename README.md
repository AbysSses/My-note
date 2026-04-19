# MyNotes

基于 LYT (Linking Your Thinking) 工作流、纯 Markdown 真相源、Tauri 2 + SvelteKit 5 的个人跨平台知识库应用。

> 详细设计见 `design_V2.md`；演进要点见 `design.md`（v0.3）。

## 当前状态

**Phase 1 · Week 3 · 索引层 + 双向链接 + Tags + 补全**

已落地能力：

- **Vault 生命周期**：`Open Vault...` 自动检测/初始化 LYT 结构（`0-inbox/ 1-notes/ 2-moc/ 3-journal/ 4-projects/ attachments/ templates/ .mynotes/`），默认模板嵌入二进制，首次写入不会覆盖用户编辑。
- **编辑器**：CodeMirror 6，含 Live Preview 装饰、`[[wiki-link]]` Cmd/Ctrl+Click 打开或创建、自动保存（500ms 防抖）。
- **Home & 命令条**：Today / Week / Capture / Daily Record 四个快捷命令，键位 `⌘D / ⌘⇧W / ⌘⇧N / ⌘⇧D`。
- **SQLite 索引**（§5.3 / §8.2）：每个 vault 一个 `{app-support}/index/{hash}.sqlite`，WAL + FTS5；schema 或 vault 路径漂移会触发自动 rebuild。索引是纯派生数据，删除即全量重建。
- **实时索引**：`vault_open / vault_init` 做一次 full scan（mtime 差分），之后 `notify-rs` 文件监听 + 200ms 防抖增量更新 `notes / tags / links / tasks / notes_fts`。
- **右侧链接面板**：当前笔记的反向链接 / 链出 / 未解析 [[target]]，一键跳转。
- **Tags 侧栏与聚合页**：侧栏显示所有标签及笔记数；点击进入 tag 聚合页列出所有携带该标签的笔记。
- **`[[` 自动补全**：CodeMirror CompletionSource 基于索引 `index_all_notes`，stem 冲突时回退到全路径，命中现有 `]]` 时不重复闭合。
- **FTS5 全文搜索 IPC**：`index_search` 已开放（`snippet()` 返回 `<mark>` 高亮），前端 UI 尚未接线。

**待实现**：命令面板（⌘P）、全局搜索 UI、Promote inbox → notes 流程、MOC 聚合视图、Project 模块、插入图片/附件。按 `design_V2.md` §10 路线图推进。

## 环境要求

- Node ≥ 20（推荐 22）
- pnpm ≥ 9（`npm i -g pnpm`）
- Rust ≥ 1.75（`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`）
- macOS：Xcode Command Line Tools（`xcode-select --install`）

## 首次上手

```bash
cd ~/Documents/my-notes
pnpm install              # 安装前端依赖
pnpm tauri:dev            # 首次启动会 cargo build src-tauri，耗时 ~3 分钟
```

首次 `cargo build` 比较慢（会编译 `rusqlite`（bundled SQLite）、`notify`、`tauri` 等原生依赖）；增量编译后每次变更秒级。

## 快捷键

| 键位   | 功能                                   |
| ------ | -------------------------------------- |
| ⌘D     | 打开/创建今日 daily note               |
| ⌘⇧W    | 打开/创建本周 weekly note              |
| ⌘⇧N    | Quick Capture（丢进 `0-inbox/`）       |
| ⌘⇧D    | Daily Record（向今日笔记追加时间戳行） |
| ⌘/Ctrl + Click `[[link]]` | 打开或创建目标笔记 |
| `[[`   | 在编辑器内触发笔记名自动补全           |

## 目录速览

```
my-notes/
├─ src/                    # SvelteKit 5 前端
│  ├─ routes/+page.svelte  # 主 UI：三栏布局（tree / editor / panel）
│  └─ lib/
│     ├─ ipc/              # Tauri invoke 的薄封装（vault / file / index）
│     ├─ editor/           # CodeMirror 6：livepreview + wikicomplete
│     ├─ panel/            # 右侧链接面板（backlinks / outgoing / unresolved）
│     ├─ tags/             # Tags 侧栏 + Tag 聚合页
│     ├─ commands.ts       # Today/Week/Capture/Record 行为
│     ├─ template.ts       # 模板填充 & 日期工具
│     └─ state/            # Svelte 5 runes 全局状态
├─ src-tauri/              # Rust 后端
│  ├─ src/
│  │  ├─ commands/         # IPC：vault / file / index（总 15 个命令）
│  │  ├─ services/         # config / scanner（full + incremental）/ watcher
│  │  ├─ db/               # SQLite 层
│  │  │  ├─ schema.sql     # notes / tags / links / tasks / notes_fts / schema_meta
│  │  │  ├─ mod.rs         # per-vault DB 路径、pragma、schema_version rebuild
│  │  │  └─ indexer.rs     # frontmatter / #tag / [[link]] / task 解析 + upsert
│  │  └─ error.rs
│  └─ templates/           # 打包进二进制的 .md 模板
├─ design.md               # 设计文档（初版）
├─ design_V2.md            # 当前设计（§ 路线图）
└─ package.json
```

## 架构要点

- **纯 Markdown 真相源**：vault 里只有 `.md` / 附件；应用卸载后用户数据依然能被任何编辑器打开。
- **DB 是派生数据**：`.mynotes/` 下**不**存 SQLite，DB 放在 OS app-support（Mac: `~/Library/Application Support/`）以避免 iCloud/Syncthing 与 WAL 锁互撕；删库即全量 rescan。
- **单写者 SQLite**：`Arc<Mutex<Connection>>` 在 AppState 里，IPC 线程只借 Arc clone，锁粒度是单条查询——满足个人知识库（万级笔记）规模。
- **链接解析两步走**：先 `notes.title = links.dst` 命中，再 `path` 按 stem 命中；未命中保留 `dst_resolved = NULL`，前端面板能突出显示"待创建"。

## 常用命令

| 命令               | 用途                                                 |
| ------------------ | ---------------------------------------------------- |
| `pnpm tauri:dev`   | 开发模式启动（前端 HMR + Rust 后端热编译）           |
| `pnpm tauri:build` | 生产构建 → `src-tauri/target/release/`               |
| `pnpm dev`         | 只启前端壳子做浏览器预览；vault / 文件系统能力不可用 |
| `pnpm check`       | Svelte + TS 类型检查                                 |
| `pnpm format`      | Prettier 全仓格式化                                  |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 跑 Rust 单元测试（含 indexer 解析器测试） |

## 许可证

MIT
