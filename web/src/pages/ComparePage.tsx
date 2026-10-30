import styles from './Page.module.css';

export function ComparePage() {
  return (
    <section>
      <h1 className={styles.title}>Сверка</h1>
      <p className={styles.hint}>Сравнение результата движка с эталоном.</p>
    </section>
  );
}
