use lin::Db;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering::Relaxed};
struct Count;
static LIVE: AtomicIsize = AtomicIsize::new(0);
static ALLOC: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static FREE: AtomicUsize = AtomicUsize::new(0);
static FREE_BYTES: AtomicUsize = AtomicUsize::new(0);
static REALLOC: AtomicUsize = AtomicUsize::new(0);
static REALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static REALLOC_OLD_BYTES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            LIVE.fetch_add(l.size() as isize, Relaxed);
            ALLOC.fetch_add(1, Relaxed);
            ALLOC_BYTES.fetch_add(l.size(), Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            LIVE.fetch_add(l.size() as isize, Relaxed);
            ALLOC.fetch_add(1, Relaxed);
            ALLOC_BYTES.fetch_add(l.size(), Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size() as isize, Relaxed);
        FREE.fetch_add(1, Relaxed);
        FREE_BYTES.fetch_add(l.size(), Relaxed);
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, n) };
        if !q.is_null() {
            LIVE.fetch_add(n as isize - l.size() as isize, Relaxed);
            REALLOC.fetch_add(1, Relaxed);
            REALLOC_BYTES.fetch_add(n, Relaxed);
            REALLOC_OLD_BYTES.fetch_add(l.size(), Relaxed);
        }
        q
    }
}
#[global_allocator]
static GLOBAL: Count = Count;
#[derive(Clone, Copy, serde::Serialize)]
struct Counts {
    live: isize,
    alloc: usize,
    alloc_bytes: usize,
    free: usize,
    free_bytes: usize,
    realloc: usize,
    realloc_new_bytes: usize,
    realloc_old_bytes: usize,
}
fn counts() -> Counts {
    Counts {
        live: LIVE.load(Relaxed),
        alloc: ALLOC.load(Relaxed),
        alloc_bytes: ALLOC_BYTES.load(Relaxed),
        free: FREE.load(Relaxed),
        free_bytes: FREE_BYTES.load(Relaxed),
        realloc: REALLOC.load(Relaxed),
        realloc_new_bytes: REALLOC_BYTES.load(Relaxed),
        realloc_old_bytes: REALLOC_OLD_BYTES.load(Relaxed),
    }
}
fn seeded(n: usize) -> Db {
    let mut db = Db::empty();
    db.run("index docs [wing, ts]").unwrap();
    for start in (0..n).step_by(500) {
        let input=format!("insert docs [{}]",(start..(start+500).min(n)).map(|i|format!(r#"{{ id:"d-{i}",uri:"bench://{i}",title:"{}",layer:"wiki",wing:"{}",body:"body {i}",ts:timestamp({}) }}"#,if i%10==0 {format!("doc {i} wal note")}else{format!("doc {i} plain")},if i%2==0 {"rag"}else{"sys"},1700000000000i64+(i%2)as i64)).collect::<Vec<_>>().join(","));
        db.run(&input).unwrap();
    }
    assert_eq!(db.store.collection("docs").len(), n);
    db
}

fn main() {
    let n: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    let start = counts();
    let mut db = seeded(n).with_quotas(lin::Quotas {
        max_output_bytes: 1024 * 1024 * 1024,
        ..Default::default()
    });
    let seeded = counts();
    let result = db.run(&format!("docs | take {n}")).unwrap();
    let held = counts();
    assert_eq!(result.rows.len(), n);
    for (i, row) in result.rows.iter().enumerate() {
        assert_eq!(row["id"].text(), Some(format!("d-{i}").as_str()));
        assert_eq!(row["uri"].text(), Some(format!("bench://{i}").as_str()));
        assert_eq!(row["body"].text(), Some(format!("body {i}").as_str()));
        assert_eq!(row.len(), 9);
        assert_eq!(row, db.store.get_by_id("docs", &format!("d-{i}")).unwrap());
    }
    drop(result);
    let released = counts();
    drop(db);
    let closed = counts();
    println!(
        "{}",
        serde_json::json!({"rows":n,"row_descriptor_bytes":std::mem::size_of::<lin::Row>(),"output_quota_bytes":1024*1024*1024usize,"start":start,"seeded":seeded,"held":held,"released":released,"closed":closed,"exact_all_rows_readback":true,"counting_system_allocator":true,"latency_qualified":false,"rss_qualified":false})
    );
}
