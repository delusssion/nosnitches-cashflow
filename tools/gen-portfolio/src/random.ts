/** mulberry32 — быстрый seedable PRNG: один seed даёт одну и ту же последовательность. */
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export class Rng {
  private next: () => number;
  constructor(seed: number) {
    this.next = mulberry32(seed);
  }
  float(): number {
    return this.next();
  }
  /** Равномерно в [min, max]. */
  uniform(min: number, max: number): number {
    return min + (max - min) * this.next();
  }
  int(min: number, max: number): number {
    return Math.floor(this.uniform(min, max + 1));
  }
  chance(p: number): boolean {
    return this.next() < p;
  }
  pick<T>(items: readonly T[]): T {
    return items[Math.floor(this.next() * items.length)]!;
  }
  /** Выбор по весам: [[значение, вес], ...]. */
  weighted<T>(items: readonly (readonly [T, number])[]): T {
    let r = this.next() * items.reduce((s, [, w]) => s + w, 0);
    for (const [v, w] of items) {
      r -= w;
      if (r < 0) return v;
    }
    return items.at(-1)![0];
  }
  /** Стандартное нормальное (Бокс — Мюллер). */
  normal(): number {
    const u = 1 - this.next();
    const v = this.next();
    return Math.sqrt(-2 * Math.log(u)) * Math.cos(2 * Math.PI * v);
  }
}
