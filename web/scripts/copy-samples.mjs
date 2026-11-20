// Копирует демо-данные в public/samples, чтобы кнопки «Загрузить демо» работали без сервера:
// testdata/samples (учебные 3 сделки и вывод движка), тестовые сделки и эталоны testdata/golden.
import { cpSync, mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const testdata = resolve(root, '../testdata');
const dest = resolve(root, 'public/samples');

mkdirSync(resolve(dest, 'engine'), { recursive: true });
mkdirSync(resolve(dest, 'golden'), { recursive: true });
for (const name of ['deals_sample.csv', 'cashflow_sample.csv']) {
  cpSync(resolve(testdata, 'samples', name), resolve(dest, name));
}
for (const name of ['cashflow.csv', 'errors.csv']) {
  cpSync(resolve(testdata, 'samples/engine', name), resolve(dest, 'engine', name));
}
cpSync(resolve(testdata, 'deals/test_deals.csv'), resolve(dest, 'engine/test_deals.csv'));
cpSync(resolve(testdata, 'samples/scenario'), resolve(dest, 'scenario'), { recursive: true });

const golden = readdirSync(resolve(testdata, 'golden')).filter((f) => /^deal_.+\.csv$/.test(f));
for (const name of golden) cpSync(resolve(testdata, 'golden', name), resolve(dest, 'golden', name));
writeFileSync(resolve(dest, 'golden/index.json'), JSON.stringify(golden.sort()));
