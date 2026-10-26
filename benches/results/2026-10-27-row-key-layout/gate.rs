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
fn time<T:Clone>(v:&T)->f64{let t=std::time::Instant::now();for _ in 0..1024{drop(std::hint::black_box(v.clone()));}t.elapsed().as_nanos()as f64/1024.}
fn count<T:Clone>(v:&T)->(usize,usize){ALLOCS.store(0,Relaxed);BYTES.store(0,Relaxed);ACTIVE.store(true,Relaxed);drop(std::hint::black_box(v.clone()));ACTIVE.store(false,Relaxed);(ALLOCS.load(Relaxed),BYTES.load(Relaxed))}
fn main(){
 for shape in ["short9","short32","long9","mixed32","unicode9"]{
  let n=if shape.ends_with("32"){32}else{9};let mut a:BTreeMap<String,Cell>=BTreeMap::new();let mut b:BTreeMap<Key,Cell>=BTreeMap::new();
  for i in 0..n {let key=match shape{"long9"=>format!("long_field_name_exceeding_inline_capacity_{i}"),"mixed32" if i%2==0=>format!("long_field_name_exceeding_inline_capacity_{i}"),"unicode9"=>format!("поле_{i}"),_=>format!("field_{i}")};let v=if i%2==0{Cell::Int(i)}else{Cell::text_arc("some payload shared across row copies")};a.insert(key.clone(),v.clone());b.insert(Key::new(&key),v);}
  assert_eq!(a.len(),b.len());for(k,v)in&a{assert_eq!(b.get(k.as_str()),Some(v));}
  let ca=count(&a);let cb=count(&b);
  for sample in 0..24{let mut values=[0.;3];for offset in 0..3{let variant=(sample+offset)%3;values[variant]=if variant==2{time(&b)}else{time(&a)};}
   println!("{}",serde_json::json!({"shape":shape,"sample":sample,"baseline_ns":values[0],"control_ns":values[1],"candidate_ns":values[2],"baseline_allocations":ca.0,"candidate_allocations":cb.0,"baseline_bytes":ca.1,"candidate_bytes":cb.1,"exact_lookup":true,"candidate_key_size":std::mem::size_of::<Key>(),"baseline_key_size":std::mem::size_of::<String>()}));}
 }
}
