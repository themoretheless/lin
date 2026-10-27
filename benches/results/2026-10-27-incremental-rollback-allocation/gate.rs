use lin::{Db,Cell};
use std::alloc::{GlobalAlloc,Layout,System};
use std::sync::atomic::{AtomicUsize,Ordering::Relaxed};
thread_local!{static ACTIVE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};}
static ALLOCS:AtomicUsize=AtomicUsize::new(0);static BYTES:AtomicUsize=AtomicUsize::new(0);static FREES:AtomicUsize=AtomicUsize::new(0);static FREE_BYTES:AtomicUsize=AtomicUsize::new(0);static REALLOCS:AtomicUsize=AtomicUsize::new(0);static REALLOC_BYTES:AtomicUsize=AtomicUsize::new(0);static FAILED:AtomicUsize=AtomicUsize::new(0);
fn active()->bool{ACTIVE.try_with(|a|a.get()).unwrap_or(false)}
struct Count;
unsafe impl GlobalAlloc for Count{
 unsafe fn alloc(&self,l:Layout)->*mut u8{let p=unsafe{System.alloc(l)};if active(){if p.is_null(){FAILED.fetch_add(1,Relaxed);}else{ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size(),Relaxed);}}p}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8{let p=unsafe{System.alloc_zeroed(l)};if active(){if p.is_null(){FAILED.fetch_add(1,Relaxed);}else{ALLOCS.fetch_add(1,Relaxed);BYTES.fetch_add(l.size(),Relaxed);}}p}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){if active(){FREES.fetch_add(1,Relaxed);FREE_BYTES.fetch_add(l.size(),Relaxed);}unsafe{System.dealloc(p,l)}}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8{let q=unsafe{System.realloc(p,l,n)};if active(){if q.is_null(){FAILED.fetch_add(1,Relaxed);}else{REALLOCS.fetch_add(1,Relaxed);REALLOC_BYTES.fetch_add(n,Relaxed);}}q}
}
#[global_allocator]static GLOBAL:Count=Count;
fn measured(f:impl FnOnce())->serde_json::Value{for c in [&ALLOCS,&BYTES,&FREES,&FREE_BYTES,&REALLOCS,&REALLOC_BYTES,&FAILED]{c.store(0,Relaxed);}ACTIVE.with(|a|a.set(true));f();ACTIVE.with(|a|a.set(false));assert_eq!(FAILED.load(Relaxed),0);serde_json::json!({"allocations":ALLOCS.load(Relaxed),"allocated_bytes":BYTES.load(Relaxed),"deallocations":FREES.load(Relaxed),"deallocated_bytes":FREE_BYTES.load(Relaxed),"reallocations":REALLOCS.load(Relaxed),"reallocated_bytes":REALLOC_BYTES.load(Relaxed),"failed_allocations":FAILED.load(Relaxed)})}
fn seeded(n:usize)->Db {
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();
    for start in (0..n).step_by(500) {
        let input=format!("insert docs [{}]",(start..(start+500).min(n)).map(|i|format!(r#"{{ id:"d-{i}",uri:"bench://{i}",title:"{}",layer:"wiki",wing:"{}",body:"body {i}",ts:timestamp({}) }}"#,if i%10==0 {format!("doc {i} wal note")}else{format!("doc {i} plain")},if i%2==0 {"rag"}else{"sys"},1700000000000i64+(i%2)as i64)).collect::<Vec<_>>().join(","));
        db.run(&input).unwrap();
    }
    assert_eq!(db.store.collection("docs").len(),n);db
}
fn main(){ACTIVE.with(|a|assert!(!a.get()));for n in [1000usize,10000,100000]{let mut db=seeded(n);let hash=db.store.get_by_id("docs","d-500").unwrap()["hash"].text().unwrap().to_owned();let a=db.prepare(&format!(r#"update docs[id=="d-500"] cas "{hash}" {{wing:"sys"}}"#)).unwrap();let b=db.prepare(&format!(r#"update docs[id=="d-500"] cas "{hash}" {{wing:"rag"}}"#)).unwrap();let expected=rmp_serde::to_vec_named(db.store.get_by_id("docs","d-500").unwrap()).unwrap();
for sample in 0..6{for mode in ["update_pair","rollback"]{let generation=db.store.r#gen;let counters=measured(||{if mode=="rollback"{let mut tx=db.transaction().unwrap();assert_eq!(tx.run_prepared(&a).unwrap().done.n,1);tx.rollback().unwrap();}else{assert_eq!(a.run(&mut db).unwrap().done.n,1);assert_eq!(b.run(&mut db).unwrap().done.n,1);}});assert_eq!(rmp_serde::to_vec_named(db.store.get_by_id("docs","d-500").unwrap()).unwrap(),expected);assert_eq!(db.scalar(r#"docs | wing == "rag" | count"#).unwrap(),Cell::Int((n/2)as i64));assert_eq!(db.store.r#gen,generation+if mode=="rollback"{0}else{2});println!("{}",serde_json::json!({"rows":n,"sample":sample,"mode":mode,"operations":if mode=="rollback"{1}else{2},"counters":counters,"exact_readback":true}));}}
}}
