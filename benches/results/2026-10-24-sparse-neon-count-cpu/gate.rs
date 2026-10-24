use lin::{Cell,Db};
use std::{sync::Arc,hint::black_box,time::Instant};
fn sparse_bytes_serial(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
    rows.iter()
        .map(|row| {
            row.as_ref().map_or(4, |v| {
                8usize.saturating_add(
                    v.iter()
                        .filter(|value| value.to_bits() != 0)
                        .count()
                        .saturating_mul(8),
                )
            })
        })
        .fold(0usize, usize::saturating_add)
}

fn neon_count(v:&[f32])->usize {
 use std::arch::aarch64::*;
 let mut total=0usize;
 let mut chunks=v.chunks_exact(16);
 for c in &mut chunks {
  // Each load stays within this sixteen-element chunk. Compare raw bits,
  // preserving -0 and every NaN payload as nonzero, exactly as the baseline.
  unsafe {
   let p=c.as_ptr().cast::<u32>();let z=vdupq_n_u32(0);
   let a=vshrq_n_u32::<31>(vmvnq_u32(vceqq_u32(vld1q_u32(p),z)));
   let b=vshrq_n_u32::<31>(vmvnq_u32(vceqq_u32(vld1q_u32(p.add(4)),z)));
   let d=vshrq_n_u32::<31>(vmvnq_u32(vceqq_u32(vld1q_u32(p.add(8)),z)));
   let e=vshrq_n_u32::<31>(vmvnq_u32(vceqq_u32(vld1q_u32(p.add(12)),z)));
   total+=vaddvq_u32(vaddq_u32(vaddq_u32(a,b),vaddq_u32(d,e))) as usize;
  }
 }
 total+chunks.remainder().iter().filter(|x|x.to_bits()!=0).count()
}
fn candidate(rows:&[Option<Arc<[f32]>>])->usize {
 rows.iter().map(|r|r.as_ref().map_or(4,|v|8usize.saturating_add(neon_count(v).saturating_mul(8)))).fold(0usize,usize::saturating_add)
}
fn fixture(n:usize) -> Vec<Option<Arc<[f32]>>> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").iter().map(|r|{let Cell::Vec(v)=&r["embedding"] else {panic!("embedding")};assert_eq!(v.len(),768);Some(v.clone())}).collect()
}
fn main(){
 let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 // Exercise every tail/alignment, raw-bit payloads, None and Some(empty).
 let mut seed=17u32;
 for n in 0..1025 {let raw:Vec<f32>=(0..n+4).map(|_|{seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;f32::from_bits(if seed%3==0 {0}else{seed})}).collect();
  for start in 0..4 {let v=&raw[start..start+n];assert_eq!(neon_count(v),v.iter().filter(|x|x.to_bits()!=0).count());}}
 let special=vec![None,Some(Arc::from([])),Some(Arc::from([0.0;35])),Some(Arc::from([-0.0,f32::from_bits(0x7fc00001),f32::from_bits(0x7fa00002),f32::INFINITY,f32::NEG_INFINITY,-1.25]))];
 for (name,rows,loops) in [("default_1k",fixture(1000),64),("default_10k",fixture(10000),8),("dense",(0..1000).map(|_|Some(vec![1.25;768].into())).collect(),64),("boundaries",special,1024)] {
  assert_eq!(sparse_bytes_serial(&rows),candidate(&rows));let funcs:[fn(&[Option<Arc<[f32]>>])->usize;2]=[sparse_bytes_serial,candidate];let mut times=vec![Vec::<f64>::new();2];
  for round in 0..24 {for pos in 0..2 {let v=(pos+round+process)%2;let t=Instant::now();for _ in 0..loops {black_box(funcs[v](black_box(&rows)));}times[v].push(t.elapsed().as_nanos() as f64/loops as f64);}}
  println!("{}",serde_json::json!({"case":name,"ns":times,"exact_count":true,"loops":loops}));
 }
}
