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
 for n in [1000usize,10000,100000] {
  let mut db=seeded(n);
  let prepared=(0..512).map(|i|{let id=format!("d-{i}");let row=db.store.get_by_id("docs",&id).unwrap();db.prepare(&format!(r#"delete docs[id=="{id}"] cas "{}""#,row["hash"].text().unwrap())).unwrap()}).collect::<Vec<_>>();
  for sample in 0..16 {
   let originals=(sample*32..sample*32+32).map(|i|db.store.get_by_id("docs",&format!("d-{i}")).unwrap().clone()).collect::<Vec<_>>();
   let mut tx=db.transaction().unwrap();
   let t=std::time::Instant::now();
   for prep in &prepared[sample*32..sample*32+32] {let h=tx.run_prepared(prep).unwrap();assert_eq!(h.done.n,1);std::hint::black_box(&h);drop(h);}
   let ns=t.elapsed().as_nanos() as f64/32.;
   tx.rollback().unwrap();
   assert_eq!(db.store.collection("docs").len(),n);
   for row in &originals {assert_eq!(db.store.get_by_id("docs",row["id"].text().unwrap()).unwrap(),row);}
   let h=db.run("docs | wing == \"rag\" | count").unwrap();assert_eq!(h.rows[0].values().next().and_then(Cell::as_int),Some((n/2)as i64));
   println!("{}",serde_json::json!({"rows":n,"sample":sample,"ns_per_delete":ns,"operations":32,"exact_readback":true}));
  }
 }
}
