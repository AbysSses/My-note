import { expect, test } from '@playwright/test';

/**
 * Phase 4 Stage 2 — agent-chat E2E coverage.
 *
 * The build under test is `PUBLIC_E2E=1 pnpm build && pnpm preview …`
 * (configured in `playwright.config.ts`'s `webServer.command`). With
 * that flag set, two mock layers ship into the bundle:
 *
 *   1. `src/lib/e2e/mockBootstrap.ts` seeds a fake vault on mount and
 *      installs a `window.__TAURI_INTERNALS__` stub so `invoke()` calls
 *      from production code resolve to plausible fake responses
 *      (file_write → null, file_move_with_refs → fake RenameResult, …).
 *   2. `src/lib/panel/ChatPanel.svelte` swaps in an in-panel mock
 *      provider that yields scripted streaming text + tool-call /
 *      proposal payloads keyed off keywords in the user message.
 *
 * Both layers are gated by the `?e2eMock=1` URL flag AND the build-time
 * `PUBLIC_E2E` constant — production bundles drop the entire mock
 * surface via Vite dead-code elimination.
 */
test.describe('agent-chat 回归骨架', () => {
  test('基础页面可达（E2E 脚手架冒烟）', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('body')).toBeVisible();
  });

  test('权限矩阵关闭后工具不可见且不可执行（UI gate）', async ({ page }) => {
    await page.goto('/?e2eMock=1');

    await page.getByTestId('open-settings').click();
    await expect(page.getByTestId('settings-modal')).toBeVisible();

    const aiToggle = page.getByTestId('ai-enabled-toggle');
    await expect(aiToggle).toBeChecked();

    await expect(page.getByTestId('panel-tab-chat')).toBeVisible();
    await expect(page.getByTestId('ai-tool-readonly')).toBeEnabled();
    await expect(page.getByTestId('ai-tool-writeback')).toBeEnabled();
    await expect(page.getByTestId('ai-tool-destructive')).toBeEnabled();

    await aiToggle.uncheck();
    await expect(aiToggle).not.toBeChecked();
    await expect(page.getByTestId('panel-tab-chat')).toHaveCount(0);
    await expect(page.getByTestId('ai-tool-readonly')).toBeDisabled();
    await expect(page.getByTestId('ai-tool-writeback')).toBeDisabled();
    await expect(page.getByTestId('ai-tool-destructive')).toBeDisabled();
  });

  test('chat 流式返回（delta -> done）', async ({ page }) => {
    await page.goto('/?e2eMock=1');
    await page.getByTestId('panel-tab-chat').click();
    await page.getByTestId('chat-compose').fill('请给我一句测试回复');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toBeVisible();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });
    await expect(page.getByTestId('chat-transcript')).toContainText('Mock 流式回复');
  });

  test('tool call 展示（request/result trace）', async ({ page }) => {
    await page.goto('/?e2eMock=1');
    await page.getByTestId('panel-tab-chat').click();
    await page.getByTestId('chat-compose').fill('请搜索 #project 相关笔记');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-tool-trace')).toBeVisible();
    await expect(page.getByTestId('chat-inline-tool-cards')).toBeVisible();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });
    await expect(page.getByTestId('chat-transcript')).toContainText('找到 1 条结果');
  });

  test('proposal accept / reject / adjust', async ({ page }) => {
    await page.goto('/?e2eMock=1');
    await page.getByTestId('panel-tab-chat').click();

    await page.getByTestId('chat-compose').fill('请给我一个摘要提案');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });
    await expect(page.getByTestId('proposal-card').first()).toBeVisible();

    await page.getByTestId('proposal-adjust').first().click();
    await expect(page.getByTestId('chat-compose')).toContainText('请基于刚才对');

    await page.getByTestId('proposal-reject').first().click();
    await expect(page.locator('.proposal-card__resolution--rejected').first()).toContainText('已拒绝');

    await page.getByTestId('chat-compose').fill('请再来一个摘要提案');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });
    await page.getByTestId('proposal-accept').last().click();
    await expect(page.locator('.proposal-card__resolution--accepted').last()).toContainText('已写入');
  });

  test('writeback 后 editor / note reload', async ({ page }) => {
    // Phase 4 Stage 2 — fills in the previously fixme'd writeback case.
    // The mock invoke stub returns `null` for `file_write`, so accepting
    // a propose_summary should propagate through `acceptProposal.ts`,
    // surface "已写入" in the proposal card, and trigger
    // `onOpenNote(target_rel_path, { forceReload: true })` — which in
    // `+page.svelte` flips `vaultState.openFilePath` and remounts the
    // editor (the `{#key …}` block guards against editor-state staleness).
    await page.goto('/?e2eMock=1');
    await page.getByTestId('panel-tab-chat').click();

    await page.getByTestId('chat-compose').fill('请给我一个摘要提案');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });

    const proposalCard = page.getByTestId('proposal-card').first();
    await expect(proposalCard).toBeVisible();

    await page.getByTestId('proposal-accept').first().click();
    await expect(page.locator('.proposal-card__resolution--accepted').first()).toContainText(
      '已写入'
    );

    // Editor should remount with the proposal target as the active file.
    // The mock proposal targets `1-notes/mock-note.md` when no file is
    // currently open (see ChatPanel mock branch).
    await expect(page.getByTestId('active-file-path')).toContainText('1-notes/mock-note.md');
    await expect(page.getByTestId('editor-host')).toBeVisible();
  });

  test('destructive proposal 二次确认', async ({ page }) => {
    await page.goto('/?e2eMock=1');
    await page.getByTestId('panel-tab-chat').click();

    await page.getByTestId('chat-compose').fill('请生成删除提案');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });
    await expect(page.getByTestId('proposal-card').first()).toBeVisible();

    page.once('dialog', async (dialog) => {
      await dialog.dismiss();
    });
    await page.getByTestId('proposal-accept').first().click();
    await expect(page.locator('.proposal-card__resolution--rejected').first()).toContainText('已取消');

    await page.getByTestId('chat-compose').fill('请再生成一个删除提案');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });
    page.once('dialog', async (dialog) => {
      await dialog.accept();
    });
    await page.getByTestId('proposal-accept').last().click();
    // Phase 4 Stage 3 — destructive delete now lands in the OS Trash;
    // the resolution copy mentions 回收站 so the user knows it's
    // recoverable.
    await expect(page.locator('.proposal-card__resolution--accepted').last()).toContainText('回收站');
  });

  test('provider 失败、超时、取消、重试', async ({ page }) => {
    await page.goto('/?e2eMock=1');
    await page.getByTestId('panel-tab-chat').click();

    await page.getByTestId('chat-compose').fill('请触发失败');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-failure-banner')).toBeVisible();
    await expect(page.getByTestId('chat-failure-kind')).toContainText('other');

    await page.getByTestId('chat-compose').fill('取消测试');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toBeVisible();
    await page.getByTestId('chat-cancel').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });
    await expect(page.getByTestId('chat-failure-banner')).toBeVisible();

    await page.getByTestId('chat-compose').fill('请给我一句测试回复');
    await page.getByTestId('chat-send').click();
    await expect(page.getByTestId('chat-streaming-bubble')).toHaveCount(0, { timeout: 6000 });
    await expect(page.getByTestId('chat-transcript')).toContainText('Mock 流式回复');
  });
});
