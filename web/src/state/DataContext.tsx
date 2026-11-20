import { createContext, useCallback, useMemo, useState, type ReactNode } from 'react';
import { parseCsv } from '../data/parseCsv';
import { buildStore, type CashflowStore } from '../data/store';
import type { CashflowEvent, CsvKind, Deal, EngineIssue, ValidationError } from '../data/types';

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
  /** Замечания движка (errors.csv) по deal_id. */
  issues: Map<string, EngineIssue[]> | null;
  loads: Partial<Record<CsvKind, LoadState>>;
  loadFile: (kind: CsvKind, file: Blob, fileName: string) => Promise<void>;
  loadDemo: (set?: DemoSet) => Promise<void>;
  reset: () => void;
}

// eslint-disable-next-line react-refresh/only-export-components
export const DataContext = createContext<DataState | null>(null);

export type DemoSet = 'samples' | 'engine';

/** Наборы демо-данных: учебные (3 сделки) и реальный вывод движка cfgen по 55 тестовым сделкам. */
// eslint-disable-next-line react-refresh/only-export-components
export const DEMO_SETS: Record<DemoSet, Partial<Record<CsvKind, string>>> = {
  samples: {
    cashflow: 'samples/cashflow_sample.csv',
    deals: 'samples/deals_sample.csv',
  },
  engine: {
    cashflow: 'samples/engine/cashflow.csv',
    deals: 'samples/engine/test_deals.csv',
    errors: 'samples/engine/errors.csv',
  },
};

function groupIssues(rows: EngineIssue[]): Map<string, EngineIssue[]> {
  const map = new Map<string, EngineIssue[]>();
  for (const r of rows) {
    const id = r.deal_id ?? '';
    const list = map.get(id);
    if (list) list.push(r);
    else map.set(id, [r]);
  }
  return map;
}

export function DataProvider({ children }: { children: ReactNode }) {
  const [cashflow, setCashflow] = useState<CashflowStore | null>(null);
  const [deals, setDeals] = useState<Map<string, Deal> | null>(null);
  const [issues, setIssues] = useState<Map<string, EngineIssue[]> | null>(null);
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
        } else if (kind === 'deals') {
          setDeals(new Map((res.rows as Deal[]).map((d) => [d.deal_id, d])));
        } else {
          setIssues(groupIssues(res.rows as EngineIssue[]));
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

  const reset = useCallback(() => {
    setCashflow(null);
    setDeals(null);
    setIssues(null);
    setLoads({});
  }, []);

  const loadDemo = useCallback(
    async (set: DemoSet = 'samples') => {
      reset();
      const files = DEMO_SETS[set];
      await Promise.all(
        (Object.keys(files) as CsvKind[]).map(async (kind) => {
          const path = files[kind]!;
          const res = await fetch(path);
          await loadFile(kind, await res.blob(), path.split('/').pop()!);
        }),
      );
    },
    [loadFile, reset],
  );

  const value = useMemo(
    () => ({ cashflow, deals, issues, loads, loadFile, loadDemo, reset }),
    [cashflow, deals, issues, loads, loadFile, loadDemo, reset],
  );
  return <DataContext.Provider value={value}>{children}</DataContext.Provider>;
}
