@group(0) @binding(0) var<storage, read> query: array<f32>;
@group(0) @binding(1) var<storage, read> vectors: array<f32>;
@group(0) @binding(2) var<storage, read> lengths: array<u32>;
@group(0) @binding(3) var<storage, read_write> scores: array<f32>;
var<workgroup> scales: array<vec2<f32>, 64>;
var<workgroup> sums: array<vec3<f32>, 64>;

@compute @workgroup_size(64)
fn main(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    let row = group.x;
    let dim = arrayLength(&query);
    var scale = vec2<f32>(0.0);
    for (var i = lane; i < lengths[row]; i += 64u) {
        scale = max(scale, abs(vec2<f32>(query[i], vectors[row * dim + i])));
    }
    scales[lane] = scale;
    workgroupBarrier();
    for (var step = 32u; step > 0u; step /= 2u) {
        if lane < step { scales[lane] = max(scales[lane], scales[lane + step]); }
        workgroupBarrier();
    }
    scale = scales[0];
    var sum = vec3<f32>(0.0);
    if scale.x > 0.0 && scale.y > 0.0 {
        for (var i = lane; i < lengths[row]; i += 64u) {
            let a = query[i] / scale.x;
            let b = vectors[row * dim + i] / scale.y;
            sum += vec3<f32>(a * b, a * a, b * b);
        }
    }
    sums[lane] = sum;
    workgroupBarrier();
    for (var step = 32u; step > 0u; step /= 2u) {
        if lane < step { sums[lane] += sums[lane + step]; }
        workgroupBarrier();
    }
    if lane == 0u {
        sum = sums[0];
        var score = 0.0;
        if sum.y > 0.0 && sum.z > 0.0 {
            if scale.x > sqrt(1e-18 / sum.y) && scale.y > sqrt(1e-18 / sum.z) {
                score = clamp(sum.x / (sqrt(sum.y) * sqrt(sum.z)), -1.0, 1.0);
            }
        }
        scores[row] = score;
    }
}
