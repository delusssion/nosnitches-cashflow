import {
  flexRender,
  getCoreRowModel,
  getSortedRowModel,
  useReactTable,
  type ColumnDef,
  type SortingState,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useRef, useState, type CSSProperties, type ReactNode } from 'react';
import styles from './VirtualTable.module.css';

export interface ColumnMeta {
  /** Ширина колонки в CSS grid, напр. '120px' или 'minmax(120px, 1fr)'. */
  width?: string;
  align?: 'right';
  /** Подвал колонки (итоги). */
  footer?: ReactNode;
}

interface Props<T> {
  data: T[];
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  columns: ColumnDef<T, any>[];
  height?: number | string;
  rowHeight?: number;
  onRowClick?: (row: T) => void;
  rowStyle?: (row: T) => CSSProperties | undefined;
  initialSorting?: SortingState;
  empty?: string;
  showFooter?: boolean;
}

/** Таблица с виртуализацией строк: рисуются только видимые, поэтому 100 000+ строк скроллятся плавно. */
export function VirtualTable<T>({
  data,
  columns,
  height = 480,
  rowHeight = 34,
  onRowClick,
  rowStyle,
  initialSorting = [],
  empty = 'Нет данных',
  showFooter,
}: Props<T>) {
  const [sorting, setSorting] = useState<SortingState>(initialSorting);
  // eslint-disable-next-line react-hooks/incompatible-library
  const table = useReactTable({
    data,
    columns,
    state: { sorting },
    onSortingChange: setSorting,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });
  const rows = table.getRowModel().rows;
  const scroller = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });

  const leaf = table.getVisibleLeafColumns();
  const template = leaf.map((c) => (c.columnDef.meta as ColumnMeta)?.width ?? '1fr').join(' ');
  const cellClass = (meta: ColumnMeta | undefined, base: string | undefined) =>
    meta?.align === 'right' ? `${base} ${styles.right}` : base;

  return (
    <div ref={scroller} className={styles.wrap} style={{ height }}>
      <div className={styles.table} role="table">
        <div className={styles.head} style={{ gridTemplateColumns: template }} role="row">
          {table.getFlatHeaders().map((h) => {
            const meta = h.column.columnDef.meta as ColumnMeta | undefined;
            const sort = h.column.getIsSorted();
            return (
              <div
                key={h.id}
                role="columnheader"
                className={cellClass(
                  meta,
                  h.column.getCanSort() ? `${styles.th} ${styles.sortable}` : styles.th,
                )}
                onClick={h.column.getToggleSortingHandler()}
              >
                {flexRender(h.column.columnDef.header, h.getContext())}
                {sort === 'asc' ? ' ▲' : sort === 'desc' ? ' ▼' : ''}
              </div>
            );
          })}
        </div>
        {rows.length === 0 ? (
          <div className={styles.empty}>{empty}</div>
        ) : (
          <div style={{ height: virtualizer.getTotalSize(), position: 'relative' }}>
            {virtualizer.getVirtualItems().map((v) => {
              const row = rows[v.index]!;
              return (
                <div
                  key={row.id}
                  role="row"
                  className={onRowClick ? `${styles.row} ${styles.clickable}` : styles.row}
                  style={{
                    gridTemplateColumns: template,
                    height: rowHeight,
                    transform: `translateY(${v.start}px)`,
                    ...rowStyle?.(row.original),
                  }}
                  onClick={onRowClick && (() => onRowClick(row.original))}
                >
                  {row.getVisibleCells().map((cell) => (
                    <div
                      key={cell.id}
                      role="cell"
                      className={cellClass(
                        cell.column.columnDef.meta as ColumnMeta | undefined,
                        styles.td,
                      )}
                    >
                      {flexRender(cell.column.columnDef.cell, cell.getContext())}
                    </div>
                  ))}
                </div>
              );
            })}
          </div>
        )}
        {showFooter && rows.length > 0 && (
          <div className={styles.foot} style={{ gridTemplateColumns: template }}>
            {leaf.map((c) => {
              const meta = c.columnDef.meta as ColumnMeta | undefined;
              return (
                <div key={c.id} className={cellClass(meta, styles.td)}>
                  {meta?.footer}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
