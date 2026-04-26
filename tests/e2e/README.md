# Agent Chat E2E（Phase 4 骨架）

## 目的

此目录用于承载 Phase 4 的 agent-chat 回归测试，优先覆盖发布关键链路：

- chat 流式返回
- tool call request/result 展示
- proposal accept/reject/adjust
- writeback 后 editor/note reload
- destructive proposal 二次确认
- 权限矩阵 gate
- provider 失败/超时/取消/重试

## 运行

```bash
pnpm e2e
```

> 默认通过 `playwright.config.ts` 启动 `pnpm preview`（`http://127.0.0.1:4173`）。
