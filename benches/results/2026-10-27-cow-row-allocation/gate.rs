use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use lin::{Db, Cell};
struct Count;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ACTIVE.load(Relaxed) { ALLOCS.fetch_add(1,Relaxed); BYTES.fetch_add(l.size(),Relaxed); }
        unsafe {System.alloc(l)}
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if ACTIVE.load(Relaxed) { ALLOCS.fetch_add(1,Relaxed); BYTES.fetch_add(l.size(),Relaxed); }
        unsafe {System.alloc_zeroed(l)}
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if ACTIVE.load(Relaxed) { FREES.fetch_add(1,Relaxed); }
        unsafe {System.dealloc(p,l)}
    }
    unsafe fn realloc(&self, p:*mut u8, old:Layout, n:usize)->*mut u8 {
        if ACTIVE.load(Relaxed) { ALLOCS.fetch_add(1,Relaxed); BYTES.fetch_add(n,Relaxed); }
        unsafe {System.realloc(p,old,n)}
    }
}
#[global_allocator] static GLOBAL: Count = Count;
fn seeded(n:usize)->Db {
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();
    for start in (0..n).step_by(500) {
        let input=format!("insert docs [{}]",(start..(start+500).min(n)).map(|i|format!(r#"{{ id:"d-{i}",uri:"bench://{i}",title:"{}",layer:"wiki",wing:"{}",body:"body {i}",ts:timestamp({}) }}"#,if i%10==0 {format!("doc {i} wal note")}else{format!("doc {i} plain")},if i%2==0 {"rag"}else{"sys"},1700000000000i64+(i%2)as i64)).collect::<Vec<_>>().join(","));
        db.run(&input).unwrap();
    }
    assert_eq!(db.store.collection("docs").len(),n);db
}
fn main(){
    for n in [1000usize,10000,100000] {for kind in ["wing","title","delete","rollback"] {
        let mut db=seeded(n);let id="d-500";let old=db.store.get_by_id("docs",id).unwrap().clone();
        let hash=old["hash"].text().unwrap();let query=match kind {
            "wing"|"rollback"=>format!(r#"update docs[id=="{id}"] cas "{hash}" {{wing:"sys"}}"#),
            "title"=>format!(r#"update docs[id=="{id}"] cas "{hash}" {{title:"new unique replacement"}}"#),
            _=>format!(r#"delete docs[id=="{id}"] cas "{hash}""#)
        };let prepared=db.prepare(&query).unwrap();let before_gen=db.stats().r#gen;
        ALLOCS.store(0,Relaxed);BYTES.store(0,Relaxed);FREES.store(0,Relaxed);
        ACTIVE.store(true,Relaxed);
        let affected=if kind=="rollback" {
            let mut tx=db.transaction().unwrap();let result=tx.run_prepared(&prepared).unwrap();let n=result.done.n;std::hint::black_box(&result);drop(result);tx.rollback().unwrap();n
        }else{let result=prepared.run(&mut db).unwrap();let n=result.done.n;std::hint::black_box(&result);drop(result);n};
        ACTIVE.store(false,Relaxed);
        let counts=(ALLOCS.load(Relaxed),BYTES.load(Relaxed),FREES.load(Relaxed));
        assert_eq!(affected,1);assert_eq!(db.stats().r#gen,before_gen+1);
        if kind=="delete" {assert!(db.store.get_by_id("docs",id).is_none());assert_eq!(db.store.collection("docs").len(),n-1);}
        else {let updated=db.store.get_by_id("docs",id).unwrap();if kind=="rollback"{assert_eq!(updated,&old);}else if kind=="wing"{assert_eq!(updated["wing"].text(),Some("sys"));assert_eq!(updated["hash"],old["hash"]);}else{assert_eq!(updated["title"].text(),Some("new unique replacement"));assert_eq!(updated["hash"],old["hash"]);}}
        let indexed=db.run("docs | wing == \"rag\" | count").unwrap();let expected=n/2-usize::from(kind=="wing"||kind=="delete");assert_eq!(indexed.rows[0].values().next().and_then(Cell::as_int),Some(expected as i64));
        println!("{}",serde_json::json!({"rows":n,"case":kind,"allocations":counts.0,"requested_bytes":counts.1,"deallocations":counts.2,"exact_readback":true,"handle_drop_included":true}));
    }}
}
