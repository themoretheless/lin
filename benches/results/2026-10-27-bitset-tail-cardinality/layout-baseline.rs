#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitSet {
    bits: Vec<u64>,
    capacity: usize,
}

impl BitSet {
    pub fn new(capacity: usize) -> Self {
        let num_words = capacity.div_ceil(64);
        Self {
            bits: vec![0u64; num_words],
            capacity,
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
        self.bits[word_idx] |= 1u64 << bit_idx;
    }

    #[inline]
    pub fn remove(&mut self, pos: usize) {
        let word_idx = pos >> 6;
        if word_idx >= self.bits.len() {
            return;
        }
        let bit_idx = pos & 63;
        self.bits[word_idx] &= !(1u64 << bit_idx);
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
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
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
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a &= b;
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
            *a |= b;
        }
    }

    /// Check if any bits are set
    pub fn is_empty(&self) -> bool {
        self.bits.iter().all(|&w| w == 0)
    }

    /// Check if all bits would be within capacity
    pub fn can_hold(&self, pos: usize) -> bool {
        pos < self.capacity
    }
}


#[allow(dead_code)]enum ModelPosting{Standard(Vec<usize>),Bitmap(BitSet)}
fn show(initial:usize,position:Option<usize>,s:&BitSet){
 println!("{}",format!("{{\"initial_capacity\":{},\"inserted_position\":{},\"logical_capacity\":{},\"count\":{},\"buffer_len_bytes\":{},\"buffer_capacity_bytes\":{},\"bitset_bytes\":{},\"posting_model_bytes\":{}}}",initial,position.map(|p|p.to_string()).unwrap_or("null".into()),s.capacity,s.count(),s.bits.len()*8,s.bits.capacity()*8,std::mem::size_of::<BitSet>(),std::mem::size_of::<ModelPosting>()));
}
fn main(){for cap in [0,1,63,64,65,1024,65537]{let mut s=BitSet::new(cap);show(cap,None,&s);for pos in [0,63,64,127,128,65536,100003]{s.insert(pos);show(cap,Some(pos),&s);}}}
