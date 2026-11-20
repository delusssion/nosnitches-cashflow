import { useCallback, useState } from 'react';
import { parseCsv } from '../data/parseCsv';
import type { CashflowEvent, ValidationError } from '../data/types';

/** Одна «сторона» сравнения: один или несколько CSV кэшфлоу, склеенные в один список событий. */
export interface SideState {
  files: string[];
  events: CashflowEvent[];
  progress: number;
  status: 'idle' | 'loading' | 'done' | 'failed';
  errors: ValidationError[];
  errorCount: number;
  message?: string;
}

export interface NamedBlob {
  blob: Blob;
  name: string;
}

export const EMPTY_SIDE: SideState = {
  files: [],
  events: [],
  progress: 0,
  status: 'idle',
  errors: [],
  errorCount: 0,
};

/** Загрузка стороны: файлы разбираются по очереди в Web Worker, прогресс — по сумме размеров. */
export function useCashflowSide(): [SideState, (files: NamedBlob[]) => Promise<void>] {
  const [side, set] = useState<SideState>(EMPTY_SIDE);

  const load = useCallback(async (files: NamedBlob[]) => {
    set({ ...EMPTY_SIDE, files: files.map((f) => f.name), status: 'loading' });
    try {
      const total = files.reduce((s, f) => s + f.blob.size, 0) || 1;
      let done = 0;
      const events: CashflowEvent[] = [];
      const errors: ValidationError[] = [];
      let errorCount = 0;
      for (const f of files) {
        const res = await parseCsv(f.blob, 'cashflow', {
          onProgress: (p) =>
            set((s) => ({
              ...s,
              progress: Math.round(((done + (f.blob.size * p) / 100) / total) * 100),
            })),
        });
        done += f.blob.size;
        for (const r of res.rows) events.push(r);
        errors.push(
          ...res.errors
            .slice(0, 1000 - errors.length)
            .map((e) => ({ ...e, field: `${f.name}: ${e.field}` })),
        );
        errorCount += res.errorCount;
      }
      set({
        files: files.map((f) => f.name),
        events,
        progress: 100,
        status: 'done',
        errors,
        errorCount,
      });
    } catch (e) {
      set((s) => ({ ...s, status: 'failed', message: e instanceof Error ? e.message : String(e) }));
    }
  }, []);

  return [side, load];
}
