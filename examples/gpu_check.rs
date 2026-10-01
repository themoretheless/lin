//! Run: cargo run --release --features gpu --example gpu_check
#[cfg(feature = "gpu")]
pub fn main() -> Result<(), lin::Error> {
    use std::{sync::Arc, time::Instant};
    let gpu = Arc::new(lin::gpu::GpuCompute::new()?);
    println!("Adapter: {:?}", gpu.adapter_info());
    let query = [1.0f32, 2.0, 3.0];
    let cases: &[&[f32]] = &[
        &query,
        &[-1.0, -2.0, -3.0],
        &[0.0; 3],
        &[],
        &[1e30, 2e30, 3e30],
        &[1e-20, 2e-20, 3e-20],
        &[1.0],
        &[3.0, 2.0, 1.0, 9.0],
    ];
    let reference = |a: &[f32], b: &[f32]| {
        let mut dot = 0f64;
        let mut na = 0f64;
        let mut nb = 0f64;
        for (&a, &b) in a.iter().zip(b) {
            dot += a as f64 * b as f64;
            na += (a as f64).powi(2);
            nb += (b as f64).powi(2);
        }
        if na <= 1e-18 || nb <= 1e-18 {
            0.0
        } else {
            (dot / (na.sqrt() * nb.sqrt())).clamp(-1.0, 1.0)
        }
    };
    for (v, score) in cases.iter().zip(gpu.cosine_batch(&query, cases)?) {
        assert!((score as f64 - reference(&query, v)).abs() < 1e-5);
    }
    assert_eq!(gpu.cosine_batch(&[], cases)?, vec![0.0; cases.len()]);
    assert!(gpu.cosine_batch(&[f32::NAN], &[&[1.0]]).is_err());
    let many: Vec<&[f32]> = vec![&[1.0]; 65_537];
    assert!(
        gpu.cosine_batch(&[1.0], &many)?
            .iter()
            .all(|s| (*s - 1.0).abs() < 1e-6)
    );
    let mut cpu = lin::Db::fixture();
    let mut accelerated = lin::Db::fixture().with_gpu(gpu.clone());
    for q in [
        "docs | search vec \"wal\" | take 20",
        "docs | search \"wal\" | take 20",
    ] {
        let sort_before = gpu.sort_dispatches();
        assert_eq!(cpu.run(q)?.rows, accelerated.run(q)?.rows);
        assert!(
            gpu.sort_dispatches() > sort_before,
            "search must rank on GPU"
        );
        assert_eq!(cpu.run(q)?.rows, accelerated.reader().run(q)?.rows);
    }
    let dim = 768;
    let count = 10_000;
    let q: Vec<f32> = (0..dim)
        .map(|i| ((i * 17 % 101) as f32 - 50.0) / 50.0)
        .collect();
    let data: Vec<Vec<f32>> = (0..count)
        .map(|r| {
            (0..dim)
                .map(|i| ((r * 13 + i * 31) % 103) as f32 / 103.0 - 0.5)
                .collect()
        })
        .collect();
    let refs: Vec<&[f32]> = data.iter().map(Vec::as_slice).collect();
    let start = Instant::now();
    let expected: Vec<f64> = refs.iter().map(|v| reference(&q, v)).collect();
    let cpu_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let actual = gpu.cosine_batch(&q, &refs)?;
    let gpu_ms = start.elapsed().as_secs_f64() * 1000.0;
    let error = actual
        .iter()
        .zip(expected)
        .map(|(a, b)| (*a as f64 - b).abs())
        .fold(0.0, f64::max);
    assert!(error < 1e-5, "max error {error}");
    let mut resident = gpu.upload_vectors(dim, &refs)?;
    let start = Instant::now();
    let resident_scores = gpu.cosine_resident(&q, &mut resident)?;
    let resident_ms = start.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(resident_scores, actual);
    println!("Resident GPU (query+dispatch+readback): {resident_ms:.3} ms");
    assert!(gpu.cosine_resident(&[1.0], &mut resident).is_err());

    println!(
        "PASS: cosine edge cases, vec/hybrid, reader; {count} x {dim}: CPU {cpu_ms:.3} ms, GPU upload+dispatch+readback {gpu_ms:.3} ms; max error {error:e}"
    );
    Ok(())
}
#[cfg(not(feature = "gpu"))]
pub fn main() {
    eprintln!("Enable --features gpu");
}
