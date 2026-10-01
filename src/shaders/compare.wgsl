// Ordered 64-bit keys represented by two u32 words: no f32 rounding.
@group(0) @binding(0) var<storage, read> input: array<u32>;
@group(0) @binding(1) var<storage, read> operation: array<u32>;
@group(0) @binding(2) var<storage, read_write> mask: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let row = id.x;
    if row >= arrayLength(&mask) { return; }
    let base = row * 5u;
    let valid = input[base + 4u] != 0u;
    let equal = valid && input[base] == input[base + 2u] && input[base + 1u] == input[base + 3u];
    let less = valid && (input[base + 1u] < input[base + 3u] ||
        (input[base + 1u] == input[base + 3u] && input[base] < input[base + 2u]));
    let greater = valid && !less && !equal;
    var keep = false;
    switch operation[0] {
        case 0u: { keep = equal; }
        case 1u: { keep = !equal; }
        case 2u: { keep = greater; }
        case 3u: { keep = less; }
        case 4u: { keep = greater || equal; }
        case 5u: { keep = less || equal; }
        default: {}
    }
    mask[row] = select(0u, 1u, keep);
}
