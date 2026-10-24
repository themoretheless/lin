use lin::{Embedder,HashingEmbedder};
use std::{alloc::{GlobalAlloc,Layout,System},sync::atomic::{AtomicBool,AtomicU64,Ordering::Relaxed},hint::black_box};
struct Tracked;
static ACTIVE:AtomicBool=AtomicBool::new(false);
static ALLOCS:AtomicU64=AtomicU64::new(0);static ALLOC_BYTES:AtomicU64=AtomicU64::new(0);
static FREES:AtomicU64=AtomicU64::new(0);static FREE_BYTES:AtomicU64=AtomicU64::new(0);
static REALLOCS:AtomicU64=AtomicU64::new(0);static REALLOC_OLD:AtomicU64=AtomicU64::new(0);static REALLOC_NEW:AtomicU64=AtomicU64::new(0);
unsafe impl GlobalAlloc for Tracked {
 unsafe fn alloc(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc(l)};if !p.is_null()&&ACTIVE.load(Relaxed){ALLOCS.fetch_add(1,Relaxed);ALLOC_BYTES.fetch_add(l.size()as u64,Relaxed);}p}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc_zeroed(l)};if !p.is_null()&&ACTIVE.load(Relaxed){ALLOCS.fetch_add(1,Relaxed);ALLOC_BYTES.fetch_add(l.size()as u64,Relaxed);}p}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){if ACTIVE.load(Relaxed){FREES.fetch_add(1,Relaxed);FREE_BYTES.fetch_add(l.size()as u64,Relaxed);}unsafe{System.dealloc(p,l)}}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 {let out=unsafe{System.realloc(p,l,n)};if !out.is_null()&&ACTIVE.load(Relaxed){REALLOCS.fetch_add(1,Relaxed);REALLOC_OLD.fetch_add(l.size()as u64,Relaxed);REALLOC_NEW.fetch_add(n as u64,Relaxed);}out}
}
#[global_allocator]static ALLOCATOR:Tracked=Tracked;
fn start(){assert!(!ACTIVE.load(Relaxed));for c in [&ALLOCS,&ALLOC_BYTES,&FREES,&FREE_BYTES,&REALLOCS,&REALLOC_OLD,&REALLOC_NEW]{c.store(0,Relaxed);}ACTIVE.store(true,Relaxed);}
fn stop()->serde_json::Value {ACTIVE.store(false,Relaxed);serde_json::json!({"alloc_calls":ALLOCS.load(Relaxed),"allocated_bytes":ALLOC_BYTES.load(Relaxed),"free_calls":FREES.load(Relaxed),"freed_bytes":FREE_BYTES.load(Relaxed),"realloc_calls":REALLOCS.load(Relaxed),"realloc_old_bytes":REALLOC_OLD.load(Relaxed),"realloc_new_bytes":REALLOC_NEW.load(Relaxed)})}
fn fixture(n:usize)->Vec<String>{(0..n).map(|i|if i%10==0 {format!("doc {i} wal note body {i}")}else{format!("doc {i} plain body {i}")}).collect()}
fn main(){
 // Observer sensitivity: retained heap storage and explicit growth/destruction.
 start();let mut probe=Vec::<u8>::with_capacity(123);probe.resize(123,7);black_box(probe.as_ptr());probe.reserve_exact(1000);black_box(probe.as_ptr());drop(probe);let sensitivity=stop();assert_eq!(sensitivity["alloc_calls"],1);assert_eq!(sensitivity["allocated_bytes"],123);assert_eq!(sensitivity["realloc_calls"],1);assert_eq!(sensitivity["free_calls"],1);
 let case=std::env::args().nth(1).unwrap();let n=if case=="default_10k"{10000}else{1000};let dim=if case=="small_dim"{8}else{768};
 let texts=if case=="unicode_1k"{vec!["ЁЖ ΟΣ İ wal\u{b}note a\u{a0}b ".repeat(8);n]}else{fixture(n)};let refs:Vec<&str>=texts.iter().map(String::as_str).collect();
 start();let emb=HashingEmbedder::new("allocation-test",dim);let constructor=stop();
 start();let cold=emb.embed_batch(&refs);let cold_alloc=stop();assert_eq!(cold.len(),n);assert!(cold.iter().all(|v|v.len()==dim));let reference=cold.clone();
 start();drop(cold);let shared_drop=stop(); // reference retains every Arc.
 let mut warm=Vec::new();
 for sample in 0..12 {
 start();let out=emb.embed_batch(&refs);let alloc=stop();assert_eq!(out.len(),reference.len());assert!(out.iter().zip(&reference).all(|(a,b)|a.iter().zip(b.iter()).all(|(a,b)|a.to_bits()==b.to_bits())));
 start();drop(out);let release=stop();warm.push(serde_json::json!({"sample":sample,"allocation":alloc,"release":release,"exact_bits":true}));
 }
 start();drop(reference);let final_release=stop();
 println!("{}",serde_json::json!({"case":case,"rows":n,"dim":dim,"observer_sensitivity":sensitivity,"constructor":constructor,"cold_batch":cold_alloc,"cold_shared_output_drop":shared_drop,"warm_batches":warm,"cold_final_output_release":final_release}));
}
