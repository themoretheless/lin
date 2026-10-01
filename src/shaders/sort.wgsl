@group(0) @binding(0) var<storage, read> source: array<u32>;
@group(0) @binding(1) var<storage, read_write> output_indices: array<u32>;
@group(0) @binding(2) var<storage, read> keys: array<u32>;
@group(0) @binding(3) var<storage, read> text: array<u32>;
@group(0) @binding(4) var<uniform> params: vec4<u32>;

fn before(a: u32, b: u32) -> bool {
    let x = a * 5u;
    let y = b * 5u;
    var order = 0i;
    if keys[x] != keys[y] { order = select(1i, -1i, keys[x] < keys[y]); }
    else if keys[x] == 1u || keys[x] == 3u {
        let ao = keys[x + 3u]; let bo = keys[y + 3u];
        let an = keys[x + 4u]; let bn = keys[y + 4u];
        for (var i = 0u; i < min(an, bn); i++) {
            if text[ao + i] != text[bo + i] {
                order = select(1i, -1i, text[ao + i] < text[bo + i]); break;
            }
        }
        if order == 0i && an != bn { order = select(1i, -1i, an < bn); }
    } else {
        if keys[x + 1u] != keys[y + 1u] { order = select(1i, -1i, keys[x + 1u] < keys[y + 1u]); }
        else if keys[x + 2u] != keys[y + 2u] { order = select(1i, -1i, keys[x + 2u] < keys[y + 2u]); }
    }
    // Original position breaks ties in both directions: stable descending too.
    if order == 0i { return a < b; }
    return select(order < 0i, order > 0i, params.y != 0u);
}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let pos = id.x;
    let count = arrayLength(&source);
    if pos >= count { return; }
    let width = params.x;
    let base = (pos / (width * 2u)) * (width * 2u);
    let middle = min(base + width, count);
    let end = min(base + width * 2u, count);
    let first = pos < middle;
    let own_start = select(middle, base, first);
    let other_start = select(base, middle, first);
    let other_end = select(middle, end, first);
    let item = source[pos];
    var lo = other_start;
    var hi = other_end;
    while lo < hi {
        let mid = lo + (hi - lo) / 2u;
        if before(source[mid], item) { lo = mid + 1u; }
        else { hi = mid; }
    }
    let rank = (pos - own_start) + (lo - other_start);
    output_indices[base + rank] = item;
}
