/**
 * e2e/cycles-navigation.spec.ts — Cycle → チケット詳細ナビゲーション E2E
 *
 * 前提:
 *   - Rust API (localhost:8151) + Vite dev server (localhost:5173) が起動済み
 *   - テストユーザー admin/admin が存在
 *   - 対象プロジェクトに active な Cycle とチケットが1件以上存在
 *
 * 実行: cd frontend && npx playwright test e2e/cycles-navigation.spec.ts
 */
import { test, expect, Page } from '@playwright/test';

const TEST_USER = { username: 'admin', password: 'admin' };

async function login(page: Page): Promise<void> {
  await page.goto('/login');
  await page.fill("input[name='username'], input[type='email']", TEST_USER.username);
  await page.fill("input[type='password']", TEST_USER.password);
  await page.click("button[type='submit']");
  await page.waitForURL(/\/(dashboard|$)/, { timeout: 15000 });
}

/** サイドバーからサイクル画面へ遷移（プロジェクトコンテキスト前提） */
async function goToCycles(page: Page): Promise<void> {
  const cyclesNav = page.locator('[data-testid="nav-cycles"]');
  await expect(cyclesNav).toBeVisible({ timeout: 10000 });
  await cyclesNav.click();
  await page.waitForURL(/\/cycles/, { timeout: 10000 });
}

test.describe('Cycle チケット詳細ナビゲーション', () => {
  test.beforeEach(async ({ page }) => {
    await login(page);
    await goToCycles(page);
  });

  test('Cycle詳細 → チケット選択で Cycle コンテキストを維持する', async ({ page }) => {
    // アクティブ Cycle カードまたは Cycle 行をクリック
    const activeCard = page.locator('[data-testid="cycle-active-card"]');
    const cycleRow = page.locator('[data-testid^="cycle-row-"]').first();

    if (await activeCard.isVisible()) {
      await activeCard.click();
    } else if (await cycleRow.isVisible()) {
      await cycleRow.click();
    } else {
      test.skip(true, 'テスト対象の Cycle が存在しません');
      return;
    }

    // Cycle 詳細 URL
    await page.waitForURL(/\/cycles\/\d+$/, { timeout: 10000 });
    await expect(page.locator('[data-testid="cycle-detail-page"]')).toBeVisible();

    const cycleDetailUrl = page.url();
    const cycleIdMatch = cycleDetailUrl.match(/\/cycles\/(\d+)/);
    expect(cycleIdMatch).not.toBeNull();

    // チケット行をクリック（Cycle 内にチケットが必要）
    const ticketRow = page.locator('[data-testid^="ticket-row-"]').first();
    if (!(await ticketRow.isVisible())) {
      test.skip(true, 'Cycle 内にチケットがありません');
      return;
    }

    const ticketKey = (await ticketRow.getAttribute('data-testid'))?.replace('ticket-row-', '') ?? '';
    expect(ticketKey).toBeTruthy();

    await ticketRow.click();

    // URL が /cycles/:cycleId/:ticketKey であること（/tickets ではない）
    await page.waitForURL(new RegExp(`/cycles/\\d+/${ticketKey.replace('-', '\\-')}`), {
      timeout: 10000,
    });
    expect(page.url()).toContain('/cycles/');
    expect(page.url()).not.toMatch(/\/tickets\/[^/]+$/);

    // 詳細パネル表示
    await expect(page.locator('[data-testid="detail-panel"]')).toBeVisible();

    // チケットテーブルも表示（Cycle 内一覧）
    await expect(page.locator('[data-testid="ticket-table-page"]')).toBeVisible();
  });

  test('詳細パネルを閉じると Cycle 詳細に戻る', async ({ page }) => {
    const activeCard = page.locator('[data-testid="cycle-active-card"]');
    if (await activeCard.isVisible()) {
      await activeCard.click();
    } else {
      test.skip(true, 'アクティブ Cycle がありません');
      return;
    }

    await page.waitForURL(/\/cycles\/\d+$/);

    const ticketRow = page.locator('[data-testid^="ticket-row-"]').first();
    if (!(await ticketRow.isVisible())) {
      test.skip(true, 'Cycle 内にチケットがありません');
      return;
    }

    await ticketRow.click();
    await expect(page.locator('[data-testid="detail-panel"]')).toBeVisible();

    const cycleDetailUrl = page.url().replace(/\/[^/]+$/, '');

    // パネル閉じる
    await page.locator('[data-testid="detail-panel"] .detail-panel__close').click();

    await page.waitForURL(/\/cycles\/\d+$/, { timeout: 10000 });
    expect(page.url()).toBe(cycleDetailUrl);
    await expect(page.locator('[data-testid="detail-panel"]')).not.toBeVisible();
  });

  test('戻るボタンで Cycle 一覧に戻る', async ({ page }) => {
    const activeCard = page.locator('[data-testid="cycle-active-card"]');
    if (await activeCard.isVisible()) {
      await activeCard.click();
    } else {
      test.skip(true, 'アクティブ Cycle がありません');
      return;
    }

    await page.waitForURL(/\/cycles\/\d+$/);

    await page.locator('[data-testid="cycle-detail-back"]').click();
    await page.waitForURL(/\/cycles$/, { timeout: 10000 });
    expect(page.url()).toMatch(/\/cycles$/);
  });

  test('Cycle コンテキスト中はサイドバー「サイクル」がアクティブ', async ({ page }) => {
    const activeCard = page.locator('[data-testid="cycle-active-card"]');
    if (await activeCard.isVisible()) {
      await activeCard.click();
    } else {
      test.skip(true, 'アクティブ Cycle がありません');
      return;
    }

    await page.waitForURL(/\/cycles\/\d+$/);

    const ticketRow = page.locator('[data-testid^="ticket-row-"]').first();
    if (await ticketRow.isVisible()) {
      await ticketRow.click();
      await page.waitForURL(/\/cycles\/\d+\/.+/);
    }

    const cyclesNav = page.locator('[data-testid="nav-cycles"]');
    await expect(cyclesNav).toHaveClass(/sidebar__link--active/);

    const ticketsNav = page.locator('[data-testid="nav-tickets"]');
    await expect(ticketsNav).not.toHaveClass(/sidebar__link--active/);
  });
});

test.describe('回帰: 通常チケット一覧', () => {
  test.beforeEach(async ({ page }) => {
    await login(page);
  });

  test('チケット一覧からの詳細は従来どおり /tickets/:id', async ({ page }) => {
    await page.locator('[data-testid="nav-tickets"]').click();
    await page.waitForURL(/\/tickets/);

    const ticketRow = page.locator('[data-testid^="ticket-row-"]').first();
    if (!(await ticketRow.isVisible())) {
      test.skip(true, 'チケットがありません');
      return;
    }

    await ticketRow.click();
    await page.waitForURL(/\/tickets\/[^/]+$/);
    expect(page.url()).toContain('/tickets/');
    expect(page.url()).not.toContain('/cycles/');
  });
});
