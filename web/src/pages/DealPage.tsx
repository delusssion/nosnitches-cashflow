import styles from './Page.module.css';

export function DealPage() {
  return (
    <section>
      <h1 className={styles.title}>Сделка</h1>
      <p className={styles.hint}>Выберите сделку на экране «Портфель».</p>
    </section>
  );
}
