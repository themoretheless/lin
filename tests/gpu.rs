#![cfg(feature = "gpu")]
#[path = "../examples/gpu_check.rs"]
mod verification;

#[test]
#[ignore = "requires a hardware GPU; run with --features gpu --test gpu -- --ignored"]
fn hardware_cosine_and_search_match_cpu() {
    verification::main().unwrap();
}

#[cfg(feature = "async")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires a hardware GPU"]
async fn compute_async_queries_and_streams_share_backend_safely() {
    use futures_util::StreamExt;
    use lin::{AsyncDb, Db, Queryable};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::fixture();
    let query = Queryable::from("docs")
        .filter("title ~ /wal/i or wing == \"rag\"")
        .select(["title", "wing"])
        .take_all();
    let expected = query.to_vec(&mut cpu).unwrap();
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let mut db = Db::fixture().with_gpu(gpu.clone());
        db.store = cpu.store.clone_mem();
        let db = AsyncDb::new(db);
        let query = query.clone();
        let expected = expected.clone();
        tasks.push(tokio::spawn(async move {
            assert_eq!(query.clone().to_vec_async(&db).await.unwrap(), expected);
            let mut stream = query.to_stream_async(&db).await;
            let mut rows = Vec::new();
            while let Some(row) = stream.next().await {
                rows.push(row.unwrap());
            }
            assert_eq!(rows, expected);
        }));
    }
    for task in tasks {
        task.await.unwrap();
    }
    assert!(gpu.regex_dispatches() >= 16);
    assert!(gpu.text_dispatches() >= 16);
}

#[test]
#[ignore = "requires a hardware GPU"]
fn gpu_hop_cursor_preserves_mixed_primary_and_doc_rows() {
    use lin::{Db, Queryable};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::fixture();
    let mut edge = cpu.store.edges[0].clone();
    edge.rel = "wikilink".into();
    edge.from = "u1".into();
    edge.to = "u2".into();
    cpu.store.edges = vec![edge.clone()];
    edge.to = cpu.store.collection("docs")[0]["id"]
        .text()
        .unwrap()
        .to_owned();
    cpu.store.edges.push(edge);
    let mut db = Db::fixture().with_gpu(gpu.clone());
    db.store = cpu.store.clone_mem();
    let query = Queryable::from("users")
        .filter("id == \"u1\"")
        .hop("wikilink")
        .take_all();
    let expected = query.to_vec(&mut cpu).unwrap();
    assert_eq!(expected.len(), 2);
    assert_eq!(query.to_vec(&mut db).unwrap(), expected);
    assert_eq!(
        query
            .cursor(&db)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        expected
    );
    assert_eq!(
        query
            .cursor_read(&db.reader())
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        expected
    );
    assert!(gpu.graph_dispatches() >= 4);
}

#[test]
#[ignore = "requires a hardware GPU"]
fn arbitrary_wgsl_resident_dispatch_and_errors() {
    use lin::gpu::{GpuCompute, native as wgpu};
    let gpu = GpuCompute::new().unwrap();
    let source = r#"
        @group(0) @binding(0) var<storage, read_write> values: array<u32>;
        @compute @workgroup_size(4, 2, 1)
        fn increment(@builtin(global_invocation_id) id: vec3<u32>) {
            let i = id.x + id.y * 8u + id.z * 32u;
            if i < arrayLength(&values) { values[i] += 3u; }
        }
        @compute @workgroup_size(64)
        fn double(@builtin(global_invocation_id) id: vec3<u32>) {
            if id.x < arrayLength(&values) { values[id.x] *= 2u; }
        }
    "#;
    let first = gpu.compile_wgsl(source, "increment").unwrap();
    let second = gpu.compile_wgsl(source, "double").unwrap();
    assert!(gpu.bind_group_layout(&first, 0).is_ok());
    assert!(gpu.bind_group_layout(&first, 1).is_err());
    assert!(gpu.bind_group_layout(&first, u32::MAX).is_err());
    let buffer = gpu
        .storage_buffer(bytemuck::cast_slice(&vec![1u32; 64]))
        .unwrap();
    let group = |kernel: &lin::gpu::ComputeKernel| {
        gpu.checked(|device, _| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &kernel.bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            })
        })
        .unwrap()
    };
    let g1 = group(&first);
    let g2 = group(&second);
    gpu.dispatch(&first, &[&g1], [2, 2, 2]).unwrap();
    gpu.dispatch(&second, &[&g2], [1, 1, 1]).unwrap();
    let bytes = gpu.read_buffer(&buffer).unwrap();
    assert_eq!(bytemuck::cast_slice::<u8, u32>(&bytes), &[8u32; 64]);
    gpu.checked(|_, queue| queue.write_buffer(&buffer, 0, bytemuck::cast_slice(&[7u32])))
        .unwrap();
    gpu.dispatch(&second, &[&g2], [1, 1, 1]).unwrap();
    let bytes = gpu.read_buffer(&buffer).unwrap();
    assert_eq!(bytemuck::cast_slice::<u8, u32>(&bytes)[0], 14);
    assert!(gpu.compile_wgsl("invalid WGSL", "main").is_err());
    assert!(gpu.compile_wgsl(source, "missing").is_err());
    assert!(gpu.dispatch(&first, &[], [1, 1, 1]).is_err());
    assert!(gpu.dispatch(&first, &[&g1], [u32::MAX, 1, 1]).is_err());
    assert!(gpu.storage_buffer(&[]).is_err());
    assert!(gpu.storage_buffer(&[1, 2, 3]).is_err());
    // An error must not poison subsequent submissions/scopes.
    gpu.dispatch(&second, &[&g2], [1, 1, 1]).unwrap();
    assert_eq!(
        bytemuck::cast_slice::<u8, u32>(&gpu.read_buffer(&buffer).unwrap())[0],
        28
    );
}

#[test]
#[ignore = "requires a hardware GPU"]
fn storage_texture_uniform_and_multiple_groups() {
    use lin::gpu::{GpuCompute, native as wgpu};
    use wgpu::util::DeviceExt;
    let gpu = GpuCompute::new().unwrap();
    let kernel = gpu
        .compile_wgsl(
            r#"
        @group(0) @binding(0) var image: texture_storage_2d<rgba8unorm, write>;
        @group(1) @binding(0) var<uniform> color: vec4<f32>;
        @compute @workgroup_size(1)
        fn paint(@builtin(global_invocation_id) id: vec3<u32>) {
            textureStore(image, vec2<i32>(id.xy), color);
        }
    "#,
            "paint",
        )
        .unwrap();
    let (texture, uniform) = gpu
        .checked(|device, _| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: 2,
                    height: 2,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&[1f32, 0.0, 0.0, 1.0]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            (texture, uniform)
        })
        .unwrap();
    let view = texture.create_view(&Default::default());
    let g0 = gpu
        .checked(|device, _| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &kernel.bind_group_layout(0),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                }],
            })
        })
        .unwrap();
    let g1 = gpu
        .checked(|device, _| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &kernel.bind_group_layout(1),
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            })
        })
        .unwrap();
    gpu.dispatch(&kernel, &[&g0, &g1], [2, 2, 1]).unwrap();
    let output = gpu
        .checked(|device, queue| {
            let output = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: 512,
                mapped_at_creation: false,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            });
            let mut encoder = device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &output,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(256),
                        rows_per_image: Some(2),
                    },
                },
                wgpu::Extent3d {
                    width: 2,
                    height: 2,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit([encoder.finish()]);
            output
        })
        .unwrap();
    let bytes = gpu.read_buffer(&output).unwrap();
    for offset in [0, 4, 256, 260] {
        assert_eq!(&bytes[offset..offset + 4], &[255, 0, 0, 255]);
    }
}

