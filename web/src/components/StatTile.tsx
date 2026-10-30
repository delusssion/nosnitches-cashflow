import styles from './ui.module.css';

interface Props {
  label: string;
  value: string;
  danger?: boolean;
}

export function StatTile({ label, value, danger }: Props) {
  return (
    <div className={danger ? styles.statDanger : styles.stat}>
      <div className={styles.statValue} title={value}>
        {value}
      </div>
      <div className={styles.statLabel}>{label}</div>
    </div>
  );
}
