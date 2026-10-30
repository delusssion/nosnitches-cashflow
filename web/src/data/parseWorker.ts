/// <reference lib="webworker" />
import Papa from 'papaparse';
import { createCollector } from './collector';
import type { CsvKind, RowTypes, ValidationError } from './types';

export interface WorkerRequest {
  file: Blob;
  kind: CsvKind;
}

export type WorkerResponse =
  | { type: 'progress'; percent: number }
  | { type: 'rows'; rows: RowTypes[CsvKind][] }
  | { type: 'done'; errors: ValidationError[]; errorCount: number; rowCount: number }
  | { type: 'fail'; message: string };

const ctx = self as unknown as DedicatedWorkerGlobalScope;
const post = (msg: WorkerResponse) => ctx.postMessage(msg);

ctx.onmessage = ({ data }: MessageEvent<WorkerRequest>) => {
  const { file, kind } = data;
  const collector = createCollector(kind);
  let headerOk: boolean | null = null;
  let lastPercent = -1;

  Papa.parse<Record<string, string>>(file as File, {
    header: true,
    skipEmptyLines: 'greedy',
    chunkSize: 1 << 20,
    chunk(results, parser) {
      if (headerOk === null) {
        headerOk = collector.checkHeader(results.meta.fields ?? []);
        if (!headerOk) {
          parser.abort();
          return;
        }
      }
      for (const e of results.errors) {
        collector.addError({ row: (e.row ?? 0) + 1, field: '', message: e.message });
      }
      const rows = collector.push(results.data);
      // Строки отдаём порциями и не копим в worker, чтобы не держать две копии.
      collector.result.rows.length = 0;
      if (rows.length) post({ type: 'rows', rows });
      const percent = file.size ? Math.floor((results.meta.cursor / file.size) * 100) : 100;
      if (percent !== lastPercent) {
        lastPercent = percent;
        post({ type: 'progress', percent: Math.min(percent, 99) });
      }
    },
    complete() {
      const { errors, errorCount, rowCount } = collector.result;
      post({ type: 'progress', percent: 100 });
      post({ type: 'done', errors, errorCount, rowCount });
    },
    error(err) {
      post({ type: 'fail', message: err.message });
    },
  });
};
