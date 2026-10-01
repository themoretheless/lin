// Thompson NFA simulation. One invocation owns a row's active sets and queue.
@group(0) @binding(0) var<storage, read> program: array<u32>;
@group(0) @binding(1) var<storage, read> input: array<u32>;
@group(0) @binding(2) var<storage, read_write> scratch: array<u32>;
@group(0) @binding(3) var<storage, read_write> matches: array<u32>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= input[0] { return; }
    let record = 1u + id.x * 4u;
    let offset = input[record];
    let length = input[record + 1u];
    matches[id.x] = 0u;
    if input[record + 2u] == 0u { return; }
    let states = program[0];
    let base = id.x * states * 3u;
    let next_base = base + states;
    let queue_base = next_base + states;
    for (var state = 0u; state < states; state += 1u) {
        scratch[base + state] = 0u;
        scratch[next_base + state] = 0u;
    }
    scratch[base + program[1]] = 1u;
    for (var position = 0u; position <= length; position += 1u) {
        let flags = input[offset + position * 2u + 1u];
        var head = 0u;
        var tail = 0u;
        for (var state = 0u; state < states; state += 1u) {
            if scratch[base + state] != 0u {
                scratch[queue_base + tail] = state;
                tail += 1u;
            }
        }
        loop {
            if head >= tail { break; }
            let state = scratch[queue_base + head];
            head += 1u;
            let header = 2u + state * 4u;
            let kind = program[header];
            if kind == 3u && (flags & 0x80000000u) != 0u {
                matches[id.x] = 1u;
                return;
            }
            if kind == 1u || (kind == 2u && (flags & program[header + 3u]) != 0u) {
                let transitions = program[header + 1u];
                let count = program[header + 2u];
                for (var i = 0u; i < count; i += 1u) {
                    let destination = program[transitions + i * 3u + 2u];
                    if scratch[base + destination] == 0u {
                        scratch[base + destination] = 1u;
                        scratch[queue_base + tail] = destination;
                        tail += 1u;
                    }
                }
            }
        }
        if position == length { break; }
        let byte = input[offset + position * 2u];
        for (var state = 0u; state < states; state += 1u) {
            let header = 2u + state * 4u;
            if scratch[base + state] != 0u && program[header] == 0u {
                let transitions = program[header + 1u];
                let count = program[header + 2u];
                for (var i = 0u; i < count; i += 1u) {
                    let transition = transitions + i * 3u;
                    if byte >= program[transition] && byte <= program[transition + 1u] {
                        scratch[next_base + program[transition + 2u]] = 1u;
                    }
                }
            }
        }
        for (var state = 0u; state < states; state += 1u) {
            scratch[base + state] = scratch[next_base + state];
            scratch[next_base + state] = 0u;
        }
    }
}