#[test]
#[ignore = "requires a hardware GPU"]
fn executor_resident_cache_tracks_mutations_rollback_and_snapshot() {
    use lin::{Cell, Db};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut db = Db::fixture().with_gpu(gpu);
    let q = r#"docs | search vec "wal" | take all"#;
    let check = |db: &mut Db| {
        let mut cpu = Db::empty();
        cpu.catalog = db.catalog.clone();
        cpu.store = db.store.clone_mem();
        assert_eq!(cpu.run(q).unwrap().rows, db.run(q).unwrap().rows);
        let hybrid = r#"docs | search "wal" | take all"#;
        assert_eq!(cpu.run(hybrid).unwrap().rows, db.run(hybrid).unwrap().rows);
    };
    check(&mut db);
    assert_eq!(db.gpu_cache_stats().unwrap().uploads, 1);
    assert_eq!(db.gpu_cache_stats().unwrap().hits, 1);
    let snapshot = db.reader();
    let snapshot_rows = snapshot.run(q).unwrap().rows;
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let read = snapshot.clone();
                scope.spawn(move || read.run(q).unwrap().rows)
            })
            .collect();
        for handle in handles {
            assert_eq!(handle.join().unwrap(), snapshot_rows);
        }
    });
    assert_eq!(snapshot.gpu_cache_stats().unwrap().uploads, 1);
    assert_eq!(snapshot.gpu_cache_stats().unwrap().hits, 4);

    db.run(r#"insert docs { uri: "raw://gpu-cache", title: "wal", layer: "wiki" }"#)
        .unwrap();
    check(&mut db);
    assert_eq!(db.gpu_cache_stats().unwrap().uploads, 2);
    db.run(r#"update docs[uri == "raw://gpu-cache"] cas each { title: "cats" }"#)
        .unwrap();
    check(&mut db);
    assert_eq!(snapshot.run(q).unwrap().rows, snapshot_rows);
    db.run(r#"delete docs[uri == "raw://gpu-cache"] cas each"#)
        .unwrap();
    check(&mut db);
    // Warm a cache with uncommitted inserted data; failure must restore old results.
    assert!(
        db.run(
            r#"insert docs { uri: "raw://gpu-rollback", title: "wal", layer: "wiki" }
        docs | search vec "wal"
        update docs[id == "missing"] cas "x" { room: "inbox" }"#
        )
        .is_err()
    );
    check(&mut db);
    assert_eq!(snapshot.run(q).unwrap().rows, snapshot_rows);
    // Public Store mutation and Arc::make_mut must invalidate even without a gen change.
    let old_uploads = db.gpu_cache_stats().unwrap().uploads;
    if let Some(Cell::Vec(v)) =
        db.store.collections.get_mut("docs").unwrap()[0].get_mut("embedding")
    {
        Arc::make_mut(v).fill(0.0);
    }
    check(&mut db);
    assert_eq!(db.gpu_cache_stats().unwrap().uploads, old_uploads + 1);
    db.reembed_collection("docs").unwrap();
    check(&mut db);
    assert_eq!(db.gpu_cache_stats().unwrap().uploads, old_uploads + 2);
    let backend = db.gpu().unwrap().clone();
    db.disable_gpu();
    assert!(db.gpu().is_none());
    assert_eq!(db.gpu_cache_stats().unwrap(), Default::default());
    check(&mut db);
    db.enable_gpu(backend);
    check(&mut db);
    assert_eq!(db.gpu_cache_stats().unwrap().uploads, 1);
}

#[test]
#[ignore = "requires a hardware GPU"]
fn numeric_filters_preserve_f64_i64_special_values_and_batch() {
    use lin::{Cell, CmpOp, Db, Field, Pred, Queryable, Value};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::empty();
    cpu.run("col metrics { value: f64 }").unwrap();
    let values = [
        Cell::Float(f64::NAN),
        Cell::Float(f64::INFINITY),
        Cell::Float(f64::NEG_INFINITY),
        Cell::Float(-0.0),
        Cell::Float(0.0),
        Cell::Float(f64::from_bits(1)),
        Cell::Float(-f64::from_bits(1)),
        Cell::Float(1.0000000000000002),
        Cell::Int(9_007_199_254_740_993),
        Cell::Int(9_007_199_254_740_992),
        Cell::Int(i64::MIN),
        Cell::Int(i64::MAX),
        Cell::Null,
    ];
    for _ in &values {
        cpu.run("insert metrics { value: 1.0 }").unwrap();
    }
    for (row, value) in cpu
        .store
        .collections
        .get_mut("metrics")
        .unwrap()
        .iter_mut()
        .zip(values)
    {
        row.insert("value".into(), value);
    }
    let mut accelerated = Db::empty().with_gpu(gpu.clone());
    accelerated.catalog = cpu.catalog.clone();
    accelerated.store = cpu.store.clone_mem();
    for op in [
        CmpOp::Eq,
        CmpOp::Ne,
        CmpOp::Gt,
        CmpOp::Lt,
        CmpOp::Ge,
        CmpOp::Le,
    ] {
        for value in [
            Value::Int(9_007_199_254_740_993),
            Value::Int(i64::MIN),
            Value::Int(i64::MAX),
            Value::Float(-0.0),
            Value::Float(f64::from_bits(1)),
            Value::Float(1.0000000000000002),
            Value::Float(f64::NAN),
            Value::Float(f64::INFINITY),
        ] {
            let pred = Pred::Cmp {
                field: Field::name("value"),
                op,
                value,
            };
            let query = Queryable::from("metrics")
                .filter(pred.clone())
                .select(["id"])
                .take_all();
            assert_eq!(
                query.to_vec(&mut cpu).unwrap(),
                query.to_vec(&mut accelerated).unwrap(),
                "{pred:?}"
            );
            let query = Queryable::from("metrics").filter(pred).count();
            assert_eq!(
                query.to_vec(&mut cpu).unwrap(),
                query.to_vec(&mut accelerated).unwrap()
            );
        }
    }
    for q in [
        "metrics | value > 0 and value < 2 | { id } | take all",
        "metrics | value < 0 or value > 2 | { id } | take all",
        "metrics | value >= 0 | { id } | skip 1 | take 2",
        "metrics | value >= 0 | { id } | take all",
    ] {
        assert_eq!(cpu.run(q).unwrap().rows, accelerated.run(q).unwrap().rows);
        assert_eq!(cpu.run_batch(q).unwrap(), accelerated.run_batch(q).unwrap());
        assert_eq!(
            cpu.run_batch(q).unwrap(),
            accelerated.reader().run_batch(q).unwrap()
        );
    }
    assert!(
        gpu.comparison_dispatches() >= 100,
        "executor must submit actual GPU comparisons"
    );
}

#[test]
#[ignore = "requires a hardware GPU"]
fn insufficient_comparison_buffer_limit_returns_error() {
    use lin::gpu::{GpuCompute, GpuOptions};
    use std::sync::Arc;
    let mut options = GpuOptions::default();
    options.limits.max_storage_buffer_binding_size = 16;
    let gpu = Arc::new(GpuCompute::with_options(options).unwrap());
    let mut db = lin::Db::fixture().with_gpu(gpu);
    let error = db.run("orders | total > 1 | count").unwrap_err();
    assert!(error.to_string().contains("comparison record"), "{error}");
    let error = db.run("docs | sort title | { id } | take 5").unwrap_err();
    assert!(error.to_string().contains("sort keys"), "{error}");

    let mut options = GpuOptions::default();
    options.limits.max_compute_workgroups_per_dimension = u32::MAX;
    assert!(GpuCompute::with_options(options).is_err());
}

#[test]
#[ignore = "requires a hardware GPU"]
fn text_filters_match_cpu_unicode_empty_null_and_mixed_predicates() {
    use lin::{Cell, CmpOp, Db, Field, Pred, Queryable, Value};
    use std::sync::Arc;
    let mut options = lin::gpu::GpuOptions::default();
    options.limits.max_storage_buffer_binding_size = 256;
    let gpu = Arc::new(lin::gpu::GpuCompute::with_options(options).unwrap());
    let mut cpu = Db::empty();
    cpu.run("col samples { text: text, n: i64, active: bool }")
        .unwrap();
    let values = [
        Some(""),
        Some("foo bar"),
        Some("foobar"),
        Some("foo_bar"),
        Some("Привет МИР"),
        Some("İstanbul Σσς"),
        Some("café cafe\u{301}"),
        Some("ＡＢＣ 😀 foo"),
        Some("nul\0byte"),
        Some("foo-bar foo"),
        None,
    ];
    for (i, _) in values.iter().enumerate() {
        cpu.run(&format!(
            "insert samples {{ text: \"placeholder\", n: {i}, active: true }}"
        ))
        .unwrap();
    }
    for (row, value) in cpu
        .store
        .collections
        .get_mut("samples")
        .unwrap()
        .iter_mut()
        .zip(values)
    {
        row.insert(
            "text".into(),
            value.map(Cell::text_arc).unwrap_or(Cell::Null),
        );
    }
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    for needle in [
        "",
        "foo",
        "bar",
        "foo bar",
        "foo_bar",
        "ПРИВЕТ",
        "привет",
        "мир",
        "i\u{307}stanbul",
        "Σ",
        "σ",
        "ς",
        "café",
        "cafe",
        "😀",
        "\0",
    ] {
        for pred in [
            Pred::Contains {
                field: Field::name("text"),
                needle: needle.into(),
            },
            Pred::Has {
                field: Field::name("text"),
                ci: false,
                needle: needle.into(),
            },
            Pred::Has {
                field: Field::name("text"),
                ci: true,
                needle: needle.into(),
            },
            Pred::Cmp {
                field: Field::name("text"),
                op: CmpOp::Eq,
                value: Value::String(needle.into()),
            },
            Pred::Cmp {
                field: Field::name("text"),
                op: CmpOp::Ne,
                value: Value::String(needle.into()),
            },
        ] {
            let q = Queryable::from("samples")
                .filter(pred.clone())
                .select(["id"])
                .take_all();
            assert_eq!(
                q.to_vec(&mut cpu).unwrap(),
                q.to_vec(&mut db).unwrap(),
                "{pred:?}"
            );
        }
    }
    for program in [
        r#"samples | text ~ "foo" and n > 1 | { id } | take all"#,
        r#"samples | text has "foo" or n == 0 | { id } | take all"#,
        r#"samples | text ~ /foo/ and n >= 2 | { id } | take all"#,
        r#"samples | active == true and text != "" | { id } | take all"#,
        r#"samples | active != false | count"#,
        r#"samples | text ~ "" | { id } | skip 2 | take 5"#,
    ] {
        assert_eq!(
            cpu.run(program).unwrap().rows,
            db.run(program).unwrap().rows,
            "{program}"
        );
        assert_eq!(
            cpu.run_batch(program).unwrap(),
            db.run_batch(program).unwrap(),
            "batch {program}"
        );
        assert_eq!(
            cpu.run_batch(program).unwrap(),
            db.reader().run_batch(program).unwrap(),
            "reader {program}"
        );
    }
    assert!(gpu.text_dispatches() >= 75);
}

#[test]
#[ignore = "requires a hardware GPU"]
fn grouped_count_preserves_keys_empty_input_filters_and_batches() {
    use lin::{Cell, Db};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::empty();
    cpu.run("col groups { key: text, n: i64 }").unwrap();
    for i in 0..513 {
        cpu.run(&format!("insert groups {{ key: \"k{}\", n: {i} }}", i % 7))
            .unwrap();
    }
    for (i, row) in cpu
        .store
        .collections
        .get_mut("groups")
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        if i % 31 == 0 {
            row.insert("key".into(), Cell::Null);
        }
        if i % 17 == 0 {
            row.insert("key".into(), Cell::text_arc("Привет \"quoted\""));
        }
    }
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    for q in [
        "groups | count",
        "groups | count by key",
        "groups | count by n",
        "groups | n < 0 | count",
        "groups | n < 0 | count by key",
        "groups | n > 100 | count by key | sort hits desc | take 3",
        r#"groups | key ~ "k" and n <= 200 | count by key"#,
        r#"groups | key ~ /k[1-3]/ | count by key"#,
        "groups | count by key | count",
        "groups | count by key | hits > 60 | { key } | take all",
    ] {
        assert_eq!(cpu.run(q).unwrap().rows, db.run(q).unwrap().rows, "{q}");
        assert_eq!(
            cpu.run_batch(q).unwrap(),
            db.run_batch(q).unwrap(),
            "batch {q}"
        );
        assert_eq!(
            cpu.run(q).unwrap().rows,
            db.reader().run(q).unwrap().rows,
            "reader {q}"
        );
    }
    assert!(gpu.count_dispatches() >= 20);
}

#[test]
#[ignore = "requires a hardware GPU"]
fn joins_match_cpu_duplicates_missing_rows_tiles_and_batches() {
    use lin::{Cell, Db};
    use std::sync::Arc;
    let mut options = lin::gpu::GpuOptions::default();
    options.limits.max_storage_buffer_binding_size = 256;
    let gpu = Arc::new(lin::gpu::GpuCompute::with_options(options).unwrap());
    let mut cpu = Db::empty();
    cpu.run(
        "col parents { key: text, label: text }
        col children { parent: text, n: i64 }
        fk children.parent -> parents.key",
    )
    .unwrap();
    for i in 0..13 {
        cpu.run(&format!(
            "insert parents {{ key: \"k{}\", label: \"p{i}\" }}",
            i % 3
        ))
        .unwrap();
    }
    for i in 0..71 {
        cpu.run(&format!(
            "insert children {{ parent: \"k{}\", n: {i} }}",
            i % 3
        ))
        .unwrap();
    }
    for (i, row) in cpu
        .store
        .collections
        .get_mut("children")
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        if i % 11 == 0 {
            row.insert("parent".into(), Cell::Null);
        }
        if i % 17 == 0 {
            row.insert("parent".into(), Cell::text_arc("missing"));
        }
    }
    let check = |cpu: &mut Db| {
        let mut db = Db::empty().with_gpu(gpu.clone());
        db.catalog = cpu.catalog.clone();
        db.store = cpu.store.clone_mem();
        for q in [
            "children | join parents on parent | { n, parents.label } | take all",
            "children | join left parents on parent | { n, parents.label } | take all",
            "children | n > 20 | join parents on parent | { n, parents.label } | skip 5 | take 20",
            "children | join parents on parent | n > 60 | { n, parents.label } | take all",
            "children | join left parents on parent | count by parent",
            "children | n < 0 | join parents on parent | { n, parents.label } | take all",
        ] {
            assert_eq!(cpu.run(q).unwrap().rows, db.run(q).unwrap().rows, "{q}");
            assert_eq!(
                cpu.run_batch(q).unwrap(),
                db.run_batch(q).unwrap(),
                "batch {q}"
            );
            assert_eq!(
                cpu.run(q).unwrap().rows,
                db.reader().run(q).unwrap().rows,
                "reader {q}"
            );
        }
        use lin::Queryable;
        for q in [
            Queryable::from("children")
                .join("parents", "parent")
                .select(["n", "parents.label"])
                .take_all(),
            Queryable::from("children")
                .left_join("parents", "parent")
                .select(["n", "parents.label"])
                .take_all(),
            Queryable::from("children")
                .filter("n > 20")
                .join("parents", "parent")
                .select(["n", "parents.label"])
                .skip(5)
                .take(20),
            Queryable::from("children")
                .join("parents", "parent")
                .filter("n > 60")
                .select(["n", "parents.label"])
                .take_all(),
            Queryable::from("children")
                .take(3)
                .left_join("parents", "parent")
                .select(["n", "parents.label"])
                .take_all(),
            Queryable::from("children")
                .left_join("parents", "parent")
                .take(5)
                .skip(2)
                .select(["n", "parents.label"])
                .take_all(),
            Queryable::from("children")
                .filter("n < 0")
                .join("parents", "parent")
                .select(["n", "parents.label"])
                .take_all(),
            Queryable::from("children")
                .select(["parent", "n"])
                .join("parents", "parent")
                .take(3),
            Queryable::from("children")
                .join("parents", "parent")
                .join("parents", "parent")
                .select(["n", "parents.label"])
                .take(9),
        ] {
            let expected = q.to_vec(cpu).unwrap();
            let before = gpu.join_dispatches();
            let mut cursor = q.cursor(&db).unwrap();
            assert!(cursor.is_lazy());
            assert_eq!(gpu.join_dispatches(), before, "open must defer join work");
            let actual = cursor.by_ref().collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(actual, expected);
            assert!(cursor.next().is_none());
            let reader = db.reader();
            let mut cursor = q.cursor_read(&reader).unwrap();
            assert!(cursor.is_lazy());
            let mut actual = Vec::new();
            while let Some(row) = cursor.next_projected() {
                actual.push(row.unwrap().into_row());
            }
            assert_eq!(actual, expected);
        }
        let q = Queryable::from("children")
            .join("parents", "parent")
            .take(0);
        let before = gpu.join_dispatches();
        assert!(q.cursor(&db).unwrap().next().is_none());
        assert_eq!(gpu.join_dispatches(), before);
    };
    check(&mut cpu);
    // ID joins take the last duplicate; non-ID joins above emit every duplicate.
    let ids: Vec<_> = cpu
        .store
        .collection("parents")
        .iter()
        .map(|r| r.get("id").unwrap().clone())
        .collect();
    cpu.catalog
        .fks
        .iter_mut()
        .find(|fk| fk.from_col == "children")
        .unwrap()
        .to_field = "id".into();
    for (i, row) in cpu
        .store
        .collections
        .get_mut("children")
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        row.insert(
            "parent".into(),
            if i % 11 == 0 {
                Cell::Null
            } else {
                ids[i % ids.len()].clone()
            },
        );
    }
    let mut duplicate = cpu.store.collection("parents")[0].clone();
    duplicate.insert("label".into(), Cell::text_arc("last duplicate"));
    cpu.store
        .collections
        .get_mut("parents")
        .unwrap()
        .push(duplicate);
    check(&mut cpu);
    assert!(
        gpu.join_dispatches() > 30,
        "joins must submit actual GPU work"
    );
}

