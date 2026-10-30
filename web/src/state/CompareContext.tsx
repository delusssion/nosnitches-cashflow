import { createContext, useCallback, useMemo, useState, type ReactNode } from 'react';
import { parseCsv } from '../data/parseCsv';
import type { CashflowEvent, ValidationError } from '../data/types';

export type CompareSide = 'engine' | 'golden';

export interface SideState {
  files: string[];
  events: CashflowEvent[];
  progress: number;
  status: 'idle' | 'loading' | 'done' | 'failed';
  errors: ValidationError[];
  errorCount: number;
  message?: string;
}

export interface CompareState {
  engine: SideState;
  golden: SideState;
  tolerance: number;
  setTolerance: (t: number) => void;
  loadSide: (side: CompareSide, files: { blob: Blob; name: string }[]) => Promise<void>;
}

const EMPTY: SideState = {
  files: [],
  events: [],
  progress: 0,
  status: 'idle',
  errors: [],
  errorCount: 0,
};

// eslint-disable-next-line react-refresh/only-export-components
export const CompareContext = createContext<CompareState | null>(null);

/** Состояние экрана сверки живёт выше роутера, чтобы не терялось при переходах между экранами. */
export function CompareProvider({ children }: { children: ReactNode }) {
  const [engine, setEngine] = useState<SideState>(EMPTY);
  const [golden, setGolden] = useState<SideState>(EMPTY);
  const [tolerance, setTolerance] = useState(0.01);

  const loadSide = useCallback(async (side: CompareSide, files: { blob: Blob; name: string }[]) => {
    const set = side === 'engine' ? setEngine : setGolden;
    set({ ...EMPTY, files: files.map((f) => f.name), status: 'loading' });
    try {
      const total = files.reduce((s, f) => s + f.blob.size, 0) || 1;
      let done = 0;
      const events: CashflowEvent[] = [];
      const errors: ValidationError[] = [];
      let errorCount = 0;
      // Несколько файлов эталона (deal_<id>.csv) разбираем по очереди и склеиваем.
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

  const value = useMemo(
    () => ({ engine, golden, tolerance, setTolerance, loadSide }),
    [engine, golden, tolerance, loadSide],
  );
  return <CompareContext.Provider value={value}>{children}</CompareContext.Provider>;
}
