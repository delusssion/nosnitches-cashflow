import { createContext, useCallback, useMemo, useState, type ReactNode } from 'react';
import { parseCsv } from '../data/parseCsv';
import { buildStore, type CashflowStore } from '../data/store';
import type { CashflowEvent, CsvKind, Deal, ValidationError } from '../data/types';

export interface LoadState {
  fileName: string;
  progress: number;
  status: 'loading' | 'done' | 'failed';
  errors: ValidationError[];
  errorCount: number;
  rowCount: number;
  message?: string;
}

export interface DataState {
  cashflow: CashflowStore | null;
  deals: Map<string, Deal> | null;
  loads: Partial<Record<CsvKind, LoadState>>;
  loadFile: (kind: CsvKind, file: Blob, fileName: string) => Promise<void>;
  loadDemo: () => Promise<void>;
  reset: () => void;
}

// eslint-disable-next-line react-refresh/only-export-components
export const DataContext = createContext<DataState | null>(null);

const DEMO_FILES: Record<CsvKind, string> = {
  cashflow: 'samples/cashflow_sample.csv',
  deals: 'samples/deals_sample.csv',
};

export function DataProvider({ children }: { children: ReactNode }) {
  const [cashflow, setCashflow] = useState<CashflowStore | null>(null);
  const [deals, setDeals] = useState<Map<string, Deal> | null>(null);
  const [loads, setLoads] = useState<DataState['loads']>({});

  const patch = useCallback(
    (kind: CsvKind, p: Partial<LoadState>) =>
      setLoads((prev) => ({ ...prev, [kind]: { ...prev[kind]!, ...p } })),
    [],
  );

  const loadFile = useCallback(
    async (kind: CsvKind, file: Blob, fileName: string) => {
      setLoads((prev) => ({
        ...prev,
        [kind]: {
          fileName,
          progress: 0,
          status: 'loading',
          errors: [],
          errorCount: 0,
          rowCount: 0,
        },
      }));
      try {
        const res = await parseCsv(file, kind, {
          onProgress: (progress) => patch(kind, { progress }),
        });
        if (kind === 'cashflow') {
          setCashflow(buildStore(res.rows as CashflowEvent[]));
        } else {
          setDeals(new Map((res.rows as Deal[]).map((d) => [d.deal_id, d])));
        }
        patch(kind, {
          status: 'done',
          progress: 100,
          errors: res.errors,
          errorCount: res.errorCount,
          rowCount: res.rowCount,
        });
      } catch (e) {
        patch(kind, { status: 'failed', message: e instanceof Error ? e.message : String(e) });
      }
    },
    [patch],
  );

  const loadDemo = useCallback(async () => {
    await Promise.all(
      (Object.keys(DEMO_FILES) as CsvKind[]).map(async (kind) => {
        const res = await fetch(DEMO_FILES[kind]);
        await loadFile(kind, await res.blob(), DEMO_FILES[kind].split('/').pop()!);
      }),
    );
  }, [loadFile]);

  const reset = useCallback(() => {
    setCashflow(null);
    setDeals(null);
    setLoads({});
  }, []);

  const value = useMemo(
    () => ({ cashflow, deals, loads, loadFile, loadDemo, reset }),
    [cashflow, deals, loads, loadFile, loadDemo, reset],
  );
  return <DataContext.Provider value={value}>{children}</DataContext.Provider>;
}
