use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::hint::black_box;
use lin::{Cell,Row};
struct Count;
static ACTIVE: AtomicBool=AtomicBool::new(false);
static ALLOCS: AtomicU64=AtomicU64::new(0);
static BYTES: AtomicU64=AtomicU64::new(0);
static FREES: AtomicU64=AtomicU64::new(0);
static FREE_BYTES: AtomicU64=AtomicU64::new(0);
unsafe impl GlobalAlloc for Count {
 unsafe fn alloc(&self,l:Layout)->*mut u8 { if ACTIVE.load(Relaxed) { ALLOCS.fetch_add(1,Relaxed); BYTES.fetch_add(l.size()as u64,Relaxed); } unsafe { System.alloc(l) } }
 unsafe fn dealloc(&self,p:*mut u8,l:Layout) { if ACTIVE.load(Relaxed) { FREES.fetch_add(1,Relaxed); FREE_BYTES.fetch_add(l.size()as u64,Relaxed); } unsafe { System.dealloc(p,l) } }
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 { if ACTIVE.load(Relaxed) { ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(n as u64,Relaxed);FREES.fetch_add(1,Relaxed);FREE_BYTES.fetch_add(l.size()as u64,Relaxed); } unsafe { System.realloc(p,l,n) } }
}
#[global_allocator]static ALLOC:Count=Count;
fn measure(name:&str,n:usize,mut f:impl FnMut())->serde_json::Value {
 for x in [&ALLOCS,&BYTES,&FREES,&FREE_BYTES] { x.store(0,Relaxed); }
 ACTIVE.store(true,Relaxed);
 for _ in 0..n { f(); }
 ACTIVE.store(false,Relaxed);
 serde_json::json!({"case":name,"operations":n,"allocations":ALLOCS.load(Relaxed),"requested_bytes":BYTES.load(Relaxed),"deallocations":FREES.load(Relaxed),"deallocated_bytes":FREE_BYTES.load(Relaxed)})
}
fn fill(source:&Row,mut dest:Row)->Row { dest.extend(source.iter().map(|(k,v)|(k.clone(),v.clone())));dest }
fn main() {
 let source=Row::from([
  ("id".into(),Cell::Text("d-20".into())),("uri".into(),Cell::Text("bench://20".into())),
  ("title".into(),Cell::Text("doc 20 wal note".into())),("layer".into(),Cell::Text("wiki".into())),
  ("wing".into(),Cell::Text("rag".into())),("body".into(),Cell::Text("body 20".into())),
  ("ts".into(),Cell::Time(123456)),("hash".into(),Cell::Text("h:sample".into())),
  ("embedding".into(),Cell::Vec(vec![0f32;768].into())),
 ]);
 assert_eq!(source.len(),9);
 let store=lin::Store::empty("allocation-probe");
 let mut out=vec![];
 out.push(measure("tls_acquire_fill_release_cold",1,||{
  let row=fill(&source,store.acquire_pooled_row());lin::Store::release_pooled_row(black_box(row));
 }));
 let check=fill(&source,store.acquire_pooled_row());assert_eq!(check,source);lin::Store::release_pooled_row(check);
 let mut target=source.clone();target.clone_from(&source);assert_eq!(target,source);
 for repeat in 0..3 {
  for mut record in [
   measure("clone_drop",1000,||drop(black_box(source.clone()))),
   measure("clone_from_held_destination",1000,||{target.clone_from(&source);black_box(&target);}),
   measure("tls_acquire_fill_release_warm",1000,||{let row=fill(&source,store.acquire_pooled_row());lin::Store::release_pooled_row(black_box(row));}),
  ] { record["repeat"]=repeat.into();out.push(record); }
 }
 assert_eq!(target,source);
 assert!(store.acquire_pooled_row().is_empty());
 println!("{}",serde_json::to_string_pretty(&out).unwrap());
}
