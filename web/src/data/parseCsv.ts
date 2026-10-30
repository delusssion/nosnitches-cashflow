import Papa from 'papaparse';
import { createCollector, type ParseResult } from './collector';
import type { WorkerRequest, WorkerResponse } from './parseWorker';
import type { CsvKind, RowTypes } from './types';

export type { ParseResult } from './collector';

export interface ParseOptions {
  /** Прогресс 0–100. */
  onProgress?: (percent: number) => void;
  signal?: AbortSignal;
}

/** Потоковый разбор CSV в Web Worker: UI не блокируется даже на сотнях тысяч строк. */
export function parseCsv<K extends CsvKind>(
  file: Blob,
  kind: K,
  { onProgress, signal }: ParseOptions = {},
): Promise<ParseResult<K>> {
  return new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./parseWorker.ts', import.meta.url), { type: 'module' });
    const rows: RowTypes[K][] = [];

    const stop = () => worker.terminate();
    signal?.addEventListener('abort', () => {
      stop();
      reject(new DOMException('Разбор отменён', 'AbortError'));
    });

    worker.onmessage = ({ data }: MessageEvent<WorkerResponse>) => {
      switch (data.type) {
        case 'progress':
          onProgress?.(data.percent);
          break;
        case 'rows':
          for (const r of data.rows as RowTypes[K][]) rows.push(r);
          break;
        case 'done':
          stop();
          resolve({
            rows,
            errors: data.errors,
            errorCount: data.errorCount,
            rowCount: data.rowCount,
          });
          break;
        case 'fail':
          stop();
          reject(new Error(data.message));
          break;
      }
    };
    worker.onerror = (e) => {
      stop();
      reject(new Error(e.message));
    };
    worker.postMessage({ file, kind } satisfies WorkerRequest);
  });
}

/** Синхронный разбор строки CSV — для тестов и маленьких файлов. */
export function parseCsvText<K extends CsvKind>(text: string, kind: K): ParseResult<K> {
  const collector = createCollector(kind);
  const parsed = Papa.parse<Record<string, string>>(text, {
    header: true,
    skipEmptyLines: 'greedy',
  });
  if (!collector.checkHeader(parsed.meta.fields ?? [])) return collector.result;
  for (const e of parsed.errors) {
    collector.addError({ row: (e.row ?? 0) + 1, field: '', message: e.message });
  }
  collector.push(parsed.data);
  return collector.result;
}
