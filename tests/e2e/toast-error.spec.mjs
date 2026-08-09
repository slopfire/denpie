import { expect, test } from '@playwright/test';
import http from 'node:http';

// Local mock provider: returns a structured OpenAI-style error body so the
// vision test exercises the provider-error formatting path end to end.
let mockServer;
let mockPort;

test.beforeAll(async () => {
  mockServer = http.createServer((_req, res) => {
    res.writeHead(429, { 'Content-Type': 'application/json' });
    res.end(
      JSON.stringify({
        error: {
          message: 'Rate limit exceeded for the model',
          type: 'rate_limit_error',
          code: 'rate_limit',
        },
      })
    );
  });
  await new Promise((resolve) => mockServer.listen(0, '127.0.0.1', resolve));
  mockPort = mockServer.address().port;
});

test.afterAll(async () => {
  await new Promise((resolve) => mockServer.close(resolve));
});

async function logIn(page) {
  await page.goto('/');

  await expect(page.locator('#login-panel')).toBeVisible();
  await page.locator('#login-username').fill('test');
  await page.locator('#login-password').fill('23452345');
  await page.locator('#login-btn').click();

  await expect(page.locator('#login-panel')).toBeHidden();
}

test('LLM failure toast shows usable details and persists until dismissed', async ({ page }) => {
  await logIn(page);
  await page.goto('/settings');
  await expect(page.locator('#settings-form')).toBeVisible();

  // Point the LLM at the mock provider and let the debounced auto-save settle.
  await page.locator('#model-input').fill('mock-vision-model');
  await page.locator('#base-url-input').fill(`http://127.0.0.1:${mockPort}/v1`);
  await page.locator('#api-key-input').fill('sk-test');
  await page.waitForTimeout(2500);

  await page.locator('#test-vision-model').click();

  // Error toast with the extracted provider message, not a raw JSON dump.
  await expect(page.locator('#toast')).toHaveClass(/toast-error/);
  await expect(page.locator('#toast')).toHaveClass(/show/);
  await expect(page.locator('#toast .toast-message')).toContainText('HTTP 429');
  await expect(page.locator('#toast .toast-message')).toContainText('Rate limit exceeded');

  // Expandable details carry the bounded raw provider body for debugging.
  await page.getByRole('button', { name: 'Show details' }).click();
  await expect(page.locator('#toast .toast-detail')).toContainText('Raw response:');

  // Errors must stay visible until dismissed — well past the old auto-hide
  // window for info/success toasts.
  await page.waitForTimeout(5000);
  await expect(page.locator('#toast')).toHaveClass(/show/);
  await expect(page.locator('#toast .toast-detail')).toBeVisible();

  // Manual dismissal still works.
  await page.locator('#toast .toast-dismiss').click();
  await expect(page.locator('#toast')).not.toHaveClass(/show/);
});
