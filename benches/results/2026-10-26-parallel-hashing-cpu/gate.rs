use lin::{Embedder,HashingEmbedder};
use std::{sync::Arc,time::Instant};
fn candidate(e:&HashingEmbedder,texts:&[&str],force_fallback:bool)->Vec<Arc<[f32]>> {
 let n=texts.len();
 if !(512..=16384).contains(&n) || !(512*1024..=16*1024*1024).contains(&n.saturating_mul(e.dim())) || texts.first().map_or(0,|s|s.len()).saturating_mul(n)<8*1024 {return e.embed_batch(texts)}
 std::thread::scope(|scope|{
  let (left,right)=texts.split_at(n/2);
  let thread=if force_fallback {None} else {std::thread::Builder::new().spawn_scoped(scope,||e.embed_batch(right)).ok()};
  let mut out=e.embed_batch(left);
  match thread {Some(t)=>out.extend(t.join().unwrap_or_else(|p|std::panic::resume_unwind(p))),None=>out.extend(e.embed_batch(right))}
  out
 })
}
fn exact(a:&[Arc<[f32]>],b:&[Arc<[f32]>])->bool {a.len()==b.len()&&a.iter().zip(b).all(|(a,b)|a.len()==b.len()&&a.iter().zip(b.iter()).all(|(x,y)|x.to_bits()==y.to_bits()))}
fn main(){
 for (case,n,dim,kind) in [("default_1k",1000,768,0),("default_10k",10000,768,0),("boundary_682",682,768,0),("boundary_683",683,768,0),("boundary_512_dim1024",512,1024,0),("guard_dim64",1000,64,0),("guard_16500",16500,768,0),("short_repeated",1000,768,1),("long_unicode",1000,768,2),("skewed_lengths",1000,768,3)] {
  let e=HashingEmbedder::new(format!("test/{dim}"),dim);
  let texts=(0..n).map(|i| match kind {1=>"a".to_owned(),2=>format!("документ {i} Rust WAL {}", "ёжλΑβ Data ".repeat(64)),3=>if i==0 {"wide ".repeat(128)}else{String::new()},_=>format!("doc {i} {} body {i}",if i%10==0 {"wal note"} else {"plain"})}).collect::<Vec<_>>();
  let refs=texts.iter().map(String::as_str).collect::<Vec<_>>();let expected=e.embed_batch(&refs);assert!(exact(&candidate(&e,&refs,true),&expected));
  let mut ns=[vec![],vec![],vec![]];let mut orders=vec![];
  for round in 0..24 {
   let order=match round%6 {0=>[0,1,2],1=>[2,1,0],2=>[1,0,2],3=>[2,0,1],4=>[0,2,1],_=>[1,2,0]};orders.push(order);
   for mode in order {let t=Instant::now();let out=if mode==2{candidate(&e,&refs,false)}else{e.embed_batch(&refs)};let elapsed=t.elapsed().as_nanos() as u64;assert!(exact(&out,&expected));ns[mode].push(elapsed);drop(out);}
  }
  println!("{}",serde_json::json!({"case":case,"rows":n,"dim":dim,"ns":ns,"order":orders,"exact_bits":true,"spawn_failure_fallback":true,"output_drop_timed":false,"cold_thread_join_and_tls_drop_timed":true}));
 }
}
