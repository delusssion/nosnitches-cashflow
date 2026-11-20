//! Бенчмарки ядра (B10): одна сделка каждого типа из тестовых данных.
//! Запуск: cargo bench -p cfcore

use std::hint::black_box;
use std::path::Path;

use cfcore::io::{read_deals, read_rates};
use cfcore::{generate, Config, Deal};
use criterion::{criterion_group, criterion_main, Criterion};

fn testdata(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../testdata")
        .join(name)
}

fn bench(c: &mut Criterion) {
    let deals = read_deals(&testdata("deals/test_deals.csv")).expect("testdata/deals");
    let rates = read_rates(&testdata("rates/key_rate.csv")).expect("testdata/rates");
    let cfg = Config::default();
    let by_id =
        |id: &str| -> &Deal { deals.iter().find(|d| d.deal_id == id).expect("сделка") };
    let cases = [
        ("ANN 30 лет (1)", "1"),
        ("RGM (2)", "2"),
        ("SIMPLE график (16)", "16"),
        ("FLOAT cap/floor (22)", "22"),
        ("ANN + субсидия (47)", "47"),
    ];
    let mut g = c.benchmark_group("сделка");
    for (name, id) in cases {
        let deal = by_id(id);
        g.bench_function(name, |b| b.iter(|| generate(black_box(deal), &rates, &cfg)));
    }
    g.finish();
    c.bench_function("55 тестовых сделок", |b| {
        b.iter(|| {
            for d in &deals {
                black_box(generate(d, &rates, &cfg));
            }
        })
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
