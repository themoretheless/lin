use lin::{Embedder,HashingEmbedder};
use std::{sync::{Arc,OnceLock,mpsc},time::Instant,hint::black_box};
struct Job { embedder:HashingEmbedder,text:String,spans:Vec<(usize,usize)>,reply:mpsc::SyncSender<Vec<Arc<[f32]>>> }
fn worker()->Option<&'static mpsc::SyncSender<Job>> {
 static WORKER:OnceLock<Option<mpsc::SyncSender<Job>>>=OnceLock::new();
 WORKER.get_or_init(||{
  let(tx,rx)=mpsc::sync_channel::<Job>(1);
  std::thread::Builder::new().name("lin-prototype-embed".into()).spawn(move||while let Ok(job)=rx.recv(){
   let refs:Vec<&str>=job.spans.iter().map(|&(a,b)|&job.text[a..b]).collect();
   let result=job.embedder.embed_batch(&refs);let _=job.reply.send(result);
  }).ok().map(|_|tx)
 }).as_ref()
}
fn proposed(emb:&HashingEmbedder,texts:&[&str])->Vec<Arc<[f32]>> {
 if texts.len()<512 || emb.dim()<128 {return emb.embed_batch(texts)}
 let Some(tx)=worker() else{return emb.embed_batch(texts)};
 let(left,right)=texts.split_at(texts.len()/2);let(reply,rx)=mpsc::sync_channel(1);
 let mut text=String::with_capacity(right.iter().map(|s|s.len()).sum());let mut spans=Vec::with_capacity(right.len());
 for value in right {let start=text.len();text.push_str(value);spans.push((start,text.len()));}
 let job=Job{embedder:emb.clone(),text,spans,reply};
 if tx.try_send(job).is_err(){return emb.embed_batch(texts)}
 let mut out=emb.embed_batch(left);out.extend(rx.recv().unwrap_or_else(|_|emb.embed_batch(right)));out
}
fn exact(a:&[Arc<[f32]>],b:&[Arc<[f32]>])->bool {a.len()==b.len() && a.iter().zip(b).all(|(a,b)|a.len()==b.len() && a.iter().zip(b.iter()).all(|(a,b)|a.to_bits()==b.to_bits()))}
fn fixture(n:usize)->Vec<String>{(0..n).map(|i|if i%10==0 {format!("doc {i} wal note body {i}")}else{format!("doc {i} plain body {i}")}).collect()}
fn main(){
 let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 let emb=HashingEmbedder::new("test",768);let cold=fixture(1000);let refs:Vec<&str>=cold.iter().map(String::as_str).collect();let t=Instant::now();let first=proposed(&emb,&refs);let cold_ns=t.elapsed().as_nanos();assert!(exact(&first,&emb.embed_batch(&refs)));drop(first);
 for dim in [8,9,128,768,1536] {for n in [0,1,511,512,513,1000] {
  let emb=HashingEmbedder::new("test",dim);let owned=vec!["ЁЖ ΟΣ İ wal\u{b}note a\u{a0}b".to_string();n];let refs:Vec<&str>=owned.iter().map(String::as_str).collect();assert!(exact(&proposed(&emb,&refs),&emb.embed_batch(&refs)));
 }}
 for(name,texts,dim,loops)in[("default_1k",fixture(1000),768,16),("default_10k",fixture(10000),768,2),("unicode_1k",vec!["ЁЖ ΟΣ İ wal\u{b}note a\u{a0}b ".repeat(8);1000],768,4),("small_dim_fallback",fixture(1000),8,16)] {
  let emb=HashingEmbedder::new("test",dim);let refs:Vec<&str>=texts.iter().map(String::as_str).collect();assert!(exact(&proposed(&emb,&refs),&emb.embed_batch(&refs)));let mut ns=vec![Vec::<f64>::new();2];
  for round in 0..12 {for v in if (round+process)%2==0 {[0,1]}else{[1,0]} {
   let t=Instant::now();for _ in 0..loops {let out=if v==0 {emb.embed_batch(black_box(&refs))}else{proposed(&emb,black_box(&refs))};black_box(out);}
   ns[v].push(t.elapsed().as_nanos() as f64/loops as f64);
  }}
  println!("{}",serde_json::json!({"case":name,"rows":refs.len(),"dim":dim,"ns":ns,"exact_values":true,"first_worker_call_ns":cold_ns,"includes_transfer_allocation_and_drop":true}));
 }
}
