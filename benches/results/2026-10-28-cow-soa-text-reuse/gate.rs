use lin::{Cell,Row,Store};
use lin::persist::Pack;
use std::alloc::{GlobalAlloc,Layout,System};
use std::sync::atomic::{AtomicUsize,Ordering::Relaxed};
struct Counter;
static ALLOCS:AtomicUsize=AtomicUsize::new(0);
static BYTES:AtomicUsize=AtomicUsize::new(0);
unsafe impl GlobalAlloc for Counter {
 unsafe fn alloc(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc(l)};if !p.is_null(){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size(),Relaxed);}p}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc_zeroed(l)};if !p.is_null(){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size(),Relaxed);}p}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){unsafe{System.dealloc(p,l)}}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 {let q=unsafe{System.realloc(p,l,n)};if !q.is_null(){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(n,Relaxed);}q}
}
#[global_allocator]static GLOBAL:Counter=Counter;
fn main(){
 let kind=std::env::args().nth(1).unwrap();assert!(["short","long","mixed"].contains(&kind.as_str()));
 let short="short";let long="long unchanged string exceeding twenty two bytes";
 let values=if kind=="short"{[short,short,short]}else if kind=="long"{[long,long,long]}else{[short,long,short]};
 let mut row=Row::new();row.insert("id".into(),Cell::from_text("soa-probe"));row.insert("uri".into(),Cell::from_text("soa://probe"));
 for(field,value)in["title","layer","wing"].into_iter().zip(values){row.insert(field.into(),Cell::from_text(value));}
 let mut store=Store::empty("soa-probe");store.apply_pack(&Pack::InsertBulk{collection:"docs".into(),rows:vec![row.clone()],edges:vec![]});
 let before=row.clone();let mut after=row.clone();after.insert("ts".into(),Cell::Int(17));
 // Isolate only SoA synchronization for an unchanged identity/text update.
 // This does not execute a database statement or measure throughput.
 let a=ALLOCS.load(Relaxed);let b=BYTES.load(Relaxed);
 for _ in 0..1000 {store.row_maps_sync_updated("docs",0,&before,&after);}
 let calls=ALLOCS.load(Relaxed)-a;let bytes=BYTES.load(Relaxed)-b;
 assert_eq!(store.docs_wing()[0].as_ref(),values[2]);assert_eq!(store.docs_title_contains_count(values[0]),1);assert_eq!(store.collection("docs")[0],row);
 let expected=0;assert_eq!(calls,expected);
 println!("{}",serde_json::json!({"shape":kind,"sync_calls":1000,"allocation_calls":calls,"requested_bytes":bytes,"expected_short_text_allocations":expected,"unchanged_text_verified":true,"database_update_benchmark":false,"performance_qualified":false}));
}
