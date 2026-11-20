import { stringify } from 'csv-stringify';
import { createWriteStream, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { DEAL_COLUMNS, makeDeal, makePrepayments, PREPAYMENT_COLUMN } from './deal.ts';
import { Rng } from './random.ts';

export interface GenOptions {
  n: number;
  seed: number;
  reportDate: string;
  /** Доля сделок с досрочками (0 — колонки prepayment_schedule нет, файл как в спринте 2). */
  prepayShare?: number;
}

export const columnsFor = (prepayShare = 0) =>
  prepayShare > 0 ? [...DEAL_COLUMNS, PREPAYMENT_COLUMN] : [...DEAL_COLUMNS];

/** Потоковый источник строк: сделки создаются по мере чтения, в памяти не копятся. */
export function dealStream({ n, seed, reportDate, prepayShare = 0 }: GenOptions): Readable {
  const rng = new Rng(seed);
  // Досрочки — из своего потока случайных чисел: остальные поля не зависят от флага.
  const prepayRng = new Rng(seed ^ 0x5eed_9a1);
  let i = 0;
  return new Readable({
    objectMode: true,
    read(size) {
      for (let k = 0; k < size; k++) {
        if (i >= n) {
          this.push(null);
          return;
        }
        const row = makeDeal(rng, i++, reportDate);
        const cells: (string | number)[] = DEAL_COLUMNS.map((c) => row[c] ?? '');
        if (prepayShare > 0) cells.push(makePrepayments(prepayRng, row, prepayShare));
        if (!this.push(cells)) return;
      }
    },
  });
}

export async function generateToFile(opts: GenOptions, out: string): Promise<void> {
  mkdirSync(dirname(out), { recursive: true });
  await pipeline(
    dealStream(opts),
    stringify({ header: true, columns: columnsFor(opts.prepayShare), eof: true }),
    createWriteStream(out),
  );
}
