#[cfg(test)]
mod exact_cardinality_oracle {
    use super::BitSet;
    use std::collections::BTreeSet;

    fn check(bits: &BitSet, model: &BTreeSet<usize>) {
        assert_eq!(bits.count(), model.len());
        assert_eq!(bits.is_empty(), model.is_empty());
        assert_eq!(bits.to_indices(), model.iter().copied().collect::<Vec<_>>());
        let mut streamed = Vec::new();
        bits.for_each(|p| streamed.push(p));
        assert_eq!(streamed, bits.to_indices());
        for p in [0, 1, 63, 64, 65, 1023, 4095, 65535, 65536, 100003] {
            assert_eq!(bits.contains(p), model.contains(&p));
        }
        let clone = bits.clone();
        assert_eq!(clone, *bits);
        assert_eq!(clone.count(), model.len());
    }

    #[test]
    fn exact_cardinality_stateful_model() {
        let mut transitions = 0;
        for capacity in [0, 1, 63, 64, 65, 1024, 65537] {
            let mut bits = BitSet::new(capacity);
            let mut model = BTreeSet::new();
            check(&bits, &model);
            let mut state = 0x1020304050607080u64 ^ capacity as u64;
            for step in 0..2048 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let p = [0, 1, 63, 64, 65, 1023, 4095, 65535, 65536, 100003][state as usize % 10];
                match step % 8 {
                    0 | 1 => { bits.insert(p); model.insert(p); }
                    2 | 3 => { bits.remove(p); model.remove(&p); }
                    4 | 5 => {
                        let mut other = BitSet::new((state as usize >> 8) % 129);
                        let mut rhs = BTreeSet::new();
                        for q in [p, step % 67, (state as usize >> 16) % 100007] {
                            other.insert(q); rhs.insert(q);
                        }
                        if step % 8 == 4 {
                            bits.and_assign(&other);
                            model = model.intersection(&rhs).copied().collect();
                        } else {
                            bits.or_assign(&other);
                            model.extend(rhs);
                        }
                    }
                    6 => { let cloned = bits.clone(); bits.or_assign(&cloned); }
                    _ => { let cloned = bits.clone(); bits.and_assign(&cloned); }
                }
                check(&bits, &model);
                transitions += 1;
            }
        }
        assert_eq!(transitions, 14336);
    }
}
