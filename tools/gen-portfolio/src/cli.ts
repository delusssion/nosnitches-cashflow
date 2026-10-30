import { Command } from 'commander';
import { resolve } from 'node:path';
import { generateToFile } from './generate.ts';

const program = new Command()
  .name('gen-portfolio')
  .description('Синтетический портфель сделок в формате 1 docs/CONTRACT.md')
  .option('--n <count>', 'число сделок', '1000')
  .option('--seed <seed>', 'seed генератора (один seed — один и тот же файл)', '42')
  .option('--report-date <date>', 'дата расчёта YYYY-MM-DD', '2022-08-31')
  .option('--out <path>', 'куда писать CSV', '../../testdata/synthetic/deals.csv')
  .parse();

const o = program.opts<{ n: string; seed: string; reportDate: string; out: string }>();
const n = Number(o.n);
const seed = Number(o.seed);
if (!Number.isInteger(n) || n < 0) program.error('--n: ожидается целое ≥ 0');
if (!Number.isInteger(seed)) program.error('--seed: ожидается целое');
if (!/^\d{4}-\d{2}-\d{2}$/.test(o.reportDate)) program.error('--report-date: ожидается YYYY-MM-DD');

const out = resolve(o.out);
const t0 = performance.now();
await generateToFile({ n, seed, reportDate: o.reportDate }, out);
const s = (performance.now() - t0) / 1000;
console.log(`${n.toLocaleString('ru-RU')} сделок → ${out} за ${s.toFixed(1)} с`);
