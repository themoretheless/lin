#![cfg(feature = "gpu")]

/// Validate every shipped compute module without an adapter. Hardware tests
/// separately exercise resource bindings and results on an actual device.
#[test]
fn shipped_compute_shaders_validate_with_baseline_capabilities() {
    use lin::gpu::native::naga;
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/shaders");
    let mut count = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|s| s.to_str()) != Some("wgsl") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|error| {
            panic!("{}: {}", path.display(), error.emit_to_string(&source))
        });
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
        assert!(
            !module.entry_points.is_empty()
                && module
                    .entry_points
                    .iter()
                    .all(|entry| entry.stage == naga::ShaderStage::Compute),
            "{} must contain compute entry points",
            path.display()
        );
        count += 1;
    }
    assert!(count >= 14, "compute shader inventory unexpectedly shrank");
}
