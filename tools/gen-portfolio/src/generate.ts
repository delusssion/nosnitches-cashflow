import { stringify } from 'csv-stringify';
import { createWriteStream, mkdirSync } from 'node:fs';
import { dirname } from 'node:path';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { DEAL_COLUMNS, makeDeal } from './deal.ts';
import { Rng } from './random.ts';

export interface GenOptions {
  n: number;
  seed: number;
  reportDate: string;
}

/** Потоковый источник строк: сделки создаются по мере чтения, в памяти не копятся. */
export function dealStream({ n, seed, reportDate }: GenOptions): Readable {
  const rng = new Rng(seed);
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
        if (!this.push(DEAL_COLUMNS.map((c) => row[c] ?? ''))) return;
      }
    },
  });
}

export async function generateToFile(opts: GenOptions, out: string): Promise<void> {
  mkdirSync(dirname(out), { recursive: true });
  await pipeline(
    dealStream(opts),
    stringify({ header: true, columns: [...DEAL_COLUMNS], eof: true }),
    createWriteStream(out),
  );
}
