@group(0) @binding(0) var<storage, read> edges: array<vec4<u32>>;
@group(0) @binding(1) var<storage, read> params: array<u32>;
@group(0) @binding(2) var<storage, read_write> stack: array<u32>;
@group(0) @binding(3) var<storage, read_write> output: array<u32>;
@compute @workgroup_size(1)
fn main() {
    stack[0] = params[0]; stack[1] = 0u; stack[2] = 0xffffffffu;
    stack[3] = 0u; stack[4] = 0u; stack[5] = 0u;
    var top = 1u;
    var emitted = 0u;
    loop {
        if top == 0u { break; }
        top -= 1u;
        let frame = top * 6u;
        let at = stack[frame];
        let depth = stack[frame+1u];
        let last_edge = stack[frame+2u];
        let ancestors = vec3<u32>(stack[frame+3u],stack[frame+4u],stack[frame+5u]);
        if depth > 0u && depth >= params[1] && depth <= params[2] && params[4u+at] != 0u {
            output[2u+emitted*2u] = at;
            output[3u+emitted*2u] = last_edge;
            emitted += 1u;
            output[0] = emitted;
            if emitted >= params[3] { return; }
        }
        if depth >= params[2] { continue; }
        for (var i = 0u; i < arrayLength(&edges); i += 1u) {
            let edge = edges[i];
            if edge.x != at || edge.y == at { continue; }
            var visited = false;
            for (var j = 0u; j < depth; j += 1u) { visited = visited || ancestors[j] == edge.y; }
            if visited { continue; }
            if (top+1u)*6u > arrayLength(&stack) { output[1]=1u; return; }
            var path = ancestors; path[depth] = at;
            let child = top*6u;
            stack[child]=edge.y; stack[child+1u]=depth+1u; stack[child+2u]=i;
            stack[child+3u]=path.x; stack[child+4u]=path.y; stack[child+5u]=path.z;
            top += 1u;
        }
    }
}
