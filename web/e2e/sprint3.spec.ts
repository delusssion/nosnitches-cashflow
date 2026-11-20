import { expect, test, type Page } from '@playwright/test';
import { fileURLToPath } from 'node:url';

// Сценарий спринта 3 из docs/DEMO.md: реальный вывод движка, сверка с эталонами, сценарий, ставки.
const SHOTS = fileURLToPath(new URL('../../docs/screenshots/', import.meta.url));
const shot = (page: Page, name: string, fullPage = false) =>
  page.screenshot({ path: `${SHOTS}${name}.png`, fullPage });

test('спринт 3: движок → сверка → сценарий → ставки', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', (e) => errors.push(e.message));

  // 1. Реальный вывод cfgen по 55 тестовым сделкам с errors.csv
  await page.goto('/');
  await page.getByRole('button', { name: 'Результат движка: 55 сделок' }).click();
  await expect(page.getByText('отклонено / с предупреждением')).toBeVisible();
  await page.waitForTimeout(500);
  await shot(page, '06-engine-portfolio');
  // отклонённая сделка видна в таблице (строки виртуальные — находим поиском)
  await page.getByPlaceholder('Поиск по deal_id').fill('32');
  await expect(page.getByRole('row').filter({ hasText: 'UNSUPPORTED_CURRENCY' })).toBeVisible();

  // 2. Отклонённая сделка 32 — замечание движка
  await page.goto('/#/deal/32');
  await expect(page.getByText('UNSUPPORTED_CURRENCY')).toBeVisible();

  // 3. Настоящая сверка: движок против 13 эталонов аналитика
  await page.goto('/#/compare');
  await page.getByRole('button', { name: 'Движок против эталонов' }).click();
  await expect(page.getByText(/Все 13 сделок совпали с эталоном/)).toBeVisible();
  await shot(page, '07-compare-engine-golden');

  // 4. Сценарий: досрочка по 22, ставка по 7, cap по 25
  await page.goto('/#/scenario');
  await page.getByRole('button', { name: 'Демо-сценарий' }).click();
  await expect(page.getByText('Остаток долга')).toBeVisible();
  await expect(page.getByRole('row').filter({ hasText: 'prepayment' })).toContainText('новое');
  await page.waitForTimeout(500);
  await shot(page, '08-scenario', true);

  // 5. Ставки: история КС, прогноз H3 со стрессом +200 б.п.
  await page.goto('/#/rates');
  await page.getByRole('button', { name: 'КС ЦБ из testdata' }).click();
  await page.getByRole('button', { name: '+200 б.п.' }).click();
  await expect(page.getByText('10,00 %')).toBeVisible();
  await page.waitForTimeout(500);
  await shot(page, '09-rates');

  expect(errors).toEqual([]);
});
