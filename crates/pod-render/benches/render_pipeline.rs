use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use glam::{Vec2, Vec3};
use hecs::{Entity, World};
use pod_core::{ColorRect, Material, Mesh, Parent3D, Sprite, Transform, Transform3D};

use pod_render::renderer::extract_render_state;

fn build_mixed_render_world(world_size: usize, with_hierarchy: bool) -> World {
    let mut world = World::new();

    let mut mesh_entities: Vec<Entity> = Vec::new();

    let sprite_count = world_size / 2;
    let mesh_count = world_size / 3;
    let sprite3d_count = world_size / 6;
    let color_rect_count = world_size / 50;

    for i in 0..sprite_count {
        world.spawn((
            Transform {
                position: Vec2::new((i % 256) as f32, (i / 256) as f32),
                rotation: 0.0,
                scale: Vec2::ONE,
            },
            Sprite {
                texture: format!("bench_sprite_{i}"),
                frame: (i % 16) as u32,
                layer: (i % 12) as i32,
                visible: i % 25 != 0,
                color: [0.2, 0.4, 0.6, 1.0],
            },
        ));
    }

    for i in 0..mesh_count {
        let transform = Transform3D {
            position: Vec3::new((i % 128) as f32, (i / 128) as f32, ((i % 64) as f32) / 4.0),
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: Vec3::ONE,
        };
        let mesh = Mesh {
            asset_id: format!("bench_mesh_{i}"),
            layer: (i % 8) as i32,
            visible: true,
            cast_shadows: i % 2 == 0,
            receive_shadows: true,
        };
        let material = Material {
            asset_id: format!("bench_material_{i}"),
            visible: i % 23 != 0,
            ..Default::default()
        };

        let entity = if with_hierarchy && i % 4 == 0 && i > 0 {
            let parent = mesh_entities[(i / 4 - 1) % mesh_entities.len()];
            world.spawn((
                transform,
                mesh,
                material,
                Parent3D {
                    parent: parent.id() as u64,
                },
            ))
        } else {
            world.spawn((transform, mesh, material))
        };

        mesh_entities.push(entity);
    }

    for i in 0..sprite3d_count {
        world.spawn((
            Transform3D {
                position: Vec3::new((i % 96) as f32, (i / 96) as f32, ((i % 32) as f32) * 0.5),
                rotation: [0.0, 0.0, 0.0, 1.0],
                scale: Vec3::ONE,
            },
            Sprite {
                texture: format!("bench_sprite3d_{i}"),
                frame: (i % 8) as u32,
                layer: (i % 10) as i32,
                visible: true,
                color: [0.8, 0.7, 0.2, 1.0],
            },
        ));
    }

    for i in 0..color_rect_count {
        let size = (i % 5) as f32 + 1.0;
        world.spawn((
            ColorRect::new(size, size + 0.5, [0.1, 0.1, 0.1, 1.0]),
            Transform {
                position: Vec2::new((i % 64) as f32, (i / 64) as f32),
                rotation: 0.0,
                scale: Vec2::ONE,
            },
        ));
    }

    world
}

fn bench_render_state_extraction(c: &mut Criterion) {
    let flat_world = build_mixed_render_world(9_000, false);
    let hierarchical_world = build_mixed_render_world(9_000, true);

    let mut group = c.benchmark_group("render_state_extract");
    group.throughput(Throughput::Elements(9_000));

    group.bench_function("mixed_flat", |bench| {
        bench.iter(|| {
            let state = extract_render_state(&flat_world);
            black_box(state);
        });
    });

    group.bench_function("mixed_hierarchy", |bench| {
        bench.iter(|| {
            let state = extract_render_state(&hierarchical_world);
            black_box(state);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_render_state_extraction);
criterion_main!(benches);
