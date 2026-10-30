import { useRef, useState, type DragEvent } from 'react';
import styles from './ui.module.css';

interface Props {
  title: string;
  hint?: string;
  fileName?: string;
  multiple?: boolean;
  onFiles: (files: File[]) => void;
}

export function DropZone({ title, hint, fileName, multiple, onFiles }: Props) {
  const [active, setActive] = useState(false);
  const input = useRef<HTMLInputElement>(null);

  const onDrop = (e: DragEvent) => {
    e.preventDefault();
    setActive(false);
    const files = Array.from(e.dataTransfer.files);
    if (files.length) onFiles(multiple ? files : files.slice(0, 1));
  };

  return (
    <div
      role="button"
      tabIndex={0}
      className={active ? `${styles.drop} ${styles.dropActive}` : styles.drop}
      onClick={() => input.current?.click()}
      onKeyDown={(e) => (e.key === 'Enter' || e.key === ' ') && input.current?.click()}
      onDragOver={(e) => {
        e.preventDefault();
        setActive(true);
      }}
      onDragLeave={() => setActive(false)}
      onDrop={onDrop}
    >
      <div className={styles.dropTitle}>{title}</div>
      {hint && <div>{hint}</div>}
      {fileName && <div className={styles.dropFile}>{fileName}</div>}
      <input
        ref={input}
        type="file"
        accept=".csv,text/csv"
        multiple={multiple}
        hidden
        onChange={(e) => {
          const files = Array.from(e.target.files ?? []);
          if (files.length) onFiles(files);
          e.target.value = '';
        }}
      />
    </div>
  );
}
