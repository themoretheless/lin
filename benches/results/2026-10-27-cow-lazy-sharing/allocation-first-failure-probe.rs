use std::alloc::{GlobalAlloc,Layout,System};
use std::sync::atomic::{AtomicBool,AtomicU64,Ordering::Relaxed};
use std::collections::BTreeMap;
use std::hint::black_box;
use lin::{Cell,Row};
struct Count;
static ACTIVE:AtomicBool=AtomicBool::new(false);
static ALLOCS:AtomicU64=AtomicU64::new(0);
static BYTES:AtomicU64=AtomicU64::new(0);
static FREES:AtomicU64=AtomicU64::new(0);
static FREE_BYTES:AtomicU64=AtomicU64::new(0);
unsafe impl GlobalAlloc for Count {
 unsafe fn alloc(&self,l:Layout)->*mut u8{if ACTIVE.load(Relaxed){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size()as u64,Relaxed);}unsafe{System.alloc(l)}}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){if ACTIVE.load(Relaxed){FREES.fetch_add(1,Relaxed);FREE_BYTES.fetch_add(l.size()as u64,Relaxed);}unsafe{System.dealloc(p,l)}}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8{if ACTIVE.load(Relaxed){ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(n as u64,Relaxed);FREES.fetch_add(1,Relaxed);FREE_BYTES.fetch_add(l.size()as u64,Relaxed);}unsafe{System.realloc(p,l,n)}}
}
#[global_allocator]static ALLOC:Count=Count;
fn measure(name:&str,repeat:usize,mut f:impl FnMut())->serde_json::Value {
 for c in [&ALLOCS,&BYTES,&FREES,&FREE_BYTES]{c.store(0,Relaxed)}
 ACTIVE.store(true,Relaxed);for _ in 0..1000{f()}ACTIVE.store(false,Relaxed);
 serde_json::json!({"case":name,"repeat":repeat,"operations":1000,"allocations":ALLOCS.load(Relaxed),"requested_bytes":BYTES.load(Relaxed),"deallocations":FREES.load(Relaxed),"deallocated_bytes":FREE_BYTES.load(Relaxed)})
}
fn make(source:&BTreeMap<String,Cell>)->Row{source.iter().map(|(k,v)|(k.clone(),v.clone())).collect()}
fn main(){
 let nine=BTreeMap::from([
  ("id".into(),Cell::Text("d-20".into())),("uri".into(),Cell::Text("bench://20".into())),
  ("title".into(),Cell::Text("doc 20 wal note".into())),("layer".into(),Cell::Text("wiki".into())),
  ("wing".into(),Cell::Text("rag".into())),("body".into(),Cell::Text("body 20".into())),
  ("ts".into(),Cell::Time(123456)),("hash".into(),Cell::Text("h:sample".into())),("embedding".into(),Cell::Vec(vec![0.;768].into())),
 ]);let one=BTreeMap::from([("n".into(),Cell::Int(5000))]);let raw=make(&nine);assert_eq!(raw.len(),9);
 assert_eq!(raw.iter().collect::<Vec<_>>(),nine.iter().collect::<Vec<_>>());assert_eq!(make(&one).iter().collect::<Vec<_>>(),one.iter().collect::<Vec<_>>());
 let mut db=lin::Db::empty();db.run(r#"insert docs { id:"a",uri:"u:a",title:"a",body:"a",wing:"rag" }"#).unwrap();let hash=db.store.get_by_id("docs","a").unwrap()["hash"].text().unwrap().to_owned();
 let handle=db.run(&format!(r#"update docs[id=="a"] cas "{hash}" {{wing:"sys"}}"#)).unwrap();let promoted=&handle.rows[0];assert_eq!(promoted["wing"].text(),Some("sys"));assert_eq!(promoted.len(),9);
 let mut changed=promoted.clone();*changed.get_mut("wing").unwrap()=Cell::Int(42);assert_eq!(promoted["wing"].text(),Some("sys"));drop(changed);
 let mut records=Vec::new();
 for repeat in 0..3{
  records.push(measure("empty_row_new_drop",repeat,||drop(black_box(Row::new()))));
  records.push(measure("one_cell_construct_drop",repeat,||drop(black_box(make(&one)))));
  records.push(measure("nine_cell_construct_drop",repeat,||drop(black_box(make(&nine)))));
  records.push(measure("unpromoted_row_clone_drop",repeat,||drop(black_box(raw.clone()))));
  records.push(measure("returned_update_row_clone_drop",repeat,||drop(black_box(promoted.clone()))));
  records.push(measure("returned_update_row_detach_drop",repeat,||{let mut row=promoted.clone();*row.get_mut("wing").unwrap()=Cell::Int(42);drop(black_box(row));}));
 }
 assert_eq!(promoted["wing"].text(),Some("sys"));assert_eq!(raw.iter().collect::<Vec<_>>(),nine.iter().collect::<Vec<_>>());
 println!("{}",serde_json::to_string_pretty(&serde_json::json!({"row_size_bytes":std::mem::size_of::<Row>(),"map_size_bytes":std::mem::size_of::<BTreeMap<String,Cell>>(),"fixture_fields":nine.len(),"records":records,"scope":"allocator calls/requested bytes for primitive construction or clone/drop; fixture/Db lifetime excluded; no CPU/RSS claim"})).unwrap());
}