#[test]
#[ignore = "requires a hardware GPU"]
fn stable_sort_matches_cpu_all_cells_directions_and_odd_lengths() {
    use lin::{Cell, Db};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::empty();
    cpu.run("col sort_items { key: f64, ordinal: i64 }")
        .unwrap();
    let values = [
        Cell::Null,
        Cell::Float(f64::NAN),
        Cell::Float(0.0),
        Cell::Float(-0.0),
        Cell::Float(f64::INFINITY),
        Cell::Float(f64::NEG_INFINITY),
        Cell::Int(i64::MIN),
        Cell::Int(i64::MAX),
        Cell::Int(9_007_199_254_740_993),
        Cell::Int(9_007_199_254_740_992),
        Cell::Float(f64::from_bits(1)),
        Cell::Float(-f64::from_bits(1)),
        Cell::Float(1.0000000000000002),
        Cell::Float(1.0),
        Cell::Time(100),
        Cell::text_arc(""),
        Cell::text_arc("Привет"),
        Cell::text_arc("a\0b"),
        Cell::text_arc("a"),
        Cell::Bool(false),
        Cell::Bool(true),
        Cell::Vec(Arc::from([1f32, 2.0])),
        Cell::Vec(Arc::from([3f32; 10])),
    ];
    for i in 0..137 {
        cpu.run(&format!("insert sort_items {{ key: 1.0, ordinal: {i} }}"))
            .unwrap();
    }
    for (i, row) in cpu
        .store
        .collections
        .get_mut("sort_items")
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        row.insert("key".into(), values[(i * 17) % values.len()].clone());
    }
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    for q in [
        "sort_items | sort key | { ordinal, id } | take all",
        "sort_items | sort key desc | { ordinal, id } | take all",
        "sort_items | key > 0 | sort key desc | { ordinal, id } | skip 3 | take 17",
        "sort_items | ordinal < 0 | sort key | { id } | take all",
        "sort_items | ordinal == 1 | sort key | { id } | take all",
        "sort_items | take 65 | sort key | { ordinal, id } | take all",
        "sort_items | sort key | sort ordinal desc | { id } | take all",
        "sort_items | sort key | count by ordinal | sort hits desc | { ordinal } | take all",
    ] {
        assert_eq!(cpu.run(q).unwrap().rows, db.run(q).unwrap().rows, "{q}");
        assert_eq!(
            cpu.run_batch(q).unwrap(),
            db.run_batch(q).unwrap(),
            "batch {q}"
        );
        assert_eq!(
            cpu.run(q).unwrap().rows,
            db.reader().run(q).unwrap().rows,
            "reader {q}"
        );
    }
    assert!(
        gpu.sort_dispatches() >= 100,
        "sort must submit actual GPU passes"
    );
}

