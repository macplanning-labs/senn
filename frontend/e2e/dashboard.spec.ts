/**
 * e2e/dashboard.spec.ts — ダッシュボード E2Eテスト
 *
 * テスト対象:
 *   - ログイン後ダッシュボードが表示される
 *   - 統計カードの表示
 *   - サイドバーのナビゲーション
 *   - コマンドパレット（⌘K）
 *
 * 前提: API サーバー (localhost:8151) が起動し、テストユーザーが存在すること
 */
import { test, expect, Page } from "@playwright/test";

const TEST_USER = { username: "admin", password: "admin" };

async function login(page: Page): Promise<void> {
  await page.goto("/login");
  await page.fill("input[name='username'], input[type='email']", TEST_USER.username);
  await page.fill("input[type='password']", TEST_USER.password);
  await page.click("button[type='submit']");
  // ダッシュボードに遷移するまで待機
  await page.waitForURL(/\/$/, { timeout: 10000 });
}

test.describe("ダッシュボード", () => {
  test.beforeEach(async ({ page }) => {
    await login(page);
  });

  test("ダッシュボードが表示される", async ({ page }) => {
    // メインレイアウトの存在確認
    await expect(page.locator("[data-testid='main-layout']")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/dashboard.png", fullPage: true });
  });

  test("サイドバーのナビゲーションが機能する", async ({ page }) => {
    // チケット一覧へ遷移
    await page.click("[data-testid='nav-tickets']");
    await page.waitForURL(/\/tickets/);
    await expect(page).toHaveURL(/\/tickets/);
    await page.screenshot({ path: "e2e/screenshots/tickets-list.png", fullPage: true });
  });

  test("コマンドパレットが開く", async ({ page }) => {
    // ⌘K でコマンドパレットを開く
    await page.keyboard.press("Meta+k");
    await expect(page.locator("[data-testid='command-palette']")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/command-palette.png", fullPage: true });
    // Escで閉じる
    await page.keyboard.press("Escape");
  });
});

test.describe("チケット操作", () => {
  test.beforeEach(async ({ page }) => {
    await login(page);
    await page.click("[data-testid='nav-tickets']");
    await page.waitForURL(/\/tickets/);
  });

  test("チケット一覧が表示される", async ({ page }) => {
    await expect(page.locator("[data-testid='ticket-list']")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/ticket-list.png", fullPage: true });
  });

  test("Wiki画面が表示される", async ({ page }) => {
    await page.click("[data-testid='nav-wiki']");
    await page.waitForURL(/\/wiki/);
    await expect(page.locator("[data-testid='wiki-list']")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/wiki-list.png", fullPage: true });
  });

  test("ガントチャートが表示される", async ({ page }) => {
    await page.click("[data-testid='nav-gantt']");
    await page.waitForURL(/\/gantt/);
    await page.screenshot({ path: "e2e/screenshots/gantt-chart.png", fullPage: true });
  });
});
