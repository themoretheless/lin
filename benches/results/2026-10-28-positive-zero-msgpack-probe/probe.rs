use serde::{Serialize, Serializer, ser::SerializeSeq};
use std::sync::Arc;

struct ZeroFloat(f32, bool);
impl Serialize for ZeroFloat {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.0.to_bits() == 0 || (self.1 && self.0 == 0.0) {
            serializer.serialize_u8(0)
        } else {
            serializer.serialize_f32(self.0)
        }
    }
}
struct ZeroVec<'a>(&'a [f32], bool);
impl Serialize for ZeroVec<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for &value in self.0 {
            seq.serialize_element(&ZeroFloat(value, self.1))?;
        }
        seq.end()
    }
}
#[derive(Serialize)]
#[serde(tag = "t", content = "v")]
enum CellWire<'a> {
    Vec(ZeroVec<'a>),
}
fn bits(cell: lin::Cell) -> Vec<u32> {
    let lin::Cell::Vec(values) = cell else { panic!("expected vector") };
    values.iter().map(|v|v.to_bits()).collect()
}
fn main() {
    let mutant = std::env::args().any(|a|a=="--mutant-negative-zero");
    let patterns = [0u32, 0x80000000, 1, 0x80000001, 0x00800000, 0x80800000, 0x3f800000, 0xbf800000, 0x7f800000, 0xff800000, 0x7fc01234, 0x7fa00001, 0xffc05678];
    let mut cases = Vec::new();
    for n in [0usize,1,2,5,31,768,1024] {
        for kind in 0..4 {
            let values = (0..n).map(|i|f32::from_bits(match kind {
                0 => 0,
                1 => 0x80000000,
                2 => patterns[i%patterns.len()],
                _ => if i%12==0 {0x3f000000} else {0},
            })).collect::<Vec<_>>();
            let expected = values.iter().map(|v|v.to_bits()).collect::<Vec<_>>();
            let canonical = rmp_serde::to_vec_named(&lin::Cell::Vec(Arc::from(values.as_slice()))).unwrap();
            let packed = rmp_serde::to_vec_named(&CellWire::Vec(ZeroVec(&values, mutant))).unwrap();
            let original = bits(rmp_serde::from_slice(&canonical).unwrap());
            assert_eq!(original, expected, "canonical decoder does not preserve input bits n={n}/kind={kind}");
            let decoded = bits(rmp_serde::from_slice(&packed).unwrap());
            assert_eq!(decoded, expected, "positive-zero encoding changed f32 bits n={n}/kind={kind}");
            let zeros = values.iter().filter(|v|v.to_bits()==0).count();
            assert_eq!(canonical.len()-packed.len(), zeros*4, "canonical length adjustment n={n}/kind={kind}");
            if zeros==0 {assert_eq!(canonical, packed);}
            cases.push(serde_json::json!({"elements":n,"pattern":kind,"positive_zeros":zeros,"canonical_bytes":canonical.len(),"packed_bytes":packed.len()}));
        }
    }
    println!("{}",serde_json::json!({"format_probe_complete":true,"cases":cases,"performance_qualified":false,"production_wal_changed":false,"legacy_reader_qualified":false}));
}
