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


#[allow(dead_code)]enum ModelPosting{Standard(Vec<usize>),Bitmap(BitSet)}
fn show(initial:usize,position:Option<usize>,s:&BitSet){
 println!("{}",format!("{{\"initial_capacity\":{},\"inserted_position\":{},\"logical_capacity\":{},\"count\":{},\"buffer_len_bytes\":{},\"buffer_capacity_bytes\":{},\"bitset_bytes\":{},\"posting_model_bytes\":{}}}",initial,position.map(|p|p.to_string()).unwrap_or("null".into()),s.capacity,s.count(),s.bits.len()*8,s.bits.capacity()*8,std::mem::size_of::<BitSet>(),std::mem::size_of::<ModelPosting>()));
}
fn main(){for cap in [0,1,63,64,65,1024,65537]{let mut s=BitSet::new(cap);show(cap,None,&s);for pos in [0,63,64,127,128,65536,100003]{s.insert(pos);show(cap,Some(pos),&s);}}}
