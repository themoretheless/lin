//! Temporary diagnostic module. Never enabled in production.
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool,Ordering};
use std::time::Instant;
static ENABLED:AtomicBool=AtomicBool::new(false);
pub const NAMES:[&str;10]=["unused_parent","row_build","id_hash_fk","embedding","edges","indexes_reserve","fts","hnsw","row_maps","store_install"];
#[derive(Default,Clone,serde::Serialize)]
struct Frame { wall_ns:u64,phase_ns:[u64;10],phase_calls:[u64;10],payload_bytes:u64 }
struct State { active:bool,start:Option<Instant>,frame:Frame,frames:Vec<Frame> }
thread_local! { static STATE:RefCell<State>=RefCell::new(State{active:false,start:None,frame:Frame::default(),frames:Vec::with_capacity(128)}); }
pub fn configure() {
 ENABLED.store(std::env::var("LIN_BULK_TRACE").as_deref()==Ok("1"),Ordering::Relaxed);
 STATE.with(|_|{});
}
pub fn start() {
 if !ENABLED.load(Ordering::Relaxed) {return;}
 STATE.with(|s|{let mut s=s.borrow_mut();assert!(!s.active);s.frame=Frame::default();s.active=true;s.start=Some(Instant::now());});
}
pub fn finish() {
 if !ENABLED.load(Ordering::Relaxed) {return;}
 let now=Instant::now();STATE.with(|s|{let mut s=s.borrow_mut();assert!(s.active);s.active=false;s.frame.wall_ns=now.duration_since(s.start.take().unwrap()).as_nanos()as u64;let frame=s.frame.clone();s.frames.push(frame);});
}
pub struct Phase {index:usize,start:Option<Instant>}
pub fn phase(index:usize)->Phase {
 let active=ENABLED.load(Ordering::Relaxed)&&STATE.with(|s|s.borrow().active);
 Phase{index,start:active.then(Instant::now)}
}
impl Drop for Phase {
 fn drop(&mut self) {if let Some(start)=self.start {let elapsed=start.elapsed().as_nanos()as u64;STATE.with(|s|{let mut s=s.borrow_mut();assert!(s.active);s.frame.phase_ns[self.index]+=elapsed;s.frame.phase_calls[self.index]+=1;});}}
}
pub fn payload(n:usize) {if ENABLED.load(Ordering::Relaxed){STATE.with(|s|{let mut s=s.borrow_mut();if s.active{s.frame.payload_bytes+=n as u64;}});}}
pub fn flush() {
 if !ENABLED.load(Ordering::Relaxed) {return;}
 let path=std::env::var("LIN_BULK_TRACE_PATH").expect("trace path");
 STATE.with(|s|{let s=s.borrow();assert!(!s.active);let data=serde_json::json!({"phase_names":NAMES,"records":s.frames,"scope":"all benchmark-work invocations including fresh validation/calibration, not only measured observations","timing_qualified":false});std::fs::write(path,serde_json::to_vec_pretty(&data).unwrap()).unwrap();});
}
