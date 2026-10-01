@group(0) @binding(0) var<storage, read> mask: array<u32>;
@group(0) @binding(1) var<storage, read> weight: array<u32>;
@group(0) @binding(2) var<storage, read_write> scores: array<vec2<u32>>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= weight[3] { return; }
    let mask_index = id.x + weight[1];
    let score_index = id.x + weight[2];
    if mask[mask_index] == 0u { return; }
    let before = scores[score_index];
    let low = before.x + weight[0];
    scores[score_index] = vec2<u32>(low, before.y + select(0u, 1u, low < before.x));
}
