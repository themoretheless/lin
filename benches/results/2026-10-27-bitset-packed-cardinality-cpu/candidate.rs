#[derive(Clone, PartialEq, Eq)]
pub struct BitSet {
    bits: Vec<u64>,
    // Packed count/capacity only when packed(); otherwise original capacity.
    capacity: usize,
}
impl std::fmt::Debug for BitSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BitSet").field("bits", &self.bits)
            .field("capacity", &self.logical_capacity()).finish()
    }
}
impl BitSet {
    // At most u32::MAX - 63 bits: both count and logical capacity fit u32.
    const PACKED_MAX_WORDS: usize = (u32::MAX as usize) / 64;
    #[inline]
    fn packed(&self) -> bool {
        usize::BITS == 64 && self.bits.len() <= Self::PACKED_MAX_WORDS
    }
    #[inline]
    fn logical_capacity(&self) -> usize {
        if self.packed() { self.capacity & u32::MAX as usize } else { self.capacity }
    }
    #[inline]
    fn cached_count(&self) -> usize { self.capacity.wrapping_shr(32) }
    #[inline]
    fn save_metadata(&mut self, capacity: usize, count: usize) {
        self.capacity = if self.packed() { capacity | count.wrapping_shl(32) } else { capacity };
    }
    pub fn new(capacity: usize) -> Self {
        Self { bits: vec![0; capacity.div_ceil(64)], capacity }
    }
    #[inline]
    pub fn insert(&mut self, pos: usize) {
        let word = pos >> 6;
        let mut capacity = self.logical_capacity();
        let mut count = if self.packed() { self.cached_count() } else { 0 };
        if word >= self.bits.len() {
            let words = (word + 1).next_power_of_two();
            self.bits.resize(words, 0);
            capacity = words * 64;
        } else if pos >= capacity { capacity = self.bits.len() * 64; }
        let mask = 1u64 << (pos & 63);
        count += usize::from(self.bits[word] & mask == 0);
        self.bits[word] |= mask;
        self.save_metadata(capacity, count);
    }
    #[inline]
    pub fn remove(&mut self, pos: usize) {
        let word = pos >> 6;
        if word >= self.bits.len() { return; }
        let mask = 1u64 << (pos & 63);
        let removed = usize::from(self.bits[word] & mask != 0);
        self.bits[word] &= !mask;
        if self.packed() { self.capacity -= removed.wrapping_shl(32); }
    }
    #[inline]
    pub fn contains(&self, pos: usize) -> bool {
        let word = pos >> 6;
        word < self.bits.len() && self.bits[word] & (1u64 << (pos & 63)) != 0
    }
    #[inline]
    pub fn count(&self) -> usize {
        if self.packed() { self.cached_count() }
        else { self.bits.iter().map(|w| w.count_ones() as usize).sum() }
    }
    pub fn extend_into(&self, out: &mut Vec<usize>) { self.for_each(|p| out.push(p)); }
    pub fn for_each(&self, mut f: impl FnMut(usize)) {
        let capacity = self.logical_capacity();
        for (i, &word) in self.bits.iter().enumerate() {
            let mut w = word;
            while w != 0 {
                let p = (i << 6) + w.trailing_zeros() as usize;
                if p < capacity { f(p); }
                w &= w - 1;
            }
        }
    }
    pub fn to_indices(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.count());self.extend_into(&mut out);out
    }
    pub fn and_assign(&mut self, other: &BitSet) {
        let capacity = self.logical_capacity();
        if self.packed() {
            let mut count = 0usize;
            for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
                *a &= b;count += a.count_ones() as usize;
            }
            if self.bits.len() > other.bits.len() { self.bits[other.bits.len()..].fill(0); }
            self.save_metadata(capacity, count);
        } else {
            for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) { *a &= b; }
            if self.bits.len() > other.bits.len() { self.bits[other.bits.len()..].fill(0); }
        }
    }
    pub fn or_assign(&mut self, other: &BitSet) {
        let capacity = self.logical_capacity().max(other.logical_capacity());
        let mut count = if self.packed() { self.cached_count() } else { 0 };
        if other.bits.len() > self.bits.len() { self.bits.resize(other.bits.len(), 0); }
        if self.packed() {
            for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
                count += (*b & !*a).count_ones() as usize;*a |= b;
            }
        } else {
            for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) { *a |= b; }
        }
        self.save_metadata(capacity, count);
    }
    pub fn is_empty(&self) -> bool {
        if self.packed() { self.cached_count() == 0 }
        else { self.bits.iter().all(|&w| w == 0) }
    }
    pub fn can_hold(&self, pos: usize) -> bool { pos < self.logical_capacity() }
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