#[test]
#[ignore = "requires a hardware GPU"]
fn gpu_cursor_is_lazy_bounded_and_keeps_stage_order_and_projection_schema() {
    use lin::{Db, Queryable};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::empty();
    cpu.run("col stream_items { n: i64, text: text }").unwrap();
    let records: Vec<_> = (0..2053)
        .map(|i| {
            format!(
                "{{ n: {i}, text: \"{}\" }}",
                if i % 2 == 0 { "hit" } else { "miss" }
            )
        })
        .collect();
    cpu.run(&format!("insert stream_items [{}]", records.join(",")))
        .unwrap();
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    let q = Queryable::from("stream_items")
        .filter("n > 1000 and text ~ \"hit\"")
        .select(["n"])
        .take(5);
    let expected = q.to_vec(&mut cpu).unwrap();
    let before = (gpu.comparison_dispatches(), gpu.text_dispatches());
    {
        let mut cursor = q.cursor(&db).unwrap();
        assert!(cursor.is_lazy());
        assert_eq!(
            (gpu.comparison_dispatches(), gpu.text_dispatches()),
            before,
            "opening must not execute the whole query"
        );
        let first = cursor.next_projected().unwrap().unwrap();
        let second = cursor.next_projected().unwrap().unwrap();
        assert_eq!(
            first.fields().as_ptr(),
            second.fields().as_ptr(),
            "projection schema must be shared"
        );
        let after_first = (gpu.comparison_dispatches(), gpu.text_dispatches());
        assert!(after_first.0 > before.0 && after_first.1 > before.1);
        let mut rows = vec![first.into_row(), second.into_row()];
        while let Some(row) = cursor.next_projected() {
            rows.push(row.unwrap().into_row());
        }
        assert_eq!(rows, expected);
        assert_eq!(
            (gpu.comparison_dispatches(), gpu.text_dispatches()),
            after_first,
            "take must stop before another batch"
        );
    }
    for q in [
        Queryable::from("stream_items")
            .skip(7)
            .filter("n < 10")
            .select(["n"])
            .take_all(),
        Queryable::from("stream_items")
            .take(1100)
            .filter("n > 1000")
            .select(["n"])
            .take_all(),
        Queryable::from("stream_items")
            .filter("n >= 0")
            .select(["n"])
            .skip(1020)
            .take(17),
        Queryable::from("stream_items")
            .filter("n > 2000")
            .filter("text ~ \"hit\"")
            .select(["n"])
            .take_all(),
        Queryable::from("stream_items")
            .filter("n < 0")
            .select(["n"])
            .take_all(),
        Queryable::from("stream_items")
            .filter("n >= 0")
            .select(["n"])
            .take(0),
    ] {
        let expected = q.to_vec(&mut cpu).unwrap();
        let mut cursor = q.cursor(&db).unwrap();
        assert!(cursor.is_lazy());
        assert_eq!(
            cursor.by_ref().collect::<Result<Vec<_>, _>>().unwrap(),
            expected
        );
        assert!(cursor.next().is_none());
        let snapshot = db.reader();
        let mut cursor = q.cursor_read(&snapshot).unwrap();
        assert!(cursor.is_lazy());
        let mut rows = Vec::new();
        while let Some(row) = cursor.next_projected() {
            rows.push(row.unwrap().into_row());
        }
        assert_eq!(rows, expected);
    }
}

