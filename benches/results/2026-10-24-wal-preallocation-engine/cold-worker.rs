use lin::Db;
use std::time::Instant;
use std::os::unix::fs::MetadataExt;
fn source(start:usize,n:usize)->String {
 format!("insert docs [{}]",(start..start+n).map(|i|{
  let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
  let wing=if i%2==0 {"rag"} else {"sys"};
  format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000} else {30*86400000})
 }).collect::<Vec<_>>().join(","))
}
fn exact(a:&[lin::Row],b:&[lin::Row])->bool {
 a.len()==b.len() && a.iter().zip(b).all(|(a,b)|a.len()==b.len() && a.iter().all(|(k,v)|match(v,b.get(k)) {
  (lin::Cell::Vec(x),Some(lin::Cell::Vec(y)))=>x.len()==y.len() && x.iter().zip(y.iter()).all(|(x,y)|x.to_bits()==y.to_bits()),
  (_,Some(x))=>v==x,_=>false,
 }))
}

fn main(){
 let path=std::path::PathBuf::from(std::env::args().nth(1).unwrap());assert!(!path.exists());let src=source(0,1000);
 let total=Instant::now();let mut db=Db::open(&path).unwrap();let open_ns=total.elapsed().as_nanos();let open_meta=std::fs::metadata(path.join("log")).unwrap();
 db.run("index docs [wing, ts]").unwrap();let prepared=db.prepare(&src).unwrap();let start=Instant::now();let handle=prepared.run(&mut db).unwrap();let commit_ns=start.elapsed().as_nanos();let open_to_commit_ns=total.elapsed().as_nanos();assert_eq!(handle.done.n,1000);drop(handle);drop(prepared);
 let meta=std::fs::metadata(path.join("log")).unwrap();let query="docs | { id, uri, title, layer, wing, body, ts, hash, embedding } | take all";let actual=db.run(query).unwrap().rows;
 let mut reference=Db::empty();reference.run(&src).unwrap();let expected=reference.run(query).unwrap().rows;assert!(exact(&actual,&expected));drop(reference);drop(actual);drop(expected);
 let start=Instant::now();drop(db);let drop_ns=start.elapsed().as_nanos();std::fs::remove_dir_all(path).unwrap();
 println!("{}",serde_json::json!({"open_ns":open_ns,"commit_ns":commit_ns,"open_to_commit_ns":open_to_commit_ns,"drop_checkpoint_ns":drop_ns,"open_log_bytes":open_meta.len(),"open_allocated_bytes":open_meta.blocks()*512,"commit_log_bytes":meta.len(),"commit_allocated_bytes":meta.blocks()*512,"exact_bits":true}));
}
