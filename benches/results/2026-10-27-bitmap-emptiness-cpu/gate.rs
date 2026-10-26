use lin::Db;
use std::{hint::black_box,time::Instant};
fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 for n in [1000usize,10000,100000] {
  let mut db=Db::empty();db.run("col bits { flag: bool }").unwrap();db.run("index bits [flag]").unwrap();
  for start in (0..n).step_by(500){let text=format!("insert bits [{}]",vec!["{flag:true}";(start+500).min(n)-start].join(","));db.run(&text).unwrap();}
  let index=db.store.indexes.values().find(|i|i.def.collection=="bits").unwrap();let base=index.forward.values().next().unwrap();assert_eq!(base.count(),n);assert!(base.is_bitmap());
  for shape in ["dense","late_only","empty"] {
   let mut posting=base.clone();if shape!="dense"{for i in 0..n-usize::from(shape=="late_only"){posting.remove(i);}}
   let expected=shape=="empty";assert_eq!(posting.count()==0,expected);assert_eq!(posting.is_empty(),expected);assert_eq!(posting.to_indices().len(),if expected{0}else if shape=="dense"{n}else{1});
   let mut ns=[Vec::new(),Vec::new(),Vec::new()];let iterations=512;
   for round in 0..24{for pos in 0..3{let mode=(process+round+pos)%3;let start=Instant::now();for _ in 0..iterations{let p=black_box(&posting);let value=if mode==2{p.is_empty()}else{p.count()==0};black_box(value);}let elapsed=start.elapsed().as_nanos()as f64/iterations as f64;assert!(elapsed>0.);ns[mode].push(elapsed);}}
   println!("{}",serde_json::json!({"rows":n,"shape":shape,"ns":ns,"operations_per_sample":iterations,"exact_oracle":true,"allocation_and_drop_timed":false}));
  }
 }
}
