use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize,AtomicIsize,Ordering::Relaxed};
use lin::Db;
struct Count;
static LIVE: AtomicIsize=AtomicIsize::new(0);
static ALLOCS:AtomicUsize=AtomicUsize::new(0);
static REQUESTED:AtomicUsize=AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
 unsafe fn alloc(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc(l)};if !p.is_null(){LIVE.fetch_add(l.size() as isize,Relaxed);ALLOCS.fetch_add(1,Relaxed);REQUESTED.fetch_add(l.size(),Relaxed);}p}
 unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8 {let p=unsafe{System.alloc_zeroed(l)};if !p.is_null(){LIVE.fetch_add(l.size() as isize,Relaxed);ALLOCS.fetch_add(1,Relaxed);REQUESTED.fetch_add(l.size(),Relaxed);}p}
 unsafe fn dealloc(&self,p:*mut u8,l:Layout){LIVE.fetch_sub(l.size() as isize,Relaxed);unsafe{System.dealloc(p,l)}}
 unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8 {let q=unsafe{System.realloc(p,l,n)};if !q.is_null(){LIVE.fetch_add(n as isize-l.size() as isize,Relaxed);ALLOCS.fetch_add(1,Relaxed);REQUESTED.fetch_add(n,Relaxed);}q}
}
#[global_allocator]static GLOBAL:Count=Count;
fn counts()->(isize,usize,usize){(LIVE.load(Relaxed),ALLOCS.load(Relaxed),REQUESTED.load(Relaxed))}
fn seeded(n:usize)->Db {
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();
    for start in (0..n).step_by(500) {
        let input=format!("insert docs [{}]",(start..(start+500).min(n)).map(|i|format!(r#"{{ id:"d-{i}",uri:"bench://{i}",title:"{}",layer:"wiki",wing:"{}",body:"body {i}",ts:timestamp({}) }}"#,if i%10==0 {format!("doc {i} wal note")}else{format!("doc {i} plain")},if i%2==0 {"rag"}else{"sys"},1700000000000i64+(i%2)as i64)).collect::<Vec<_>>().join(","));
        db.run(&input).unwrap();
    }
    assert_eq!(db.store.collection("docs").len(),n);db
}

fn main(){
 let n:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 let start=counts();let mut db=seeded(n).with_quotas(lin::Quotas{max_output_bytes:1024*1024*1024,..Default::default()});let seeded=counts();
 let result=db.run(&format!("docs | take {n}")).unwrap();let held=counts();
 assert_eq!(result.rows.len(),n);
 for (i,row) in result.rows.iter().enumerate(){assert_eq!(row["id"].text(),Some(format!("d-{i}").as_str()));assert_eq!(row,db.store.get_by_id("docs",&format!("d-{i}")).unwrap());}
 drop(result);let released=counts();drop(db);let closed=counts();
 println!("{}",serde_json::json!({"rows":n,"seed_live_bytes":seeded.0-start.0,"seed_allocations":seeded.1-start.1,"seed_requested_bytes":seeded.2-start.2,"query_live_bytes":held.0-seeded.0,"query_allocations":held.1-seeded.1,"query_requested_bytes":held.2-seeded.2,"after_result_drop_live_bytes":released.0-seeded.0,"after_db_drop_live_bytes":closed.0-start.0,"exact_all_rows_readback":true}));
}
