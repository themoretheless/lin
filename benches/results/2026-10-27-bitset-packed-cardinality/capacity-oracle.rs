#[cfg(test)]
mod tail_capacity_oracle {
use super::BitSet;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceBitSet {
    bits: Vec<u64>,
    capacity: usize,
}

impl ReferenceBitSet {
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
    pub fn and_assign(&mut self, other: &ReferenceBitSet) {
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
    pub fn or_assign(&mut self, other: &ReferenceBitSet) {
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


    #[test]
    fn tail_capacity_differential() {
        assert_eq!(std::mem::size_of::<BitSet>(),std::mem::size_of::<ReferenceBitSet>());
        for capacity in [0,1,63,64,65,1023,65537] {
            let mut actual=BitSet::new(capacity);let mut reference=ReferenceBitSet::new(capacity);
            let mut state=0xabcde12345u64+capacity as u64;
            for step in 0..4096 {
                state^=state<<13;state^=state>>7;state^=state<<17;
                let pos=[0,31,32,63,64,65,127,128,65535,65536,100003][state as usize%11];
                match step%5 {
                    0=>{actual.insert(pos);reference.insert(pos);},
                    1=>{actual.remove(pos);reference.remove(pos);},
                    2|3=>{
                        let cap=[0,1,63,64,65,1024,65537][state as usize%7];
                        let mut a=BitSet::new(cap);let mut b=ReferenceBitSet::new(cap);
                        if state%3!=0 {for p in [pos,cap/2] {a.insert(p);b.insert(p);}}
                        if step%5==2 {actual.and_assign(&a);reference.and_assign(&b);}
                        else {actual.or_assign(&a);reference.or_assign(&b);}
                    },
                    _=>{actual=actual.clone();reference=reference.clone();}
                }
                assert_eq!(actual.count(),reference.count());assert_eq!(actual.is_empty(),reference.is_empty());
                assert_eq!(actual.to_indices(),reference.to_indices());
                for p in [0,1,31,32,63,64,65,127,128,capacity,capacity.saturating_add(1),65535,65536,100003,131071,131072] {
                    assert_eq!(actual.can_hold(p),reference.can_hold(p));
                    assert_eq!(actual.contains(p),reference.contains(p));
                }
            }
        }
    }

}
