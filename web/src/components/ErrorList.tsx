import type { ValidationError } from '../data/types';
import { fmtInt } from '../format';
import styles from './ui.module.css';

const SHOWN = 100;

export function ErrorList({ errors, total }: { errors: ValidationError[]; total: number }) {
  if (total === 0) return null;
  return (
    <div>
      <strong className={styles.warn}>Ошибки валидации: {fmtInt(total)}</strong>
      {total > SHOWN && <span className={styles.muted}> (показаны первые {SHOWN})</span>}
      <ul className={styles.errorList}>
        {errors.slice(0, SHOWN).map((e, i) => (
          <li key={i}>
            {e.row === 0 ? 'заголовок' : `строка ${e.row}`}
            {e.field && ` · ${e.field}`}: {e.message}
          </li>
        ))}
      </ul>
    </div>
  );
}
