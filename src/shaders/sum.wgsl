// IEEE-754 binary64 addition using paired u32 limbs. One invocation owns
// a group and visits its values in source order, including across dispatches.
struct Record { group_id: u32, lo: u32, hi: u32 }
@group(0) @binding(0) var<storage, read> records: array<Record>;
@group(0) @binding(1) var<storage, read_write> totals: array<vec2<u32>>;
fn nonzero(a: vec2<u32>) -> bool { return (a.x | a.y) != 0u; }
fn less(a: vec2<u32>, b: vec2<u32>) -> bool { return a.y < b.y || (a.y == b.y && a.x < b.x); }
fn plus(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let lo = a.x + b.x;
    return vec2<u32>(lo, a.y + b.y + select(0u, 1u, lo < a.x));
}
fn minus(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    return vec2<u32>(a.x - b.x, a.y - b.y - select(0u, 1u, a.x < b.x));
}
fn left_one(a: vec2<u32>) -> vec2<u32> { return vec2<u32>(a.x << 1u, (a.y << 1u) | (a.x >> 31u)); }
fn right_jam(a: vec2<u32>, distance: u32) -> vec2<u32> {
    if distance == 0u { return a; }
    if distance < 32u {
        let sticky = select(0u, 1u, (a.x << (32u - distance)) != 0u);
        return vec2<u32>((a.x >> distance) | (a.y << (32u - distance)) | sticky, a.y >> distance);
    }
    if distance == 32u { return vec2<u32>(a.y | select(0u, 1u, a.x != 0u), 0u); }
    if distance < 64u {
        let shift = distance - 32u;
        let sticky = select(0u, 1u, a.x != 0u || (a.y << (32u - shift)) != 0u);
        return vec2<u32>((a.y >> shift) | sticky, 0u);
    }
    return vec2<u32>(select(0u, 1u, nonzero(a)), 0u);
}
fn add_double(a: vec2<u32>, b: vec2<u32>) -> vec2<u32> {
    let ea = (a.y >> 20u) & 2047u;
    let eb = (b.y >> 20u) & 2047u;
    let sa = a.y >> 31u;
    let sb = b.y >> 31u;
    let fa = vec2<u32>(a.x, a.y & 0xfffffu);
    let fb = vec2<u32>(b.x, b.y & 0xfffffu);
    if ea == 2047u || eb == 2047u {
        if (ea == 2047u && nonzero(fa)) || (eb == 2047u && nonzero(fb)) ||
            (ea == 2047u && eb == 2047u && sa != sb) {
            return vec2<u32>(0u, 0x7ff80000u);
        }
        if ea == 2047u { return a; }
        return b;
    }
    var xa = vec2<u32>(a.x, fa.y | select(0u, 0x100000u, ea != 0u));
    var xb = vec2<u32>(b.x, fb.y | select(0u, 0x100000u, eb != 0u));
    // Three low bits retain guard, round and sticky information.
    xa = vec2<u32>(xa.x << 3u, (xa.y << 3u) | (xa.x >> 29u));
    xb = vec2<u32>(xb.x << 3u, (xb.y << 3u) | (xb.x >> 29u));
    let exponent_a = max(ea, 1u);
    let exponent_b = max(eb, 1u);
    var exponent = max(exponent_a, exponent_b);
    xa = right_jam(xa, exponent - exponent_a);
    xb = right_jam(xb, exponent - exponent_b);
    var sign = sa;
    var sig: vec2<u32>;
    if sa == sb {
        sig = plus(xa, xb);
        if (sig.y & 0x1000000u) != 0u {
            sig = right_jam(sig, 1u);
            exponent += 1u;
        }
    } else {
        if less(xa, xb) { sig = minus(xb, xa); sign = sb; }
        else { sig = minus(xa, xb); }
        if !nonzero(sig) { return vec2<u32>(0u); }
        loop {
            if exponent <= 1u || (sig.y & 0x800000u) != 0u { break; }
            sig = left_one(sig);
            exponent -= 1u;
        }
    }
    let round_bits = sig.x & 7u;
    var rounded = vec2<u32>((sig.x >> 3u) | (sig.y << 29u), sig.y >> 3u);
    if round_bits > 4u || (round_bits == 4u && (rounded.x & 1u) != 0u) {
        rounded = plus(rounded, vec2<u32>(1u, 0u));
    }
    if (rounded.y & 0x200000u) != 0u {
        rounded = right_jam(rounded, 1u);
        exponent += 1u;
    }
    if exponent >= 2047u { return vec2<u32>(0u, (sign << 31u) | 0x7ff00000u); }
    if (rounded.y & 0x100000u) == 0u { exponent = 0u; }
    return vec2<u32>(rounded.x, (sign << 31u) | (exponent << 20u) | (rounded.y & 0xfffffu));
}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&totals) { return; }
    var total = totals[id.x];
    for (var i = 0u; i < arrayLength(&records); i += 1u) {
        let r = records[i];
        if r.group_id == id.x { total = add_double(total, vec2<u32>(r.lo, r.hi)); }
    }
    totals[id.x] = total;
}

// Correctly rounded 1 / positive u32, without native floating point.
fn reciprocal_u32(n: u32) -> vec2<u32> {
    let k = 31u - countLeadingZeros(n);
    if (n & (n - 1u)) == 0u {
        return vec2<u32>(0u, (1023u - k) << 20u);
    }
    // Divide 2^(k+53) by n, collecting 53 significand bits and a remainder.
    var remainder = vec2<u32>(0u);
    var quotient = vec2<u32>(0u);
    for (var step = 0u; step <= k + 53u; step += 1u) {
        remainder = left_one(remainder);
        if step == 0u { remainder.x = 1u; }
        quotient = left_one(quotient);
        if remainder.y != 0u || remainder.x >= n {
            remainder = minus(remainder, vec2<u32>(n, 0u));
            quotient.x |= 1u;
        }
    }
    let twice = left_one(remainder);
    if twice.y != 0u || twice.x > n || (twice.x == n && (quotient.x & 1u) != 0u) {
        quotient = plus(quotient, vec2<u32>(1u, 0u));
    }
    var exponent = 1022u - k;
    if (quotient.y & 0x200000u) != 0u {
        quotient = right_jam(quotient, 1u);
        exponent += 1u;
    }
    return vec2<u32>(quotient.x, (exponent << 20u) | (quotient.y & 0xfffffu));
}
@compute @workgroup_size(64)
fn rrf(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&totals) { return; }
    var total = totals[id.x];
    for (var i = 0u; i < arrayLength(&records); i += 1u) {
        let r = records[i];
        if r.group_id == id.x { total = add_double(total, reciprocal_u32(r.lo)); }
    }
    totals[id.x] = total;
}
