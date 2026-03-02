use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};

use pod_net::{SpacetimeDBClient, SpacetimeDBClientConfig};

fn bench_player_interest_queries(c: &mut Criterion) {
    let client_config = SpacetimeDBClientConfig::default();
    let mut client = SpacetimeDBClient::new(client_config);

    let mut group = c.benchmark_group("player_interest_queries");
    group.throughput(Throughput::Elements(1));

    group.bench_function("single_partition_radius_200", |bench| {
        bench.iter(|| {
            let result = client.subscribe_for_player_with_interest(
                black_box(1001),
                black_box(1000.0),
                black_box(1000.0),
                black_box(200.0),
            );
            assert!(black_box(result).is_ok());
        });
    });

    group.bench_function("partition_size_50_radius_200", |bench| {
        bench.iter(|| {
            let result = client.subscribe_for_player_with_interest_partitioned(
                1001,
                black_box(1000.0),
                black_box(1000.0),
                black_box(200.0),
                black_box(50.0),
            );
            assert!(black_box(result).is_ok());
        });
    });

    group.bench_function("partition_size_25_radius_200", |bench| {
        bench.iter(|| {
            let result = client.subscribe_for_player_with_interest_partitioned(
                1001,
                black_box(1000.0),
                black_box(1000.0),
                black_box(200.0),
                black_box(25.0),
            );
            assert!(black_box(result).is_ok());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_player_interest_queries);
criterion_main!(benches);
