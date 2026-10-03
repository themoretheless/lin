use std::{hint::black_box, time::Instant};
fn main() {
    let started = Instant::now();
    let embedder = lin::HashingEmbedder::new("test/768", 768);
    black_box(embedder);
    let cold = started.elapsed().as_nanos();
    let started = Instant::now();
    for _ in 0..1000 { black_box(lin::HashingEmbedder::new("test/768", 768)); }
    println!("{{\"cold_constructor_ns\":{},\"warm_constructor_ns\":{}}}", cold, started.elapsed().as_nanos()/1000);
}
