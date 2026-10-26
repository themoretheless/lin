use lin::{Embedder,HashingEmbedder};
use std::{sync::Arc,time::Instant};
type Reply=Result<Vec<Arc<[f32]>>,Box<dyn std::any::Any+Send>>;
struct Job {dim:usize,texts:Vec<String>,reply:std::sync::mpsc::SyncSender<Reply>}
struct Worker {sender:Option<std::sync::mpsc::SyncSender<Job>>,handle:Option<std::thread::JoinHandle<()>>}
impl Worker {
 fn start()->std::io::Result<Self>{let(tx,rx)=std::sync::mpsc::sync_channel::<Job>(1);let handle=std::thread::Builder::new().name("lin-hash-cpu".into()).spawn(move||{
  while let Ok(job)=rx.recv(){let Job{dim,texts,reply}=job;
   let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{let e=HashingEmbedder::new("worker",dim);let refs=texts.iter().map(String::as_str).collect::<Vec<_>>();e.embed_batch(&refs)}));
   drop(texts);let _=reply.send(result);
  }
 })?;Ok(Self{sender:Some(tx),handle:Some(handle)})}
}
impl Drop for Worker {fn drop(&mut self){drop(self.sender.take());if let Some(h)=self.handle.take(){h.join().unwrap();}}}
fn candidate(e:&HashingEmbedder,texts:&[&str],worker:&mut Option<Worker>)->Vec<Arc<[f32]>> {
 let n=texts.len();
 if !(512..=16384).contains(&n) || !(512*1024..=16*1024*1024).contains(&n.saturating_mul(e.dim())) || texts.first().map_or(0,|s|s.len()).saturating_mul(n)<8*1024{return e.embed_batch(texts)}
 let(left,right)=texts.split_at(n/2);
 let Some(budget)=right.iter().try_fold(right.len().saturating_mul(std::mem::size_of::<String>()),|n,s|n.checked_add(s.len())) else{return e.embed_batch(texts)};
 if budget>1024*1024{return e.embed_batch(texts)}
 if worker.is_none(){*worker=Worker::start().ok();}
 let Some(worker)=worker.as_ref()else{return e.embed_batch(texts)};
 let(tx,rx)=std::sync::mpsc::sync_channel(1);let job=Job{dim:e.dim(),texts:right.iter().map(|s|(*s).to_owned()).collect(),reply:tx};
 if worker.sender.as_ref().unwrap().try_send(job).is_err(){return e.embed_batch(texts)}
 let mut out=e.embed_batch(left);
 match rx.recv(){Ok(Ok(v))=>out.extend(v),Ok(Err(p))=>std::panic::resume_unwind(p),Err(_)=>out.extend(e.embed_batch(right))}
 out
}
fn failure_oracle(e:&HashingEmbedder,texts:&[&str],expected:&[Arc<[f32]>]){
 let(tx,rx)=std::sync::mpsc::sync_channel::<Job>(1);drop(rx);let mut worker=Some(Worker{sender:Some(tx),handle:None});assert!(exact(&candidate(e,texts,&mut worker),expected));
 let(tx,rx)=std::sync::mpsc::sync_channel::<Job>(1);let(reply,_)=std::sync::mpsc::sync_channel(1);tx.try_send(Job{dim:8,texts:vec![],reply}).unwrap_or_else(|_|panic!("seed queue"));
 let mut worker=Some(Worker{sender:Some(tx),handle:None});assert!(exact(&candidate(e,texts,&mut worker),expected));drop(worker);drop(rx);
}
fn exact(a:&[Arc<[f32]>],b:&[Arc<[f32]>])->bool {a.len()==b.len()&&a.iter().zip(b).all(|(a,b)|a.len()==b.len()&&a.iter().zip(b.iter()).all(|(x,y)|x.to_bits()==y.to_bits()))}
fn main(){
 for (case,n,dim,kind) in [("default_1k",1000,768,0),("default_10k",10000,768,0),("boundary_682",682,768,0),("boundary_683",683,768,0),("boundary_512_dim1024",512,1024,0),("guard_dim64",1000,64,0),("guard_16500",16500,768,0),("short_repeated",1000,768,1),("long_unicode",1000,768,2),("skewed_lengths",1000,768,3),("input_budget_guard",1000,768,4)] {
  let e=HashingEmbedder::new(format!("test/{dim}"),dim);
  let texts=(0..n).map(|i| match kind {4=>"a".repeat(2100),1=>"a".to_owned(),2=>format!("документ {i} Rust WAL {}", "ёжλΑβ Data ".repeat(64)),3=>if i==0 {"wide ".repeat(128)}else{String::new()},_=>format!("doc {i} {} body {i}",if i%10==0 {"wal note"} else {"plain"})}).collect::<Vec<_>>();
  let refs=texts.iter().map(String::as_str).collect::<Vec<_>>();let expected=e.embed_batch(&refs);failure_oracle(&e,&refs,&expected);
  let mut worker=None;let timer=Instant::now();let cold=candidate(&e,&refs,&mut worker);let cold_ns=timer.elapsed().as_nanos() as u64;assert!(exact(&cold,&expected));drop(cold);let worker_created=worker.is_some();let mut ns=[vec![],vec![],vec![]];let mut orders=vec![];
  for round in 0..24 {
   let order=match round%6 {0=>[0,1,2],1=>[2,1,0],2=>[1,0,2],3=>[2,0,1],4=>[0,2,1],_=>[1,2,0]};orders.push(order);
   for mode in order {let t=Instant::now();let out=if mode==2{candidate(&e,&refs,&mut worker)}else{e.embed_batch(&refs)};let elapsed=t.elapsed().as_nanos() as u64;assert!(exact(&out,&expected));ns[mode].push(elapsed);drop(out);}
  }
  println!("{}",serde_json::json!({"case":case,"rows":n,"dim":dim,"ns":ns,"order":orders,"exact_bits":true,"full_and_disconnected_fallback":true,"cold_first_call_ns":cold_ns,"worker_created":worker_created,"max_copied_job_bytes":1024*1024,"output_drop_timed":false,"worker_drop_outside_timing":true,"logical_parallelism":std::thread::available_parallelism().map(|n|n.get()).unwrap_or(1)}));
 }
}
