@group(0) @binding(0) var<storage, read> groups: array<u32>;
@group(0) @binding(1) var<storage, read_write> counts: array<atomic<u32>>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&groups) { return; }
    atomicAdd(&counts[groups[id.x]], 1u);
}
