@group(0) @binding(0) var<storage, read> edges: array<vec4<u32>>;
@group(0) @binding(1) var<storage, read> params: array<u32>;
@group(0) @binding(2) var<storage, read_write> state: array<u32>;
@group(0) @binding(3) var<storage, read_write> output: array<u32>;
@compute @workgroup_size(1)
fn main() {
    let nodes = params[0];
    let graph = params[2] != 0u;
    for (var i = 4u; i < arrayLength(&params); i += 1u) {
        state[params[i]] = 1u;
        state[nodes + params[i]] = 1u;
    }
    var reached = 0u;
    var emitted = 0u;
    for (var depth = 0u; depth < params[1]; depth += 1u) {
        for (var i = 0u; i < nodes; i += 1u) { state[2u*nodes+i] = 0u; }
        for (var i = 0u; i < arrayLength(&edges); i += 1u) {
            let edge = edges[i];
            if state[nodes + edge.x] == 0u { continue; }
            if graph && state[4u*nodes+edge.z] == 0u {
                state[4u*nodes+edge.z] = 1u;
                output[1u+emitted] = i;
                emitted += 1u;
                output[0] = emitted;
                if emitted >= 300u { return; }
            }
            if state[edge.y] == 0u {
                state[edge.y] = 1u;
                state[2u*nodes+edge.y] = 1u;
                state[3u*nodes+edge.y] = 1u;
                reached += 1u;
            }
        }
        if !graph && reached >= 300u { break; }
        var any = false;
        for (var i = 0u; i < nodes; i += 1u) {
            state[nodes+i] = state[2u*nodes+i];
            any = any || state[nodes+i] != 0u;
        }
        if !any { break; }
    }
    if !graph {
        for (var i = 0u; i < nodes; i += 1u) { output[1u+i] = state[3u*nodes+i]; }
    }
}
