use lin::{Cell,Db};
use std::{sync::Arc,time::Instant,hint::black_box,io};
type Error=String;fn io_err(s:&str)->Error{s.to_owned()}
const SPARSE_VECTOR_THRESHOLD:usize=64*1024;
mod reference {use super::*;include!("dictionary_choice_reference.rs");pub fn sparse(v:&[Option<Arc<[f32]>>])->bool{use_sparse_vectors(v)}pub fn append(v:&[Option<Arc<[f32]>>],out:&mut Vec<u8>)->Result<(),Error>{append_sparse_rows_serial(v,out)}pub fn parallel(v:&[Option<Arc<[f32]>>],out:&mut Vec<u8>)->Result<(),Error>{append_sparse_rows_parallel(v,out)}
 pub fn mask(v:&[f32])->u32{
  #[cfg(target_arch="aarch64")] if v.len()==8 && std::arch::is_aarch64_feature_detected!("neon"){return unsafe{presence_mask8(v)}}
  v.iter().enumerate().fold(0,|m,(i,x)|m|((x.to_bits()!=0)as u32)<<i)
 }
}
fn encode_sparse_reference(v:&[Option<Arc<[f32]>>],out:&mut Vec<u8>)->Result<(),Error>{reference::append(v,out)}
fn use_sparse_vectors(v:&[Option<Arc<[f32]>>])->bool{reference::sparse(v)}
fn append_sparse_rows_parallel(v:&[Option<Arc<[f32]>>],out:&mut Vec<u8>)->Result<(),Error>{reference::parallel(v,out)}
include!("dictionary_choice_candidate.rs");

fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 let special=vec![None,Some(Arc::from([])),Some(Arc::from([0.0f32;9])),Some(Arc::from([-0.0,f32::from_bits(0x7fc00001),f32::from_bits(0x7fa00001),f32::INFINITY,f32::NEG_INFINITY,f32::from_bits(1)]))];check(&special);
 for dim in [65535,65536,65537] {let mut v=vec![0f32;dim];v[0]=-0.0;v[dim-1]=f32::from_bits(0x7fa00001);check(&vec![Some(v.into())]);}
 let high=||{let mut v=vec![0.0f32;768];for(i,x)in v[..64].iter_mut().enumerate(){*x=f32::from_bits(0x3f000000+i as u32+1);}Some(Arc::from(v))};
 let default=fixture(1000);let mut probe_miss=vec![high();1000];for i in [0,333,666,999]{probe_miss[i]=default[i].clone();}let mut late=default.clone();late[999]=high();
 let mut front=vec![high();1000];front[..4].clone_from_slice(&default[..4]);
 let cases=vec![("default_1k",default),("default_10k",fixture(10000)),("high_cardinality",vec![high();1000]),("dense",vec![Some(Arc::from(vec![1f32;768]));1000]),("zero",vec![Some(Arc::from(vec![0f32;768]));1000]),("front_only_dictionary",front),("probe_miss_majority_high",probe_miss),("late_high_cardinality",late),("special",special)];
 for(case,rows)in cases{let(oldbytes,newbytes,stats)=check(&rows);let reserve=rows.iter().flatten().map(|v|v.len()*8+80).sum::<usize>();let mut buffers=[Vec::with_capacity(reserve),Vec::with_capacity(reserve),Vec::with_capacity(reserve)];let mut ns=[vec![],vec![],vec![]];
  for round in 0..24{for pos in 0..3{let mode=(process+round+pos)%3;let out=&mut buffers[mode];out.clear();let t=Instant::now();let code=if mode==2{encode_proposed_choice(black_box(&rows),out).unwrap().0}else{encode_current_choice(black_box(&rows),out).unwrap()};ns[mode].push(t.elapsed().as_nanos()as u64);assert_eq!(decode_choice(out,rows.len(),code),expected(&rows));}}
  println!("{}",serde_json::json!({"case":case,"rows":rows.len(),"ns":ns,"reference_bytes":oldbytes,"candidate_bytes":newbytes,"dictionary_selected":stats.selected,"exact_bits":true,"output_allocation_and_drop_timed":false}));
 }
}