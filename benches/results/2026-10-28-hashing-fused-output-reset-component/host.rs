fn main(){ println!("{{\"rust_available_parallelism\":{},\"candidate_fused_output_reset\":true}}",std::thread::available_parallelism().map_or(1,|n|n.get())); }
