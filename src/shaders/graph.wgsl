@group(0) @binding(0) var<storage, read> sources: array<u32>;
@group(0) @binding(1) var<storage, read> frontier: array<u32>;
@group(0) @binding(2) var<storage, read_write> matches: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&sources) { return; }
    let key = sources[id.x];
    var lo = 0u;
    var hi = arrayLength(&frontier);
    loop {
        if lo >= hi { break; }
        let mid = lo + (hi - lo) / 2u;
        if frontier[mid] < key { lo = mid + 1u; } else { hi = mid; }
    }
    var hit = false;
    if key != 0u && lo < arrayLength(&frontier) { hit = frontier[lo] == key; }
    matches[id.x] = select(0u, 1u, hit);
}
