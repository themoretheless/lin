#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitSet {
    // Last word is cardinality, excluded from all positional operations.
    bits: Vec<u64>,
    capacity: usize,
}
impl BitSet {
    pub fn new(capacity: usize) -> Self {
        Self { bits: vec![0; capacity.div_ceil(64) + 1], capacity }
    }
    #[inline]
    fn words(&self) -> usize { self.bits.len() - 1 }
    fn grow_words(&mut self, words: usize) {
        let old_words = self.words();
        let count = self.bits[old_words];
        // Resize first: allocation failure must not clear the old metadata.
        self.bits.resize(words + 1, 0);
        self.bits[old_words] = 0;
        self.bits[words] = count;
    }
    #[inline]
    pub fn insert(&mut self, pos: usize) {
        let word = pos >> 6;
        if word >= self.words() {
            let words = (word + 1).next_power_of_two();
            self.grow_words(words);
            self.capacity = words * 64;
        } else if pos >= self.capacity {
            self.capacity = self.words() * 64;
        }
        let mask = 1u64 << (pos & 63);
        let added = u64::from(self.bits[word] & mask == 0);
        self.bits[word] |= mask;
        let meta = self.words();
        self.bits[meta] += added;
    }
    #[inline]
    pub fn remove(&mut self, pos: usize) {
        let word = pos >> 6;
        if word >= self.words() { return; }
        let mask = 1u64 << (pos & 63);
        let removed = u64::from(self.bits[word] & mask != 0);
        self.bits[word] &= !mask;
        let meta = self.words();
        self.bits[meta] -= removed;
    }
    #[inline]
    pub fn contains(&self, pos: usize) -> bool {
        let word = pos >> 6;
        word < self.words() && self.bits[word] & (1u64 << (pos & 63)) != 0
    }
    #[inline]
    pub fn count(&self) -> usize { self.bits[self.words()] as usize }
    pub fn extend_into(&self, out: &mut Vec<usize>) {
        self.for_each(|p| out.push(p));
    }
    pub fn for_each(&self, mut f: impl FnMut(usize)) {
        for (i, &word) in self.bits[..self.words()].iter().enumerate() {
            let mut w = word;
            while w != 0 {
                let p = (i << 6) + w.trailing_zeros() as usize;
                if p < self.capacity { f(p); }
                w &= w - 1;
            }
        }
    }
    pub fn to_indices(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.count());
        self.extend_into(&mut out);
        out
    }
    pub fn and_assign(&mut self, other: &BitSet) {
        let words = self.words();
        let other_words = other.words();
        let mut count = 0u64;
        for (a, b) in self.bits[..words].iter_mut().zip(other.bits[..other_words].iter()) {
            *a &= b;
            count += a.count_ones() as u64;
        }
        if words > other_words { self.bits[other_words..words].fill(0); }
        self.bits[words] = count;
    }
    pub fn or_assign(&mut self, other: &BitSet) {
        let other_words = other.words();
        if other_words > self.words() { self.grow_words(other_words); }
        if other.capacity > self.capacity { self.capacity = other.capacity; }
        let words = self.words();
        let mut count = self.bits[words];
        for (a, b) in self.bits[..words].iter_mut().zip(other.bits[..other_words].iter()) {
            count += (*b & !*a).count_ones() as u64;
            *a |= b;
        }
        self.bits[words] = count;
    }
    pub fn is_empty(&self) -> bool { self.count() == 0 }
    pub fn can_hold(&self, pos: usize) -> bool { pos < self.capacity }
}


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
