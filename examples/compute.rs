//! A user WGSL kernel, with a persistent GPU buffer and repeated dispatch.
//! cargo run --release --features gpu --example compute
#[cfg(feature = "gpu")]
fn main() -> Result<(), lin::Error> {
    use lin::gpu::{GpuCompute, native as wgpu};
    let gpu = GpuCompute::new()?;
    let shader = gpu.compile_wgsl(
        r#"
        @group(0) @binding(0) var<storage, read_write> data: array<u32>;
        @compute @workgroup_size(64)
        fn main(@builtin(global_invocation_id) id: vec3<u32>) {
            if id.x < arrayLength(&data) { data[id.x] = data[id.x] * 2u + 1u; }
        }
    "#,
        "main",
    )?;
    let values = [1u32, 2, 3, 4];
    let buffer = gpu.storage_buffer(bytemuck::cast_slice(&values))?;
    let group = gpu.checked(|device, _| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &shader.bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        })
    })?;
    // No intermediate CPU readback.
    gpu.dispatch(&shader, &[&group], [1, 1, 1])?;
    gpu.dispatch(&shader, &[&group], [1, 1, 1])?;
    let bytes = gpu.read_buffer(&buffer)?;
    let output: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
        .collect();
    assert_eq!(output, [7, 11, 15, 19]);
    println!("{}: {output:?}", gpu.adapter_info().name);
    Ok(())
}
#[cfg(not(feature = "gpu"))]
fn main() {
    eprintln!("Build with --features gpu");
}
