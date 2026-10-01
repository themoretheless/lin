@group(0) @binding(0) var<storage, read> text: array<u32>;
@group(0) @binding(1) var<storage, read> rows: array<u32>;
@group(0) @binding(2) var<storage, read> params: array<u32>;
@group(0) @binding(3) var<storage, read_write> mask: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let row = id.x;
    if row >= arrayLength(&mask) { return; }
    let offset = rows[row * 3u];
    let length = rows[row * 3u + 1u];
    let valid = rows[row * 3u + 2u] != 0u;
    let op = params[0];
    let n = params[1];
    var found = false;
    if valid && n <= length {
        if n == 0u { found = op != 3u && (op == 0u || length == 0u); }
        else {
            for (var start = 0u; start + n <= length; start++) {
                if (op == 1u || op == 2u) && (start != 0u || n != length) { break; }
                if op == 3u && ((text[offset + start] & 256u) == 0u || (text[offset + start + n - 1u] & 512u) == 0u) { continue; }
                var equal = true;
                for (var i = 0u; i < n; i++) {
                    let value = text[offset + start + i];
                    if (value & 255u) != params[i + 2u] || (op == 3u && (value & 1024u) == 0u) { equal = false; break; }
                }
                if equal { found = true; break; }
            }
        }
    }
    if op == 2u { found = !found; }
    mask[row] = select(0u, 1u, found);
}
