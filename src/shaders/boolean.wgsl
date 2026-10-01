@group(0) @binding(0) var<storage, read> a: array<u32>;
@group(0) @binding(1) var<storage, read> b: array<u32>;
@group(0) @binding(2) var<storage, read> operation: array<u32>;
@group(0) @binding(3) var<storage, read_write> mask: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&mask) { return; }
    let left = a[id.x] != 0u;
    let right = b[id.x] != 0u;
    let keep = select(left && right, left || right, operation[0] != 0u);
    mask[id.x] = select(0u, 1u, keep);
}