#[test]
#[ignore = "requires a hardware GPU"]
fn gpu_cursor_defers_later_batch_errors_and_reports_each_error_once() {
    use lin::{Cell, Db, Queryable};
    use std::sync::Arc;
    let mut options = lin::gpu::GpuOptions::default();
    options.limits.max_storage_buffer_binding_size = 4096;
    let gpu = Arc::new(lin::gpu::GpuCompute::with_options(options).unwrap());
    let mut db = Db::empty().with_gpu(gpu);
    db.run("col stream_errors { text: text }").unwrap();
    let records = vec![r#"{ text: "hit" }"#; 2053];
    db.run(&format!("insert stream_errors [{}]", records.join(",")))
        .unwrap();
    db.store.collections.get_mut("stream_errors").unwrap()[1500]
        .insert("text".into(), Cell::text_arc("x".repeat(2048)));
    let q = Queryable::from("stream_errors")
        .filter("text ~ \"hit\"")
        .select(["id"]);
    let mut short = q.clone().take(3).cursor(&db).unwrap();
    assert!(short.is_lazy());
    assert_eq!(
        short.by_ref().collect::<Result<Vec<_>, _>>().unwrap().len(),
        3
    );
    let mut full = q.take_all().cursor(&db).unwrap();
    for _ in 0..1024 {
        assert!(full.next().unwrap().is_ok());
    }
    let error = full.next().unwrap().unwrap_err();
    assert!(error.to_string().contains("text row exceeds"), "{error}");
    assert!(full.next().is_none());
    assert!(full.next().is_none());
}

#[test]
#[ignore = "requires a hardware GPU"]
fn binary64_group_sum_matches_source_order_bits_and_special_values() {
    use lin::gpu::{GpuCompute, GpuOptions};
    let gpu = GpuCompute::new().unwrap();
    let mut sequences = vec![
        vec![0.0, -0.0, -0.0],
        vec![f64::MAX, f64::MAX],
        vec![f64::INFINITY, f64::NEG_INFINITY],
        vec![f64::NEG_INFINITY, 2.0],
        vec![f64::NAN, 1.0],
        vec![1e16, 1.0, -1e16],
        vec![1e16, -1e16, 1.0],
        vec![f64::from_bits(1), f64::from_bits(1), -f64::from_bits(1)],
        vec![f64::MIN_POSITIVE, -f64::from_bits(0x000fffffffffffff)],
        vec![1.0, f64::EPSILON / 2.0],
        vec![1.0 + f64::EPSILON, f64::EPSILON / 2.0],
        vec![f64::MAX, -f64::MAX, f64::MIN_POSITIVE],
    ];
    let mut state = 0xa8362349a398c631u64;
    for _ in 0..1024 {
        let mut seq = Vec::new();
        for _ in 0..7 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            seq.push(f64::from_bits(state));
        }
        sequences.push(seq);
        let magnitude = state & 0x7fefffffffffffff;
        if magnitude > 0 {
            let a = f64::from_bits(magnitude);
            let neighbor = f64::from_bits(magnitude - 1);
            sequences.push(vec![a, -neighbor, -a, neighbor]);
            sequences.push(vec![-a, neighbor, a, -neighbor]);
        }
    }
    let mut ids = Vec::new();
    let mut values = Vec::new();
    // Interleave groups and cross dispatch boundaries.
    for i in 0..7 {
        for (group, sequence) in sequences.iter().enumerate() {
            if let Some(&value) = sequence.get(i) {
                ids.push(group as u32);
                values.push(value);
            }
        }
    }
    let result = gpu.group_sums(&ids, &values, sequences.len()).unwrap();
    assert!(gpu.sum_dispatches() >= 2);
    for (i, (sequence, actual)) in sequences.iter().zip(result).enumerate() {
        let expected = sequence.iter().fold(0.0, |sum, value| sum + value);
        if expected.is_nan() {
            assert!(actual.is_nan(), "group {i}");
        } else {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "group {i}: {sequence:?}"
            );
        }
    }
    assert_eq!(gpu.group_sums(&[], &[], 2).unwrap(), [0.0, 0.0]);
    assert!(gpu.group_sums(&[1], &[0.0], 1).is_err());
    assert!(gpu.group_sums(&[0], &[], 1).is_err());
    let mut options = GpuOptions::default();
    options.limits.max_storage_buffer_binding_size = 16;
    let small = GpuCompute::with_options(options).unwrap();
    assert_eq!(
        small
            .group_sums(&[0, 0, 0], &[1e16, 1.0, -1e16], 1)
            .unwrap(),
        [0.0]
    );
    assert_eq!(small.sum_dispatches(), 3);
    assert!(small.group_sums(&[], &[], 3).is_err());
}

