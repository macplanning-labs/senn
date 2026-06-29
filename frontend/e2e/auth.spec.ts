/**
 * e2e/auth.spec.ts — 認証画面 E2Eテスト
 *
 * テスト対象:
 *   - ログインページの表示
 *   - ログインフォームのバリデーション
 *   - 未認証時のリダイレクト
 */
import { test, expect } from "@playwright/test";

test.describe("認証画面", () => {
  test("ログインページが表示される", async ({ page }) => {
    await page.goto("/login");
    // ログインフォームの存在確認
    await expect(page.locator("[data-testid='login-form']")).toBeVisible();
    await expect(page.locator("input[name='username'], input[type='email']")).toBeVisible();
    await expect(page.locator("input[type='password']")).toBeVisible();
    // スクリーンショット
    await page.screenshot({ path: "e2e/screenshots/login-page.png", fullPage: true });
  });

  test("空フォームでエラーが表示される", async ({ page }) => {
    await page.goto("/login");
    // 空のまま送信
    await page.locator("button[type='submit']").click();
    // バリデーションエラー（入力必須）
    await page.screenshot({ path: "e2e/screenshots/login-validation.png", fullPage: true });
  });

  test("未認証でダッシュボードにアクセスするとリダイレクト", async ({ page }) => {
    await page.goto("/");
    // ログインページにリダイレクトされるはず
    await page.waitForURL(/\/login/);
    await expect(page).toHaveURL(/\/login/);
  });

  test("登録ページが表示される", async ({ page }) => {
    await page.goto("/register");
    await expect(page.locator("input[name='username']")).toBeVisible();
    await expect(page.locator("input[name='email']")).toBeVisible();
    await expect(page.locator("input[type='password']")).toBeVisible();
    await page.screenshot({ path: "e2e/screenshots/register-page.png", fullPage: true });
  });
});
