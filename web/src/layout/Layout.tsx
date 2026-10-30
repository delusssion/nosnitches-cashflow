import { NavLink, Outlet } from 'react-router';
import styles from './Layout.module.css';

const NAV = [
  { to: '/', label: 'Портфель', end: true },
  { to: '/deal', label: 'Сделка', end: false },
  { to: '/compare', label: 'Сверка', end: false },
];

export function Layout() {
  return (
    <div className={styles.shell}>
      <header className={styles.header}>
        <span className={styles.brand}>
          nosnitches <span className={styles.brandMuted}>· cashflow</span>
        </span>
        <nav className={styles.nav}>
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className={({ isActive }) =>
                isActive ? `${styles.link} ${styles.active}` : styles.link
              }
            >
              {item.label}
            </NavLink>
          ))}
        </nav>
      </header>
      <main className={styles.content}>
        <Outlet />
      </main>
    </div>
  );
}
