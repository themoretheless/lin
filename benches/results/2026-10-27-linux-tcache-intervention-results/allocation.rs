pub mod allocation_probe {
 use std::alloc::{GlobalAlloc,Layout,System};use std::sync::atomic::{AtomicBool,AtomicU64,AtomicUsize,Ordering::{Relaxed,Acquire,Release}};
 const CAP:usize=64;static ENABLED:AtomicBool=AtomicBool::new(false);static ACTIVE:AtomicBool=AtomicBool::new(false);static OWNER:AtomicUsize=AtomicUsize::new(0);static NEXT:AtomicUsize=AtomicUsize::new(0);static FIELDS:AtomicUsize=AtomicUsize::new(0);static OVERFLOW:AtomicBool=AtomicBool::new(false);
 static EVENTS:[[AtomicU64;6];CAP]=[const{[const{AtomicU64::new(0)};6]};CAP];
 thread_local!{static TOKEN:u8=const{0};}
 fn token()->usize{TOKEN.with(|v|v as *const u8 as usize)}
 #[inline]fn watched()->bool{ACTIVE.load(Acquire)&&OWNER.load(Relaxed)==token()}
 fn record(op:u64,l:Layout,new_size:usize,ns:u64,failed:bool){let index=NEXT.fetch_add(1,Relaxed);if index>=CAP{OVERFLOW.store(true,Relaxed);return;}for(slot,value)in EVENTS[index].iter().zip([op,l.size()as u64,l.align()as u64,new_size as u64,ns,u64::from(failed)]){slot.store(value,Relaxed);}}
 pub struct TracingSystem;
 unsafe impl GlobalAlloc for TracingSystem {
  unsafe fn alloc(&self,l:Layout)->*mut u8{let time=watched().then(std::time::Instant::now);let p=unsafe{System.alloc(l)};if let Some(t)=time{record(1,l,0,t.elapsed().as_nanos()as u64,p.is_null());}p}
  unsafe fn alloc_zeroed(&self,l:Layout)->*mut u8{let time=watched().then(std::time::Instant::now);let p=unsafe{System.alloc_zeroed(l)};if let Some(t)=time{record(2,l,0,t.elapsed().as_nanos()as u64,p.is_null());}p}
  unsafe fn realloc(&self,p:*mut u8,l:Layout,n:usize)->*mut u8{let time=watched().then(std::time::Instant::now);let out=unsafe{System.realloc(p,l,n)};if let Some(t)=time{record(3,l,n,t.elapsed().as_nanos()as u64,out.is_null());}out}
  unsafe fn dealloc(&self,p:*mut u8,l:Layout){let time=watched().then(std::time::Instant::now);unsafe{System.dealloc(p,l)};if let Some(t)=time{record(4,l,0,t.elapsed().as_nanos()as u64,false);}}
 }
 pub fn configure(enabled:bool){ACTIVE.store(false,Release);NEXT.store(0,Relaxed);FIELDS.store(0,Relaxed);OVERFLOW.store(false,Relaxed);ENABLED.store(enabled,Relaxed);}
 #[inline(never)]pub fn clone_row(row:&crate::store::Row)->crate::store::Row {
  NEXT.store(0,Relaxed);OVERFLOW.store(false,Relaxed);FIELDS.store(row.len(),Relaxed);
  if !ENABLED.load(Relaxed){return row.clone();}
  OWNER.store(token(),Relaxed);ACTIVE.store(true,Release);let result=row.clone();ACTIVE.store(false,Release);result
 }
 pub fn snapshot()->serde_json::Value {
  assert!(!ACTIVE.load(Acquire));let count=NEXT.load(Relaxed);let events=(0..count.min(CAP)).map(|i|EVENTS[i].iter().map(|x|x.load(Relaxed)).collect::<Vec<_>>()).collect::<Vec<_>>();
  serde_json::json!({"pid":std::process::id(),"enabled":ENABLED.load(Relaxed),"fields":FIELDS.load(Relaxed),"count":count,"overflow":OVERFLOW.load(Relaxed),"events":events,"columns":["op","size","align","new_size","service_ns","failed"]})
 }
 pub fn preflight(){
  use crate::store::{Row,Cell};let _=std::time::Instant::now().elapsed();
  for n in [0,1,8,12,33]{let row=(0..n).map(|i|(format!("field-{i}"),Cell::Text(std::sync::Arc::from("payload")))).collect::<Row>();configure(true);let copy=clone_row(&row);assert_eq!(copy,row);let trace=snapshot();assert_eq!(trace["overflow"],false);let count=trace["count"].as_u64().unwrap();if n==0{assert_eq!(count,0);}else{assert!(count>n as u64);assert!(trace["events"].as_array().unwrap().iter().all(|e|e[5]==0));}}
  configure(false);
 }
}
