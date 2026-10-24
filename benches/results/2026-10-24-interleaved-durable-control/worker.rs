use lin::Db;
use std::io::{self,BufRead,Write};
use std::time::Instant;
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
struct OwnedDir(std::path::PathBuf);
impl OwnedDir {
 fn new()->io::Result<Self> {
  let nonce=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
  let path=std::env::temp_dir().join(format!("lin-interleaved-control-{}-{}",std::process::id(),nonce));
  std::fs::create_dir(&path)?;Ok(Self(path))
 }
 fn path(&self)->&std::path::Path {&self.0}
}
impl Drop for OwnedDir {fn drop(&mut self){std::fs::remove_dir_all(&self.0).expect("owned fixture cleanup");}}
fn main()->Result<(),Box<dyn std::error::Error>> {
 let n:usize=std::env::args().nth(1).ok_or("rows missing")?.parse()?;
 assert!(n==1000 || n==10000);
 let src=source(0,n);
 let query="docs | { id, uri, title, layer, wing, body, ts, hash, embedding } | take all";
 let mut reference=Db::empty();reference.run("index docs [wing, ts]")?;reference.run(&src)?;
 let expected=reference.run(query)?.rows;drop(reference);
 let mut out=io::stdout().lock();
 writeln!(out,"{}",serde_json::json!({"ready":true,"rows":n}))?;out.flush()?;
 for line in io::stdin().lock().lines() {
  assert_eq!(line?,"sample");
  let dir=OwnedDir::new()?;
  let mut db=Db::open(dir.path())?;db.run("index docs [wing, ts]")?;
  let prepared=db.prepare(&src)?;
  let start=Instant::now();let handle=prepared.run(&mut db)?;let ns=start.elapsed().as_nanos();
  assert_eq!(handle.done.n,n);drop(handle);
  let actual=db.run(query)?.rows;assert!(exact(&actual,&expected));
  drop(actual);drop(prepared);drop(db);drop(dir);
  writeln!(out,"{}",serde_json::json!({"ns":ns,"validated_rows":n,"exact_bits":true}))?;out.flush()?;
 }
 Ok(())
}
