// Копирует демо-данные testdata/samples в public/samples, чтобы кнопка «Загрузить демо» работала без сервера.
import { cpSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dest = resolve(root, 'public/samples');
mkdirSync(dest, { recursive: true });
for (const name of ['deals_sample.csv', 'cashflow_sample.csv']) {
  cpSync(resolve(root, '../testdata/samples', name), resolve(dest, name));
}
