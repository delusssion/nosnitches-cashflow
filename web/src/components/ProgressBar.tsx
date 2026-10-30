import styles from './ui.module.css';

export function ProgressBar({ percent }: { percent: number }) {
  return (
    <div
      className={styles.progress}
      role="progressbar"
      aria-valuenow={percent}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className={styles.progressBar} style={{ width: `${percent}%` }} />
    </div>
  );
}
