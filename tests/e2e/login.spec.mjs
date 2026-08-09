import { expect, test } from '@playwright/test';

async function logIn(page) {
  await page.goto('/');

  await expect(page.locator('#login-panel')).toBeVisible();
  await page.locator('#login-username').fill('test');
  await page.locator('#login-password').fill('23452345');
  await page.locator('#login-btn').click();

  await expect(page.locator('#login-panel')).toBeHidden();
}

test('the isolated agent user can log in', async ({ page }) => {
  await logIn(page);
});

test('scheduled topic links open the scheduled archive view', async ({ page }) => {
  await logIn(page);
  await page.goto('/archive?status=scheduled&topic=Rust');

  await expect(page.locator('#view-archive')).toBeVisible();
  await expect(page.getByText('Scheduled cards for Rust', { exact: true })).toBeVisible();
});
