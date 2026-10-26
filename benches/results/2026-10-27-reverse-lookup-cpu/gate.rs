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
fn main(){for n in [1000usize,10000,100000]{
 let db=seeded(n);let index=db.store.indexes.values().find(|v|v.def.collection=="docs"&&v.reverse.len()==n).unwrap();let hash=&index.reverse;
 let mut dense=vec![None;n];for(&pos,key)in hash{dense[pos]=Some(key.clone());}for(&pos,key)in hash{assert_eq!(dense[pos].as_ref(),Some(key));}
 for kind in ["same","spread"]{let mut state=17u64;let positions=(0..4096).map(|_|{state=state.wrapping_mul(6364136223846793005).wrapping_add(1);if kind=="same"{500}else{state as usize%n}}).collect::<Vec<_>>();
 for sample in 0..24{let mut times=[0.;3];for offset in 0..3{let mode=(sample+offset)%3;let t=std::time::Instant::now();for &pos in &positions{let p=std::hint::black_box(pos);if mode==2{std::hint::black_box(dense[p].as_ref());}else{std::hint::black_box(hash.get(&p));}}times[mode]=t.elapsed().as_nanos()as f64/4096.;}
 println!("{}",serde_json::json!({"rows":n,"kind":kind,"sample":sample,"baseline_ns":times[0],"control_ns":times[1],"candidate_ns":times[2],"dense_requested_bytes":dense.capacity()*std::mem::size_of_val(&dense[0]),"hash_capacity":hash.capacity(),"exact_lookup":true}));}
 }
}}
