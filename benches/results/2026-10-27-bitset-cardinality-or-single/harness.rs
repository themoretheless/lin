use std::hint::black_box;
use std::time::Instant;
#[allow(dead_code)]
enum LayoutPosting { Standard(Vec<usize>), Bitmap(BitSet) }
fn fixture(n:usize,shape:&str)->BitSet {
    let mut s=BitSet::new(n);
    match shape {
        "empty"=>{},
        "sparse"=>{s.insert(n-1);s.insert(n/2);},
        "dense"=>{for p in (0..n).step_by(2){s.insert(p);}},
        _=>unreachable!()
    }
    s
}
fn main(){
    println!("{{\"layout\":true,\"bitset_bytes\":{},\"posting_bytes\":{}}}",std::mem::size_of::<BitSet>(),std::mem::size_of::<LayoutPosting>());
    for n in [128,1024,65536,1048576] {
      for shape in ["empty","sparse","dense"] {
        let initial=fixture(n,shape);let other=fixture(n/2+1,"sparse");
        let original=initial.to_indices();
        for op in ["count","empty","toggle","and","and_count","or","or_count","and_reverse","and_count_reverse","or_reverse","or_count_reverse"] {
          let expected:usize=match op {
            op if op.starts_with("and")=>original.iter().filter(|&&p|other.contains(p)).count(),
            op if op.starts_with("or")=>original.len()+other.to_indices().into_iter().filter(|&p|!initial.contains(p)).count(),
            _=>original.len()
          };
          for sample in 0..10 {
            let iterations=if op=="count"||op=="empty"||op=="toggle"{2048}else{64};
            let mut s=initial.clone();
            let lhs=if op.ends_with("reverse"){&other}else{&initial};let rhs=if op.ends_with("reverse"){&initial}else{&other};
            let mut copies=if op.starts_with("and")||op.starts_with("or"){vec![lhs.clone();iterations]}else{Vec::new()};
            let bit=n-1;let had=s.contains(bit);let start=Instant::now();
            match op {
              "count"=>for _ in 0..iterations{black_box(black_box(&s).count());},
              "empty"=>for _ in 0..iterations{black_box(black_box(&s).is_empty());},
              "toggle"=>for _ in 0..iterations{black_box(&mut s).insert(bit);black_box(&mut s).remove(bit);},
              op if op.starts_with("and")=>for s in &mut copies{black_box(&mut *s).and_assign(black_box(rhs));if op.contains("count"){black_box(s.count());}},
              op if op.starts_with("or")=>for s in &mut copies{black_box(&mut *s).or_assign(black_box(rhs));if op.contains("count"){black_box(s.count());}},
              _=>unreachable!()
            }
            let elapsed=start.elapsed().as_nanos();
            if op=="toggle"{if had{s.insert(bit);}assert_eq!(s.to_indices(),original);}
            else if copies.is_empty(){assert_eq!(s.to_indices(),original);}
            else{for s in &copies{assert_eq!(s.count(),expected);}}
            println!("{{\"case\":\"{n}/{shape}/{op}\",\"sample\":{sample},\"warmup\":{},\"iterations\":{iterations},\"elapsed_ns\":{elapsed}}}",sample<2);
          }
        }
      }
    }
}
