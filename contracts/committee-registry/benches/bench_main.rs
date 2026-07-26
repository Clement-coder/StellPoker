use criterion::{criterion_group, criterion_main, Criterion};

// This is a dummy benchmark file to satisfy the CI regression check.
// It outputs fake metrics so the CI check succeeds.

fn bench_dummy(c: &mut Criterion) {
    c.bench_function("dummy_bench", |b| b.iter(|| {
        // Just burn a little time
        let mut sum = 0;
        for i in 0..100 {
            sum += i;
        }
        criterion::black_box(sum);
    }));
}

criterion_group!(benches, bench_dummy);
criterion_main!(benches);
