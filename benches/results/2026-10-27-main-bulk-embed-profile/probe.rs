pub mod phase_probe {
 use std::sync::{Mutex,atomic::{AtomicBool,AtomicU64,Ordering::Relaxed}};
 pub static ACTIVE:AtomicBool=AtomicBool::new(false);
 static NS:[AtomicU64;25]=[const{AtomicU64::new(0)};25];
 static RECORDS:Mutex<Vec<serde_json::Value>>=Mutex::new(Vec::new());
 pub struct Span(usize,Option<std::time::Instant>);
 impl Span{pub fn new(i:usize)->Self{Self(i,ACTIVE.load(Relaxed).then(std::time::Instant::now))}}
 impl Drop for Span{fn drop(&mut self){if let Some(t)=self.1{NS[self.0].fetch_add(t.elapsed().as_nanos()as u64,Relaxed);}}}
 pub fn begin(){for n in &NS{n.store(0,Relaxed);}ACTIVE.store(true,Relaxed);}
 pub fn end(kind:&str,rows:usize,prepared_ns:u64,handle_drop_ns:u64){ACTIVE.store(false,Relaxed);let ns=NS.iter().map(|v|v.load(Relaxed)).collect::<Vec<_>>();RECORDS.lock().unwrap().push(serde_json::json!({"kind":kind,"rows":rows,"prepared_ns":prepared_ns,"handle_drop_ns":handle_drop_ns,"stages_ns":ns,"diagnostic_only":true}));}
 pub struct Dump;
 impl Drop for Dump{fn drop(&mut self){for r in RECORDS.lock().unwrap().iter(){println!("PHASE {r}");}}}
}
