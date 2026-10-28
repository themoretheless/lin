fn main(){ println!("{{\"rust_available_parallelism\":{},\"candidate_sparse_output\":true}}",std::thread::available_parallelism().map_or(1,|n|n.get())); }
