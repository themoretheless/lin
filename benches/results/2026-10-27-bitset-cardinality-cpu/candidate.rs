#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitSet {
    bits: Vec<u64>,
    capacity: usize,
    cardinality: usize,
}

impl BitSet {
    pub fn new(capacity: usize) -> Self {
        let num_words = capacity.div_ceil(64);
        Self {
            bits: vec![0u64; num_words],
            capacity,
            cardinality: 0,
        }
    }

    #[inline]
    pub fn insert(&mut self, pos: usize) {
        let word_idx = pos >> 6; // pos / 64
        let bit_idx = pos & 63; // pos % 64
        if word_idx >= self.bits.len() {
            let new_words = (word_idx + 1).next_power_of_two();
            self.bits.resize(new_words, 0);
            self.capacity = new_words * 64;
        } else if pos >= self.capacity {
            self.capacity = self.bits.len() * 64;
        }
        let mask = 1u64 << bit_idx;
        self.cardinality += usize::from(self.bits[word_idx] & mask == 0);
        self.bits[word_idx] |= mask;
    }

    #[inline]
    pub fn remove(&mut self, pos: usize) {
        let word_idx = pos >> 6;
        if word_idx >= self.bits.len() {
            return;
        }
        let bit_idx = pos & 63;
        let mask = 1u64 << bit_idx;
        self.cardinality -= usize::from(self.bits[word_idx] & mask != 0);
        self.bits[word_idx] &= !mask;
    }

    #[inline]
    pub fn contains(&self, pos: usize) -> bool {
        let word_idx = pos >> 6;
        if word_idx >= self.bits.len() {
            return false;
        }
        let bit_idx = pos & 63;
        (self.bits[word_idx] & (1u64 << bit_idx)) != 0
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.cardinality
    }

    /// Append set positions directly into destination vector without intermediate allocations
    pub fn extend_into(&self, out: &mut Vec<usize>) {
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let base_pos = word_idx << 6;
            let mut w = word;
            while w != 0 {
                let bit_idx = w.trailing_zeros() as usize;
                let pos = base_pos + bit_idx;
                if pos < self.capacity {
                    out.push(pos);
                }
                w &= w - 1;
            }
        }
    }

    /// Invoke closure for each set position without allocating vectors
    pub fn for_each(&self, mut f: impl FnMut(usize)) {
        for (word_idx, &word) in self.bits.iter().enumerate() {
            if word == 0 {
                continue;
            }
            let base_pos = word_idx << 6;
            let mut w = word;
            while w != 0 {
                let bit_idx = w.trailing_zeros() as usize;
                let pos = base_pos + bit_idx;
                if pos < self.capacity {
                    f(pos);
                }
                w &= w - 1;
            }
        }
    }

    /// Convert to Vec<usize> of set positions
    pub fn to_indices(&self) -> Vec<usize> {
        let mut out = Vec::with_capacity(self.count());
        self.extend_into(&mut out);
        out
    }

    /// Bitwise AND (intersection) - modifies self
    pub fn and_assign(&mut self, other: &BitSet) {
        self.cardinality = 0;
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a &= b;
            self.cardinality += a.count_ones() as usize;
        }
        if self.bits.len() > other.bits.len() {
            for a in &mut self.bits[other.bits.len()..] {
                *a = 0;
            }
        }
    }

    /// Bitwise OR (union) - modifies self
    pub fn or_assign(&mut self, other: &BitSet) {
        if other.bits.len() > self.bits.len() {
            self.bits.resize(other.bits.len(), 0);
        }
        if other.capacity > self.capacity {
            self.capacity = other.capacity;
        }
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            let before = a.count_ones();
            *a |= b;
            self.cardinality += (a.count_ones() - before) as usize;
        }
    }

    /// Check if any bits are set
    pub fn is_empty(&self) -> bool {
        self.cardinality == 0
    }

    /// Check if all bits would be within capacity
    pub fn can_hold(&self, pos: usize) -> bool {
        pos < self.capacity
    }
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
