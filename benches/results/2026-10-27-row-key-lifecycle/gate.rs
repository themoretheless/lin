use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};
use lin::Cell;
struct Count;
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static FREES: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if ACTIVE.load(Relaxed) { ALLOCS.fetch_add(1,Relaxed); BYTES.fetch_add(l.size(),Relaxed); }
        unsafe {System.alloc(l)}
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        if ACTIVE.load(Relaxed) { ALLOCS.fetch_add(1,Relaxed); BYTES.fetch_add(l.size(),Relaxed); }
        unsafe {System.alloc_zeroed(l)}
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        if ACTIVE.load(Relaxed) { FREES.fetch_add(1,Relaxed); }
        unsafe {System.dealloc(p,l)}
    }
    unsafe fn realloc(&self, p:*mut u8, old:Layout, n:usize)->*mut u8 {
        if ACTIVE.load(Relaxed) { ALLOCS.fetch_add(1,Relaxed); BYTES.fetch_add(n,Relaxed); }
        unsafe {System.realloc(p,old,n)}
    }
}
#[global_allocator] static GLOBAL: Count = Count;
use std::{collections::BTreeMap,sync::Arc,borrow::Borrow};
#[derive(Clone,Eq)]
enum Key{Inline(lin::InlineStr),Shared(Arc<str>)}
impl Key{fn new(s:&str)->Self{lin::InlineStr::try_new(s).map(Self::Inline).unwrap_or_else(||Self::Shared(Arc::from(s)))}fn text(&self)->&str{match self{Self::Inline(s)=>s.as_str(),Self::Shared(s)=>s}}}
impl Borrow<str> for Key{fn borrow(&self)->&str{self.text()}}
impl PartialEq for Key{fn eq(&self,o:&Self)->bool{self.text()==o.text()}}
impl Ord for Key{fn cmp(&self,o:&Self)->std::cmp::Ordering{self.text().cmp(o.text())}}
impl PartialOrd for Key{fn partial_cmp(&self,o:&Self)->Option<std::cmp::Ordering>{Some(self.cmp(o))}}
impl serde::Serialize for Key{fn serialize<S:serde::Serializer>(&self,s:S)->Result<S::Ok,S::Error>{s.serialize_str(self.text())}}
impl<'de> serde::Deserialize<'de> for Key{fn deserialize<D:serde::Deserializer<'de>>(d:D)->Result<Self,D::Error>{let s=String::deserialize(d)?;Ok(Key::new(&s))}}
use serde::Deserialize;
fn baseline(input:&[(String,Cell)],copies:bool){let a:BTreeMap<String,Cell>=input.iter().cloned().collect();if copies{let b=a.clone();let mut c=b.clone();*c.get_mut(input[0].0.as_str()).unwrap()=Cell::Int(123);std::hint::black_box(c.get(input[0].0.as_str()));drop(std::hint::black_box((b,c)));}drop(std::hint::black_box(a));}
fn candidate(input:&[(String,Cell)],copies:bool){let a:BTreeMap<Key,Cell>=input.iter().map(|(k,v)|(Key::new(k),v.clone())).collect();if copies{let b=a.clone();let mut c=b.clone();*c.get_mut(input[0].0.as_str()).unwrap()=Cell::Int(123);std::hint::black_box(c.get(input[0].0.as_str()));drop(std::hint::black_box((b,c)));}drop(std::hint::black_box(a));}
fn main(){
 for shape in ["short9","short32","long9","mixed32","unicode9","boundary9"]{
  let n=if shape.ends_with("32"){32}else{9};let mut input=Vec::new();
  for i in 0..n {let key=match shape{"long9"=>format!("long_field_name_exceeding_inline_capacity_{i}"),"mixed32" if i%2==0=>format!("long_field_name_exceeding_inline_capacity_{i}"),"unicode9"=>format!("поле_{i}"),"boundary9"=>format!("{}{}","x".repeat(if i%2==0{21}else{22}),i),_=>format!("field_{i}")};let v=if i%2==0{Cell::Int(i)}else{Cell::text_arc("some payload shared across row copies")};input.push((key,v));}
  let a:BTreeMap<String,Cell>=input.iter().cloned().collect();let b:BTreeMap<Key,Cell>=input.iter().map(|(k,v)|(Key::new(k),v.clone())).collect();
  let ja=serde_json::to_vec(&a).unwrap();let jb=serde_json::to_vec(&b).unwrap();assert_eq!(ja,jb);
  let ma=rmp_serde::to_vec(&a).unwrap();let mb=rmp_serde::to_vec(&b).unwrap();assert_eq!(ma,mb);
  for bytes in [&ma,&mb]{let old:BTreeMap<String,Cell>=rmp_serde::from_slice(bytes).unwrap();let new:BTreeMap<Key,Cell>=rmp_serde::from_slice(bytes).unwrap();assert_eq!(old,a);for(k,v)in&old{assert_eq!(new.get(k.as_str()),Some(v));}}
  let new:BTreeMap<Key,Cell>=serde_json::from_slice(&ja).unwrap();for(k,v)in&a{assert_eq!(new.get(k.as_str()),Some(v));}
  for copies in [false,true]{
   let mut counts=Vec::new();for v in 0..2{ALLOCS.store(0,Relaxed);BYTES.store(0,Relaxed);ACTIVE.store(true,Relaxed);if v==0{baseline(&input,copies)}else{candidate(&input,copies)}ACTIVE.store(false,Relaxed);counts.push((ALLOCS.load(Relaxed),BYTES.load(Relaxed)));}
   for sample in 0..24{let mut times=[0.;3];for offset in 0..3{let variant=(sample+offset)%3;let t=std::time::Instant::now();for _ in 0..512{if variant==2{candidate(std::hint::black_box(&input),copies)}else{baseline(std::hint::black_box(&input),copies)}}times[variant]=t.elapsed().as_nanos()as f64/512.;}
    println!("{}",serde_json::json!({"shape":shape,"copies":copies,"sample":sample,"baseline_ns":times[0],"control_ns":times[1],"candidate_ns":times[2],"before_allocations":counts[0].0,"after_allocations":counts[1].0,"before_bytes":counts[0].1,"after_bytes":counts[1].1,"exact_serde":true}));
   }
  }
 }
}
