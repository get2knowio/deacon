use criterion::{criterion_group, criterion_main, Criterion};

fn bench_hello(c: &mut Criterion) {
    c.bench_function("hello_format", |b| {
        b.iter(|| {
            let s = format!("Hello {}", "world");
            criterion::black_box(s);
        });
    });
}

criterion_group!(benches, bench_hello);
criterion_main!(benches);