#[test]
#[ignore = "requires a hardware GPU"]
fn executor_sum_matches_cpu_groups_batches_and_snapshots() {
    use lin::{Cell, Db, Row};
    use std::sync::Arc;
    fn assert_rows(actual: &[Row], expected: &[Row], query: &str) {
        assert_eq!(actual.len(), expected.len(), "{query}");
        for (a, b) in actual.iter().zip(expected) {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (key, value) in a {
                match (value, &b[key]) {
                    (Cell::Float(a), Cell::Float(b)) if b.is_nan() => assert!(a.is_nan()),
                    (Cell::Float(a), Cell::Float(b)) => {
                        assert_eq!(a.to_bits(), b.to_bits(), "{query}: {key}")
                    }
                    (a, b) => assert_eq!(a, b, "{query}: {key}"),
                }
            }
        }
    }
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::empty();
    cpu.run("col sum_rows { key: text, value: f64 }").unwrap();
    for i in 0..513 {
        cpu.run(&format!(
            "insert sum_rows {{ key: \"k{}\", value: 1.0 }}",
            i % 7
        ))
        .unwrap();
    }
    let variants = [
        Cell::Float(1e16),
        Cell::Float(1.0),
        Cell::Float(-1e16),
        Cell::Int(i64::MAX),
        Cell::Time(i64::MIN),
        Cell::Null,
        Cell::Float(f64::from_bits(1)),
        Cell::Bool(true),
        Cell::Float(-0.0),
        Cell::text_arc("value"),
    ];
    for (i, row) in cpu
        .store
        .collections
        .get_mut("sum_rows")
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        row.insert("value".into(), variants[i % variants.len()].clone());
        if i % 31 == 0 {
            row.insert("key".into(), Cell::Null);
        }
        if i % 17 == 0 {
            row.insert("key".into(), Cell::text_arc("Привет \"quoted\""));
        }
        if i == 511 {
            row.insert("value".into(), Cell::Float(f64::INFINITY));
        }
        if i == 512 {
            row.insert("value".into(), Cell::Float(f64::NAN));
        }
    }
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    for q in [
        "sum_rows | sum value by key",
        "sum_rows | key == \"missing\" | sum value by key",
        "sum_rows | key ~ \"k\" | sum value by key | { key, value } | take all",
        "sum_rows | take 200 | sum value by key | sort key | skip 2 | take 3",
        "sum_rows | sum value by key | value > 0 | { key } | take all",
        "sum_rows | sum value by key | count",
        "sum_rows | sum value by key | sum value by key",
    ] {
        let expected = cpu.run(q).unwrap().rows;
        assert_rows(&db.run(q).unwrap().rows, &expected, q);
        assert_rows(&db.run_batch(q).unwrap().to_rows(), &expected, q);
        assert_rows(&db.reader().run(q).unwrap().rows, &expected, q);
    }
    assert!(gpu.sum_dispatches() >= 18);
}

