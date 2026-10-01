@group(0) @binding(0) var<storage, read> keys: array<u32>;
@group(0) @binding(1) var<storage, read_write> lookup: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read> params: array<u32>;
@group(0) @binding(3) var<storage, read_write> result: array<u32>;
@compute @workgroup_size(64)
fn build(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&keys) { return; }
    let key = keys[id.x];
    if key != 0u { atomicMax(&lookup[key], params[0] + id.x + 1u); }
}
@compute @workgroup_size(64)
fn probe(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&keys) { return; }
    result[id.x] = atomicLoad(&lookup[keys[id.x]]);
}
