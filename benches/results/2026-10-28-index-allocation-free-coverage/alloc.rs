use std::{alloc::{GlobalAlloc,Layout,System},sync::atomic::{AtomicUsize,Ordering::Relaxed},hint::black_box};
struct Counter;static CALLS:AtomicUsize=AtomicUsize::new(0);static BYTES:AtomicUsize=AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counter {
 unsafe fn alloc(&self,l:Layout)->*mut u8{let p=unsafe{System.alloc(l)};if !p.is_null(){CALLS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size(),Relaxed);}p}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8{let p=unsafe{System.alloc_zeroed(l)};if !p.is_null(){CALLS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size(),Relaxed);}p}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8{let q=unsafe{System.realloc(p,l,n)};if !q.is_null(){CALLS.fetch_add(1,Relaxed);BYTES.fetch_add(n,Relaxed);}q}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){unsafe{System.dealloc(p,l)}}
}
#[global_allocator]static GLOBAL:Counter=Counter;
fn main(){let args:Vec<String>=std::env::args().collect();let shape=&args[1];let probe=lin::__coverage_alloc_probe::Probe::new(shape);let a=CALLS.load(Relaxed);let b=BYTES.load(Relaxed);let mut matched=0;for _ in 0..1000{matched+=black_box(black_box(&probe).run())as usize;}let calls=CALLS.load(Relaxed)-a;let bytes=BYTES.load(Relaxed)-b;assert_eq!(matched,1000);println!("{}",serde_json::json!({"shape":shape,"calls":1000,"matched":matched,"allocation_calls":calls,"requested_bytes":bytes,"performance_qualified":false,"query_benchmark":false}));}
