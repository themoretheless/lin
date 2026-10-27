use lin::index::PostingList;
use std::hint::black_box;
fn old(p:&PostingList)->bool{p.count()==0}
fn new(p:&PostingList)->bool{p.is_empty()}
fn measure(p:&PostingList,probe:fn(&PostingList)->bool,operations:usize,expected:bool)->f64{
 let probe=black_box(probe);let t=std::time::Instant::now();let mut count=0;
 for _ in 0..operations{count+=usize::from(probe(black_box(p)));}
 let ns=t.elapsed().as_nanos()as f64/operations as f64;assert_eq!(count,usize::from(expected)*operations);ns
}
fn main(){
 let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();let mut cases=Vec::new();
 for capacity in [1024usize,16384,131072,1048576]{
  for shape in ["first_word_nonempty","last_word_nonempty","empty"]{
   let mut list=PostingList::new(true,capacity);if shape=="first_word_nonempty"{list.insert(0);}else if shape=="last_word_nonempty"{list.insert(capacity-1);}
   cases.push((format!("{shape}/{capacity}"),list,shape=="empty",capacity));
  }
 }
 cases.push(("standard_empty".into(),PostingList::new(false,0),true,0));let mut standard=PostingList::new(false,0);for i in 0..31{standard.insert(i);}cases.push(("standard_nonempty".into(),standard,false,0));
 for (index,(name,list,expected,capacity))in cases.iter().enumerate(){
  let operations=if *capacity>=131072{256}else{4096};let mut samples=[Vec::new(),Vec::new(),Vec::new()];
  for sample in 0..24{let shift=(sample+index+process)%3;for j in 0..3{let role=(j+shift)%3;samples[role].push(measure(list,if role==2{new}else{old},operations,*expected));}}
  println!("{}",serde_json::json!({"case":name,"process":process,"operations_per_sample":operations,"samples":samples,"exact_membership":true}));
 }
}
