import styles from './Page.module.css';

export function PortfolioPage() {
  return (
    <section>
      <h1 className={styles.title}>Портфель</h1>
      <p className={styles.hint}>Загрузите кэшфлоу, чтобы увидеть список сделок.</p>
    </section>
  );
}
