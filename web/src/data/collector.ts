import { COLUMNS, parseRow } from './schemas';
import type { CsvKind, RowTypes, ValidationError } from './types';

/** Сколько ошибок храним; остальные только считаем. */
export const MAX_STORED_ERRORS = 1000;

export interface ParseResult<K extends CsvKind> {
  rows: RowTypes[K][];
  errors: ValidationError[];
  /** Всего ошибок, включая не попавшие в errors. */
  errorCount: number;
  /** Всего строк данных в файле. */
  rowCount: number;
}

/** Копит провалидированные строки и ошибки. Используется и в Web Worker, и в синхронном разборе. */
export function createCollector<K extends CsvKind>(kind: K) {
  const result: ParseResult<K> = { rows: [], errors: [], errorCount: 0, rowCount: 0 };
  const required = Object.keys(COLUMNS[kind]);

  const addError = (e: ValidationError) => {
    result.errorCount += 1;
    if (result.errors.length < MAX_STORED_ERRORS) result.errors.push(e);
  };

  return {
    result,
    /** Проверяет заголовок; false — файл не того формата, строки дальше не разбираются. */
    checkHeader(fields: string[]): boolean {
      const missing = required.filter((c) => !fields.includes(c));
      for (const field of missing) addError({ row: 0, field, message: 'нет колонки в заголовке' });
      return missing.length === 0;
    },
    /** Добавляет строки, возвращает только валидные из этой порции. */
    push(rawRows: Record<string, string | undefined>[]): RowTypes[K][] {
      const valid: RowTypes[K][] = [];
      for (const raw of rawRows) {
        result.rowCount += 1;
        const res = parseRow(kind, raw, result.rowCount);
        if (res.ok) {
          valid.push(res.value);
          result.rows.push(res.value);
        } else {
          res.errors.forEach(addError);
        }
      }
      return valid;
    },
    addError,
  };
}
