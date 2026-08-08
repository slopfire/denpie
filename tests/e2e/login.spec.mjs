import { expect, test } from '@playwright/test';

test('the isolated agent user can log in', async ({ page }) => {
  await page.goto('/');

  await expect(page.locator('#login-panel')).toBeVisible();
  await page.locator('#login-username').fill('test');
  await page.locator('#login-password').fill('23452345');
  await page.locator('#login-btn').click();

  await expect(page.locator('#login-panel')).toBeHidden();
});
