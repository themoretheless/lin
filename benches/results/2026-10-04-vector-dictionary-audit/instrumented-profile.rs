//! Diagnostic byte-size/roundtrip audit, not a timing benchmark.
use lin::{Cell, Db};
use std::collections::BTreeMap;
fn audit(values: &[f32]) -> (usize, usize, usize, usize) {
    let mut dictionary = Vec::<u32>::new();
    let mut entries = Vec::<(usize, u8)>::new();
    for (index, value) in values.iter().enumerate() {
        let bits = value.to_bits();
        if bits == 0 { continue; }
        let code = if let Some(code) = dictionary.iter().position(|&v| v == bits) { code } else {
            if dictionary.len() == 255 { return (8 + entries.len()*8 + values[index..].iter().filter(|v| v.to_bits()!=0).count()*8, 0, 256, 0); }
            dictionary.push(bits); dictionary.len()-1
        };
        entries.push((index, code as u8));
    }
    let index_bytes = if values.len() <= 65536 { 2 } else { 4 };
    let mut encoded = Vec::new();
    encoded.push(dictionary.len() as u8);
    for bits in &dictionary { encoded.extend_from_slice(&bits.to_le_bytes()); }
    for &(index, code) in &entries {
        if index_bytes == 2 { encoded.extend_from_slice(&(index as u16).to_le_bytes()); }
        else { encoded.extend_from_slice(&(index as u32).to_le_bytes()); }
        encoded.push(code);
    }
    let d = encoded[0] as usize;
    let mut actual = vec![0u32; values.len()];
    let payload = &encoded[1 + d*4..];
    for bytes in payload.chunks_exact(index_bytes + 1) {
        let index = if index_bytes == 2 { u16::from_le_bytes(bytes[..2].try_into().unwrap()) as usize } else { u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize };
        let code = bytes[index_bytes] as usize;
        let offset = 1 + code*4;
        actual[index] = u32::from_le_bytes(encoded[offset..offset+4].try_into().unwrap());
    }
    assert_eq!(actual, values.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
    (8 + entries.len()*8, 8 + encoded.len(), dictionary.len(), entries.len())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    for values in [vec![], vec![0.0; 768], vec![-0.0, f32::from_bits(0x7fc00001), f32::INFINITY, -1.25, -0.0]] { audit(&values); }
    for n in [1000usize,10000] {
        let source = format!("insert docs [{}]", (0..n).map(|i| {
            let title = if i%10==0 { format!("doc {i} wal note") } else { format!("doc {i} plain") };
            let wing = if i%2==0 { "rag" } else { "sys" };
            format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000} else {30*86400000})
        }).collect::<Vec<_>>().join(","));
        let mut db = Db::empty(); db.run("index docs [wing, ts]")?;
        assert_eq!(db.run(&source)?.done.n,n);
        let (mut raw, mut packed, mut nonzero) = (0,0,0);
        let mut histogram = BTreeMap::new();
        for row in db.store.collection("docs") {
            let Cell::Vec(values) = &row["embedding"] else { panic!("embedding"); };
            assert_eq!(values.len(),768);
            let (r,p,d,c) = audit(values); assert!(p>0);
            raw += r; packed += p; nonzero += c; *histogram.entry(d).or_insert(0usize) += 1;
        }
        println!("{}",serde_json::json!({"rows":n,"sparse_vector_bytes":raw,"dictionary_vector_bytes":packed,"nonzero_entries":nonzero,"dictionary_size_histogram":histogram,"bits_roundtrip":true}));
    }
    Ok(())
}