#[test]
#[ignore = "requires a hardware GPU"]
fn graph_hop_and_match_preserve_cpu_order_cycles_reverse_and_limits() {
    use lin::Db;
    use std::sync::Arc;
    let mut options = lin::gpu::GpuOptions::default();
    options.limits.max_storage_buffer_binding_size = 16384;
    let gpu = Arc::new(lin::gpu::GpuCompute::with_options(options).unwrap());
    let mut cpu = Db::fixture();
    cpu.store.edges.clear();
    for i in 0..6 {
        cpu.run(&format!(
            "insert docs {{ uri: \"gpu://g{i}\", title: \"G{i}\", layer: \"wiki\" }}"
        ))
        .unwrap();
    }
    let nodes: Vec<String> = (0..6)
        .map(|i| {
            cpu.run(&format!("docs | uri == \"gpu://g{i}\" | {{ id }}"))
                .unwrap()
                .rows[0]["id"]
                .text()
                .unwrap()
                .to_owned()
        })
        .collect();
    for (from, to) in [
        (0, 1),
        (0, 2),
        (1, 3),
        (2, 3),
        (3, 0),
        (3, 4),
        (4, 4),
        (5, 0),
    ] {
        cpu.run(&format!(
            "append edge wikilink \"{}\" -> \"{}\"",
            nodes[from], nodes[to]
        ))
        .unwrap();
    }
    // Duplicate physical edges and an unresolved destination exercise dedup/resolution.
    cpu.store.edges.push(cpu.store.edges[0].clone());
    cpu.run(&format!(
        "append edge wikilink \"{}\" -> \"gpu://missing\"",
        nodes[0]
    ))
    .unwrap();
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    for suffix in [
        "hop wikilink | { title } | take all",
        "hop wikilink depth=3 | { title } | take all",
        "hop backlink depth=2 | { title } | take all",
        "graph wikilink depth=3 | take all",
        "graph backlink depth=3 | take all",
        "match -wikilink-> b | { b.title } | take all",
        "match a -wikilink-> b -wikilink-> c | { a.title, b.title, c.title } | take all",
        "match -wikilink*1..3-> b | { b.title } | take all",
        "match <-wikilink- b | { b.title } | take all",
        "match -backlink-> b | { b.title } | take all",
        "match -[e:wikilink]-> b | { e.rel, e.from, e.to, b.title } | take all",
    ] {
        let q = format!("docs | uri == \"gpu://g0\" | {suffix}");
        let expected = cpu.run(&q).unwrap().rows;
        let before = gpu.graph_dispatches();
        assert_eq!(db.run(&q).unwrap().rows, expected, "{q}");
        assert!(
            gpu.graph_dispatches() > before,
            "GPU traversal must dispatch: {q}"
        );
        assert_eq!(db.run_batch(&q).unwrap().to_rows(), expected, "batch {q}");
        assert_eq!(db.reader().run(&q).unwrap().rows, expected, "snapshot {q}");
    }
    // Large fanout preserves the existing 300-result boundary and tile order.
    for i in 0..310 {
        cpu.run(&format!(
            "insert docs {{ uri: \"gpu://wide{i:03}\", title: \"W{i}\", layer: \"wiki\" }}"
        ))
        .unwrap();
        cpu.run(&format!(
            "append edge wikilink \"{}\" -> \"gpu://wide{i:03}\"",
            nodes[0]
        ))
        .unwrap();
    }
    db.store = cpu.store.clone_mem();
    for suffix in [
        "hop wikilink depth=3",
        "graph wikilink depth=3",
        "match -wikilink-> b",
    ] {
        let q = format!("docs | uri == \"gpu://g0\" | {suffix} | take all");
        let expected = cpu.run(&q).unwrap().rows;
        assert_eq!(expected.len(), 300);
        assert_eq!(db.run(&q).unwrap().rows, expected, "cutoff {q}");
    }
    let q = "docs | graph wikilink depth=2 | take all";
    assert_eq!(
        db.run(q).unwrap().rows,
        cpu.run(q).unwrap().rows,
        "tiled frontier"
    );
    for q in [
        lin::Queryable::from("docs")
            .filter("uri == \"gpu://g0\"")
            .hop("wikilink")
            .select(["id", "title"])
            .take_all(),
        lin::Queryable::from("docs")
            .filter("uri == \"gpu://g0\"")
            .hop("backlink")
            .select(["title"])
            .skip(1)
            .take(3),
    ] {
        let expected = q.to_vec(&mut cpu).unwrap();
        let before = gpu.graph_dispatches();
        let mut cursor = q.cursor(&db).unwrap();
        assert!(cursor.is_lazy());
        assert!(gpu.graph_dispatches() > before);
        assert_eq!(
            cursor.by_ref().collect::<Result<Vec<_>, _>>().unwrap(),
            expected
        );
        assert!(cursor.next().is_none());
        let reader = db.reader();
        let mut cursor = q.cursor_read(&reader).unwrap();
        assert!(cursor.is_lazy());
        let mut rows = Vec::new();
        while let Some(row) = cursor.next_projected() {
            rows.push(row.unwrap().into_row());
        }
        assert_eq!(rows, expected);
    }
    let q = lin::Queryable::from("docs")
        .filter("title ~ \"G0\"")
        .hop("wikilink")
        .select(["title"])
        .take(4);
    let expected = q.to_vec(&mut cpu).unwrap();
    let before = gpu.text_dispatches();
    let cursor = q.cursor(&db).unwrap();
    assert!(cursor.is_lazy());
    assert!(
        gpu.text_dispatches() > before,
        "residual seed filter must use compute"
    );
    assert_eq!(cursor.collect::<Result<Vec<_>, _>>().unwrap(), expected);
    // Empty frontiers and edges need no native dispatch.
    let q = "docs | title == \"absent\" | graph wikilink | take all";
    let before = gpu.graph_dispatches();
    assert!(db.run(q).unwrap().rows.is_empty());
    assert_eq!(gpu.graph_dispatches(), before);
    db.store.edges.clear();
    assert!(
        db.run("docs | hop wikilink | take all")
            .unwrap()
            .rows
            .is_empty()
    );
}

#[test]
#[ignore = "requires a hardware GPU"]
fn gpu_join_cursor_defers_limit_error_and_then_finishes() {
    use lin::{Db, Queryable};
    use std::sync::Arc;
    let mut options = lin::gpu::GpuOptions::default();
    options.limits.max_storage_buffer_binding_size = 3;
    let gpu = Arc::new(lin::gpu::GpuCompute::with_options(options).unwrap());
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.run("col parents { key: text }\ncol children { parent: text }\nfk children.parent -> parents.key\ninsert parents { key: \"one\" }\ninsert children { parent: \"one\" }").unwrap();
    let q = Queryable::from("children")
        .join("parents", "parent")
        .take(1);
    let mut cursor = q.cursor(&db).unwrap();
    assert!(cursor.is_lazy());
    assert_eq!(gpu.join_dispatches(), 0);
    let error = cursor.next_projected().unwrap().unwrap_err();
    assert!(
        error.to_string().contains("cannot hold join input"),
        "{error}"
    );
    assert!(cursor.next().is_none());
    assert!(cursor.next_projected().is_none());
    let q = q.take(0);
    assert!(q.cursor(&db).unwrap().next().is_none());
}

#[test]
#[ignore = "requires a hardware GPU"]
fn lexical_search_compute_matches_unicode_weights_fts_and_lazy_cursors() {
    use lin::{Db, Queryable};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::fixture();
    let samples = [
        ("gpu://s0", "foo-bar foo foo_bar", "word phrase"),
        ("gpu://s1", "FOO BAR", "word"),
        ("gpu://s2", "Привет приветствие", "İSTANBUL ΟΣ"),
        ("gpu://s3", "word", "phrase foo"),
        ("gpu://s4", "foobar", "miss"),
        ("gpu://s5", "", ""),
    ];
    cpu.run("col scan_search { title: text, body: text, snippet: text }")
        .unwrap();
    for (uri, title, body) in samples {
        cpu.run(&format!("insert docs {{ uri: \"{uri}\", title: \"{title}\", body: \"{body}\", layer: \"wiki\" }}")).unwrap();
        cpu.run(&format!(
            "insert scan_search {{ title: \"{title}\", body: \"{body}\", snippet: \"\" }}"
        ))
        .unwrap();
    }
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    for collection in ["docs", "scan_search"] {
        for term in [
            "foo",
            "foo foo",
            "foo-bar",
            "foo_bar",
            "word phrase",
            "Привет",
            "İSTANBUL",
            "ΟΣ",
            "",
            "missing-term",
        ] {
            let q = Queryable::from(collection)
                .search_lex(term)
                .select(["id", "title"])
                .take_all();
            let expected = q.to_vec(&mut cpu).unwrap();
            assert_eq!(
                q.to_vec(&mut db).unwrap(),
                expected,
                "{collection}: {term:?}"
            );
            let mut cursor = q.cursor(&db).unwrap();
            if collection == "docs" {
                assert!(cursor.is_lazy());
            }
            assert_eq!(
                cursor.by_ref().collect::<Result<Vec<_>, _>>().unwrap(),
                expected
            );
            let snapshot = db.reader();
            let mut cursor = q.cursor_read(&snapshot).unwrap();
            let mut rows = Vec::new();
            while let Some(row) = cursor.next_projected() {
                rows.push(row.unwrap().into_row());
            }
            assert_eq!(rows, expected);
        }
        let q = Queryable::from(collection)
            .search_lex("foo")
            .select(["id"])
            .skip(1)
            .take(2);
        assert_eq!(q.to_vec(&mut db).unwrap(), q.to_vec(&mut cpu).unwrap());
    }
    let q = Queryable::from("docs")
        .search_lex("foo")
        .take(3)
        .skip(1)
        .select(["id"]);
    let expected = q.to_vec(&mut cpu).unwrap();
    assert_eq!(q.to_vec(&mut db).unwrap(), expected);
    assert_eq!(
        q.cursor(&db)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap(),
        expected
    );
    assert!(gpu.text_dispatches() > 40);
    assert!(gpu.weight_dispatches() > 40);
    assert!(gpu.sort_dispatches() > 0, "search ranking must use compute");
    let q = Queryable::from("docs")
        .search("foo")
        .select(["id", "title"])
        .take(5);
    let expected = q.to_vec(&mut db).unwrap();
    let before = gpu.text_dispatches();
    let cursor = q.cursor(&db).unwrap();
    assert!(cursor.is_lazy());
    assert!(gpu.text_dispatches() > before);
    assert_eq!(cursor.collect::<Result<Vec<_>, _>>().unwrap(), expected);
}

