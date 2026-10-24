use lin::Db;
use std::{hint::black_box,time::Instant};
#[path="../src/crc.rs"] mod crc;
fn source(start:usize,n:usize)->String {
 format!("insert docs [{}]",(start..start+n).map(|i|{
  let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
  let wing=if i%2==0 {"rag"} else {"sys"};
  format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000} else {30*86400000})
 }).collect::<Vec<_>>().join(","))
}

fn main(){
 let args:Vec<String>=std::env::args().collect();
 if args[1]=="emit" {
  let n:usize=args[3].parse().unwrap();let mut db=Db::open(&args[2]).unwrap();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source(0,n)).unwrap().done.n,n);
  println!("ACK {}",n);std::process::exit(0);
 }
 let data=std::fs::read(&args[2]).unwrap();let process:usize=args[3].parse().unwrap();
 for accel in [1,4,8] {
  let compressed=lz4::block::compress(&data,Some(lz4::block::CompressionMode::FAST(accel)),true).unwrap();
  assert_eq!(lz4::block::decompress(&compressed,None).unwrap(),data);
  let mut raw_ns=Vec::new();let mut comp_ns=Vec::new();
  for round in 0..24 {for variant in if (process+round)%2==0 {[0,1]}else{[1,0]} {
   let t=Instant::now();for _ in 0..16 {
    if variant==0 {let mut h=crc::Hasher::new();h.update(black_box(&data));black_box(h.finalize());}
    else {let c=lz4::block::compress(black_box(&data),Some(lz4::block::CompressionMode::FAST(accel)),true).unwrap();let mut h=crc::Hasher::new();h.update(&c);black_box(h.finalize());black_box(c);}
   }
   let ns=t.elapsed().as_nanos() as f64/16.0;if variant==0 {raw_ns.push(ns)} else {comp_ns.push(ns)}
  }}
  println!("{}",serde_json::json!({"acceleration":accel,"input_bytes":data.len(),"compressed_bytes":compressed.len(),"raw_checksum_ns":raw_ns,"compress_checksum_ns":comp_ns,"exact_roundtrip":true}));
 }
}
