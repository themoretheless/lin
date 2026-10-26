use lin::{Cell,Db,Row};
use std::{sync::Arc,time::Instant,hint::black_box,io};
type Error=String;fn io_err(s:&str)->Error{s.to_owned()}
const SPARSE_VECTOR_THRESHOLD:usize=64*1024;
mod reference {use super::*;include!("neon_count_reference.rs");pub fn sparse(rows:&[Option<Arc<[f32]>>])->bool{use_sparse_vectors(rows)} pub fn append(rows:&[Option<Arc<[f32]>>],out:&mut Vec<u8>){if rows.len()>=4096 && rows.iter().flatten().map(|x|x.len()).sum::<usize>()>=2*1024*1024{append_sparse_rows_parallel(rows,out).unwrap()}else{append_sparse_rows_serial(rows,out).unwrap()}}}
mod candidate {use super::*;include!("neon_count_candidate.rs");pub fn sparse(rows:&[Option<Arc<[f32]>>])->bool{use_sparse_vectors(rows)} pub fn count(v:&[f32])->usize{count_nonzero_bits(v)}}
fn dense_write(rows:&[Option<Arc<[f32]>>],out:&mut Vec<u8>){for row in rows {let v=row.as_deref().unwrap_or(&[]);out.extend_from_slice(&(v.len() as u32).to_le_bytes());
 #[cfg(target_endian="little")] {let bytes=unsafe{std::slice::from_raw_parts(v.as_ptr() as *const u8,v.len()*std::mem::size_of::<f32>())};out.extend_from_slice(bytes);}
 #[cfg(not(target_endian="little"))] {for x in v{out.extend_from_slice(&x.to_le_bytes());}}
}}

fn encode(rows:&[Option<Arc<[f32]>>],mode:usize)->(bool,Vec<u8>){
 let sparse=if mode==2{candidate::sparse(rows)}else{reference::sparse(rows)};
 let mut out=Vec::with_capacity(rows.len()*(8+768*4)+64);
 if sparse {reference::append(rows,&mut out)}else{dense_write(rows,&mut out)}
 (sparse,out)
}
fn oracle(){
 let special=[0u32,0x80000000,0x7f800000,0xff800000,0x7fc01234,0xffc03456,1,0x80000001];let mut state=0x1234567812345678u64;
 for offset in 0..4 {for n in 0..=1024 {
  let values=(0..n+offset).map(|i|{state^=state<<13;state^=state>>7;state^=state<<17;f32::from_bits(if i%3==0{state as u32}else{special[i%special.len()]})}).collect::<Vec<_>>();
  let v=&values[offset..];assert_eq!(candidate::count(v),v.iter().filter(|x|x.to_bits()!=0).count());
  let rows=vec![None,Some(Arc::from([])),Some(Arc::from(v))];assert_eq!(encode(&rows,0),encode(&rows,2));
 }}
 for n in [(1<<20)-1,1<<20,(1<<20)+1,(2<<20)+31] {
  let v=(0..n).map(|i|f32::from_bits(special[i%special.len()])).collect::<Vec<_>>();assert_eq!(candidate::count(&v),v.iter().filter(|x|x.to_bits()!=0).count());
 }
 for n in [16382,16383,16384,16385,65535,65536,65537] {for bits in [0u32,0x80000000,0x3f800000] {let rows=vec![Some(Arc::from(vec![f32::from_bits(bits);n]))];assert_eq!(encode(&rows,0),encode(&rows,2));}}
}
fn fixture(n:usize) -> Vec<Row> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").to_vec()
}


fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();oracle();
 let actual=|rows:&[Row]| rows.iter().map(|r|match r.get("embedding"){Some(Cell::Vec(v))=>Some(v.clone()),_=>None}).collect::<Vec<_>>();
 let half=|n:usize|{let mut v=vec![0f32;768];for i in 0..n{v[i*768/n]=1.0;}vec![Some(Arc::from(v));1000]};
 let cases=vec![("default_1k",actual(&fixture(1000))),("default_10k",actual(&fixture(10000))),("small_127",actual(&fixture(127))),("zero_1k",vec![Some(Arc::from(vec![0f32;768]));1000]),("half_383",half(383)),("half_384",half(384)),("dense_1k",vec![Some(Arc::from(vec![1f32;768]));1000]),("negative_zero_1k",vec![Some(Arc::from(vec![-0f32;768]));1000])];
 for(case,rows)in cases {let expected=encode(&rows,0);let mut ns=vec![Vec::<u128>::new();3];
 for round in 0..24 {for pos in 0..3{let mode=(process+round+pos)%3;let timer=Instant::now();let(sparse,bytes)=encode(black_box(&rows),mode);ns[mode].push(timer.elapsed().as_nanos());assert_eq!(sparse,expected.0);assert_eq!(bytes,expected.1);drop(bytes);}}
 println!("{}",serde_json::json!({"case":case,"rows":rows.len(),"ns":ns,"bytes":expected.1.len(),"sparse":expected.0,"exact_bytes_and_codec":true,"output_drop_timed":false}));
 }}