#[test]
#[ignore = "requires a hardware GPU"]
fn hybrid_rrf_matches_cpu_identity_ties_projection_and_cursors() {
    use lin::{Db, Embedder, Queryable};
    use std::sync::Arc;
    struct Constant;
    impl Embedder for Constant {
        fn id(&self) -> &str {
            "gpu-rrf/3"
        }
        fn dim(&self) -> usize {
            3
        }
        fn embed(&self, _: &str) -> Arc<[f32]> {
            Arc::from([1.0, 0.0, 0.0])
        }
    }
    let embedder: Arc<dyn Embedder> = Arc::new(Constant);
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::fixture().with_embedder(embedder.clone());
    cpu.reembed_collection("docs").unwrap();
    for i in 0..71 {
        let title = match i % 4 {
            0 => "fusionword",
            1 => "fusionword phrase",
            2 => "prefixfusionword",
            _ => "other",
        };
        cpu.run(&format!(
            "insert docs {{ uri: \"rrf://{i}\", title: \"{title}\", layer: \"wiki\" }}"
        ))
        .unwrap();
    }
    let mut db = Db::empty().with_embedder(embedder).with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    for q in [
        Queryable::from("docs")
            .search("fusionword")
            .select(["id", "title"])
            .take_all(),
        Queryable::from("docs")
            .search("fusionword phrase")
            .select(["id", "title"])
            .skip(3)
            .take(7),
        Queryable::from("docs")
            .search("missing-term")
            .select(["id"])
            .take(5),
        Queryable::from("docs").search("").select(["id"]).take_all(),
        Queryable::from("docs")
            .filter("title ~ \"fusionword\"")
            .search("fusionword")
            .select(["id", "title"])
            .take_all(),
        Queryable::from("docs")
            .select(["uri", "title", "embedding"])
            .search("fusionword")
            .select(["uri", "title"])
            .take_all(),
        Queryable::from("docs")
            .select(["title", "embedding"])
            .search("fusionword")
            .select(["title"])
            .take_all(),
    ] {
        let expected = q.to_vec(&mut cpu).unwrap();
        let before = gpu.rrf_dispatches();
        assert_eq!(q.to_vec(&mut db).unwrap(), expected);
        assert!(gpu.rrf_dispatches() > before, "RRF must use compute");
        assert_eq!(
            q.cursor(&db)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            expected
        );
        let reader = db.reader();
        assert_eq!(
            q.cursor_read(&reader)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            expected
        );
    }
    cpu = cpu.without_embedder();
    db = db.without_embedder();
    let q = Queryable::from("docs")
        .search("fusionword")
        .select(["id"])
        .take_all();
    assert_eq!(
        q.to_vec(&mut db).unwrap(),
        q.to_vec(&mut cpu).unwrap(),
        "lex-only RRF"
    );
    let q = Queryable::from("docs")
        .filter("title == \"absent\"")
        .search("fusionword")
        .take_all();
    let before = gpu.rrf_dispatches();
    assert!(q.to_vec(&mut db).unwrap().is_empty());
    assert_eq!(gpu.rrf_dispatches(), before);
}

#[test]
#[ignore = "requires a hardware GPU"]
fn regex_executor_and_lazy_cursor_match_cpu_and_dispatch() {
    use lin::{Cell, Db, Queryable};
    use std::sync::Arc;
    let gpu = Arc::new(lin::gpu::GpuCompute::new().unwrap());
    let mut cpu = Db::empty();
    cpu.run("col regex_rows { text: text, n: i64 }").unwrap();
    for i in 0..137 {
        cpu.run(&format!(
            "insert regex_rows {{ text: \"placeholder\", n: {i} }}"
        ))
        .unwrap();
    }
    let values = [
        Some("foo bar"),
        Some("FOO_bar"),
        Some("Привет мир"),
        Some("cafe\u{301}"),
        Some("a\r\nb"),
        Some(""),
        None,
        Some("😀"),
        Some("a\0b"),
    ];
    for (i, row) in cpu
        .store
        .collections
        .get_mut("regex_rows")
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        row.insert(
            "text".into(),
            values[i % values.len()]
                .map(Cell::text_arc)
                .unwrap_or(Cell::Null),
        );
    }
    let mut db = Db::empty().with_gpu(gpu.clone());
    db.catalog = cpu.catalog.clone();
    db.store = cpu.store.clone_mem();
    for predicate in [
        r#"text ~ /foo/i"#,
        r#"text ~ /\b\w+\b/"#,
        r#"text ~ /(?mR)^b$/"#,
        r#"text ~ /(?:a?)*$/"#,
        r#"text ~ /foo/ or n < 3"#,
        r#"text ~ /\p{L}+/ and n > 20"#,
    ] {
        let q = Queryable::from("regex_rows")
            .filter(predicate)
            .select(["n", "text"])
            .skip(2)
            .take(7);
        let expected = q.to_vec(&mut cpu).unwrap();
        let before = gpu.regex_dispatches();
        assert_eq!(q.to_vec(&mut db).unwrap(), expected, "{predicate}");
        assert!(gpu.regex_dispatches() > before);
        let cursor = q.cursor(&db).unwrap();
        assert!(cursor.is_lazy());
        assert_eq!(cursor.collect::<Result<Vec<_>, _>>().unwrap(), expected);
        let snapshot = db.reader();
        assert_eq!(
            q.cursor_read(&snapshot)
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            expected
        );
        let program = format!("regex_rows | {predicate} | count");
        assert_eq!(
            db.run_batch(&program).unwrap(),
            cpu.run_batch(&program).unwrap()
        );
    }
}
