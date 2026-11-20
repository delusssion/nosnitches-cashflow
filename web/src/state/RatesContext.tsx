import { createContext, useCallback, useMemo, useState, type ReactNode } from 'react';
import { parseRates, type RateFile } from '../rates/rates';

export interface LoadedRates extends RateFile {
  fileName: string;
}

export interface RatesState {
  /** История индикатора (формат 2 контракта). */
  history: LoadedRates | null;
  /** Кривые команды 2 (базовый и стресс-сценарии). */
  curves: LoadedRates | null;
  reportDate: string;
  shiftBp: number;
  setReportDate: (d: string) => void;
  setShiftBp: (bp: number) => void;
  loadHistory: (blob: Blob, fileName: string) => Promise<void>;
  loadCurves: (blob: Blob, fileName: string) => Promise<void>;
}

// eslint-disable-next-line react-refresh/only-export-components
export const RatesContext = createContext<RatesState | null>(null);

export function RatesProvider({ children }: { children: ReactNode }) {
  const [history, setHistory] = useState<LoadedRates | null>(null);
  const [curves, setCurves] = useState<LoadedRates | null>(null);
  const [reportDate, setReportDate] = useState('2022-08-31');
  const [shiftBp, setShiftBp] = useState(0);

  // Файлы ставок маленькие (тысячи строк) — разбираем в основном потоке.
  const loadHistory = useCallback(async (blob: Blob, fileName: string) => {
    setHistory({ ...parseRates(await blob.text()), fileName });
  }, []);
  const loadCurves = useCallback(async (blob: Blob, fileName: string) => {
    setCurves({ ...parseRates(await blob.text()), fileName });
  }, []);

  const value = useMemo(
    () => ({
      history,
      curves,
      reportDate,
      shiftBp,
      setReportDate,
      setShiftBp,
      loadHistory,
      loadCurves,
    }),
    [history, curves, reportDate, shiftBp, loadHistory, loadCurves],
  );
  return <RatesContext.Provider value={value}>{children}</RatesContext.Provider>;
}
