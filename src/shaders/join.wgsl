@group(0) @binding(0) var<storage, read> left: array<u32>;
@group(0) @binding(1) var<storage, read> right: array<u32>;
@group(0) @binding(2) var<storage, read_write> matches: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&matches) { return; }
    let width = arrayLength(&right);
    let a = left[id.x / width];
    let b = right[id.x % width];
    matches[id.x] = select(0u, 1u, a != 0u && a == b);
}
