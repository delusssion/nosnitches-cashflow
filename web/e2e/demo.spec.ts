import { expect, test, type Page } from '@playwright/test';
import { fileURLToPath } from 'node:url';

// Сценарий docs/DEMO.md на демо-данных; скриншоты — в docs/screenshots/ для презентации.
const SHOTS = fileURLToPath(new URL('../../docs/screenshots/', import.meta.url));
const shot = (page: Page, name: string, fullPage = false) =>
  page.screenshot({ path: `${SHOTS}${name}.png`, fullPage });

test('демо: портфель → сделки → сверка', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));

  // 1. Загрузка портфеля
  await page.goto('/');
  await page.getByRole('button', { name: 'Загрузить демо' }).click();
  await expect(page.getByText('сделок', { exact: true })).toBeVisible();
  await expect(page.getByRole('row').filter({ hasText: 'DEMO-ANN-001' })).toBeVisible();
  await page.waitForTimeout(500); // анимация графика
  await shot(page, '01-portfolio', true);

  // 2. ANN на временной шкале: остаток убывает до 0
  await page.getByRole('row').filter({ hasText: 'DEMO-ANN-001' }).click();
  await expect(page).toHaveURL(/#\/deal\/DEMO-ANN-001$/);
  await expect(page.getByRole('heading', { name: /Временная шкала/ })).toBeVisible();
  await expect(page.getByRole('row').filter({ hasText: 'close_payment' })).toContainText('0,00');
  await page.waitForTimeout(300);
  await shot(page, '02-deal-ann', true);

  // 3. FLOAT с репрайсингом: зелёные отметки с новой ставкой
  await page.getByRole('link', { name: 'Портфель' }).click();
  await page.getByRole('row').filter({ hasText: 'DEMO-FLT-002' }).click();
  await expect(page.getByRole('row').filter({ hasText: 'repricing' }).first()).toContainText(
    '10,50 %',
  );
  await page.waitForTimeout(300);
  await shot(page, '03-deal-float', true);

  // 4. Сверка: все сделки зелёные
  await page.getByRole('link', { name: 'Сверка' }).click();
  await page.getByRole('button', { name: 'Демо-сверка' }).click();
  await expect(page.getByText(/Все 3 сделок совпали с эталоном/)).toBeVisible();
  await shot(page, '04-compare-green', true);

  // 5. Итог по портфелю
  await page.getByRole('link', { name: 'Портфель' }).click();
  await expect(page.getByText('с ненулевым остатком')).toBeVisible();
  await page.waitForTimeout(500);
  await shot(page, '05-portfolio-total');

  expect(errors).toEqual([]);
});
