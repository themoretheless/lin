use lin::{Db,OpenOpts};
use std::io::Write;
fn source(start:usize,n:usize)->String {
 format!("insert docs [{}]",(start..start+n).map(|i|{
  let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
  let wing=if i%2==0 {"rag"} else {"sys"};
  format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000} else {30*86400000})
 }).collect::<Vec<_>>().join(","))
}
fn main() {
 let args=std::env::args().collect::<Vec<_>>();let mode=&args[1];let path=&args[2];let commits=args[3].parse::<usize>().unwrap();
 if mode=="create" {
  let mut db=Db::open_with(path,OpenOpts::default()).unwrap();db.run("index docs [wing, ts]").unwrap();
  for c in 0..commits {assert_eq!(db.run(&source(c*1000,1000)).unwrap().done.n,1000);println!("ACK {}",(c+1)*1000);std::io::stdout().flush().unwrap();}
  // Deliberate process exit: no Db::drop/checkpoint after acknowledged Full commits.
  std::process::exit(0);
 } else if mode=="verify" {
  let read=Db::open_read(path).unwrap();let query="docs | { id, uri, title, layer, wing, body, ts, hash, embedding } | take all";
  let actual=read.run(query).unwrap().rows;
  let mut memory=Db::empty();for c in 0..commits {memory.run(&source(c*1000,1000)).unwrap();}
  let expected=memory.run(query).unwrap().rows;
  let exact=actual.len()==expected.len() && actual.iter().zip(&expected).all(|(a,b)|{
   a.len()==b.len() && a.iter().all(|(k,v)|match (v,b.get(k)) {
    (lin::Cell::Vec(x),Some(lin::Cell::Vec(y)))=>x.len()==y.len() && x.iter().zip(y.iter()).all(|(a,b)|a.to_bits()==b.to_bits()),
    (_,Some(other))=>v==other,_=>false,
   })
  });
  assert!(exact,"exact row mismatch");
  println!("verified {}",actual.len());
 } else {panic!("unknown mode");}
}
