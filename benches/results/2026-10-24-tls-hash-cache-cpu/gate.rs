use std::{sync::{Arc,Mutex},cell::RefCell,hint::black_box,time::Instant};
use rustc_hash::FxHashMap;
thread_local! {static BASELINE:Mutex<FxHashMap<String,Arc<str>>>=Mutex::new(FxHashMap::default());static CANDIDATE:RefCell<FxHashMap<String,Arc<str>>>=RefCell::new(FxHashMap::default());}
pub fn content_hash_arc(body: &str) -> Arc<str> {
    let h = fnv1a64(body.as_bytes());
    let mut buf = [0u8; 18];
    buf[0] = b'h';
    buf[1] = b':';
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for i in 0..16 {
        let shift = (15 - i) * 4;
        buf[2 + i] = HEX[((h >> shift) & 0xf) as usize];
    }
    // SAFETY: buf is always ASCII hex.
    Arc::from(std::str::from_utf8(&buf).unwrap())
}

pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
fn baseline(body: &str) -> Arc<str> {
    BASELINE.with(|cache| {
        let mut guard = cache.lock().unwrap();

        if let Some(cached) = guard.get(body) {
            return cached.clone();
        }

        let hash = content_hash_arc(body);
        guard.insert(body.to_owned(), hash.clone());

        while guard.len() >= 1024 {
            if let Some(key) = guard.keys().next().cloned() {
                guard.remove(&key);
            } else {
                break;
            }
        }

        hash
    })
}
fn candidate(body: &str) -> Arc<str> {
    CANDIDATE.with(|cache| {
        let mut guard = cache.borrow_mut();

        if let Some(cached) = guard.get(body) {
            return cached.clone();
        }

        let hash = content_hash_arc(body);
        guard.insert(body.to_owned(), hash.clone());

        while guard.len() >= 1024 {
            if let Some(key) = guard.keys().next().cloned() {
                guard.remove(&key);
            } else {
                break;
            }
        }

        hash
    })
}

fn main(){
 let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 for (name,bodies) in [
  ("warm_1k",(0..1000).map(|i|format!("body {i}")).collect::<Vec<_>>()),
  ("churn_2k",(0..2048).map(|i|format!("body {i}")).collect()),
  ("repeated_long",vec!["Ж".repeat(128);1000]),
  ("utf8_boundaries",(0..1000).map(|i|format!("{}{}", "Ё".repeat(32),i)).collect()),
 ] {
  BASELINE.with(|c|c.lock().unwrap().clear());CANDIDATE.with(|c|c.borrow_mut().clear());
  for b in &bodies {let want=content_hash_arc(b);assert_eq!(baseline(b),want);assert_eq!(candidate(b),want);}
  let mut ns=vec![Vec::<f64>::new();2];
  for round in 0..24 {for variant in if (process+round)%2==0 {[0,1]}else{[1,0]} {
   let f:fn(&str)->Arc<str>=if variant==0 {baseline}else{candidate};
   let start=Instant::now();for _ in 0..16 {for b in &bodies {black_box(f(black_box(b)));}}
   ns[variant].push(start.elapsed().as_nanos() as f64/16.0);
  }}
  let baseline_len=BASELINE.with(|c|c.lock().unwrap().len());let candidate_len=CANDIDATE.with(|c|c.borrow_mut().len());assert!(baseline_len<=1023 && candidate_len<=1023);
  println!("{}",serde_json::json!({"case":name,"bodies":bodies.len(),"ns":ns,"exact_values":true,"baseline_cache_len":baseline_len,"candidate_cache_len":candidate_len}));
 }
}
