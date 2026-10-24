//! Trusted-fixture CPU/byte gate only: no production WAL format or decoder.
use lin::{Cell, Db};
use std::{hint::black_box, sync::Arc, time::Instant};
type Error = String;
fn io_err(s: &str) -> Error { s.to_owned() }
fn encode_sparse_reference(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    for row in rows {
        let Some(emb) = row else {
            buf.extend_from_slice(&0u32.to_le_bytes());
            continue;
        };
        let dim = u32::try_from(emb.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| io_err("sparse vector dimension overflow"))?;
        buf.extend_from_slice(&dim.to_le_bytes());
        let count_offset = buf.len();
        buf.extend_from_slice(&0u32.to_le_bytes());
        let mut count = 0u32;
        for (block, values) in emb.chunks(8).enumerate() {
            // Bitwise zero preserves negative zero and NaN payloads.
            // Skip empty blocks without branching for each element.
            if values
                .iter()
                .fold(0u32, |bits, value| bits | value.to_bits())
                == 0
            {
                continue;
            }
            for (offset, value) in values.iter().enumerate() {
                let bits = value.to_bits();
                if bits != 0 {
                    let index = (block * 8 + offset) as u32;
                    let mut entry = [0u8; 8];
                    entry[..4].copy_from_slice(&index.to_le_bytes());
                    entry[4..].copy_from_slice(&bits.to_le_bytes());
                    buf.extend_from_slice(&entry);
                    count += 1;
                }
            }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
}

#[derive(Default, Clone, Copy)]
struct DictStats { selected: bool, dictionaries: usize, escapes: usize, legacy_bytes: usize }
fn raw_escape(row: &Option<Arc<[f32]>>, out: &mut Vec<u8>) -> Result<usize, Error> {
    let start = out.len();
    encode_sparse_reference(std::slice::from_ref(row), out)?;
    let legacy = out.len() - start;
    if row.is_some() { out.insert(start + 8, 0); }
    Ok(legacy)
}
fn encode_dictionary(rows: &[Option<Arc<[f32]>>], out: &mut Vec<u8>, force: bool) -> Result<DictStats, Error> {
    let begin = out.len();
    let mut stats = DictStats::default();
    'rows: for row in rows {
        let Some(values) = row else {
            out.extend_from_slice(&0u32.to_le_bytes()); stats.legacy_bytes += 4; continue;
        };
        let dim = u32::try_from(values.len()).ok().and_then(|n| n.checked_add(1)).ok_or_else(|| io_err("dimension overflow"))?;
        let start = out.len();
        out.extend_from_slice(&dim.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(0);
        let entries_start = out.len() + 64;
        out.resize(entries_start, 0);
        let mut dictionary = [0u32;16]; let mut d = 0usize; let mut count = 0u32;
        let narrow = values.len() <= 65536;
        for (block, chunk) in values.chunks(8).enumerate() {
            if chunk.iter().fold(0u32, |bits,v| bits|v.to_bits()) == 0 { continue; }
            for (offset, value) in chunk.iter().enumerate() {
                let bits = value.to_bits(); if bits == 0 { continue; }
                let code = if let Some(code) = dictionary[..d].iter().position(|&x| x == bits) { code } else {
                    if d == 16 {
                        out.truncate(start); stats.legacy_bytes += raw_escape(row,out)?; stats.escapes += 1; continue 'rows;
                    }
                    dictionary[d] = bits; d += 1; d-1
                };
                let index = (block*8+offset) as u32;
                let mut entry = [0u8;5];
                let len = if narrow {
                    entry[..2].copy_from_slice(&(index as u16).to_le_bytes()); entry[2] = code as u8; 3
                } else {
                    entry[..4].copy_from_slice(&index.to_le_bytes()); entry[4] = code as u8; 5
                };
                out.extend_from_slice(&entry[..len]); count += 1;
            }
        }
        let legacy = 8 + count as usize*8;
        let dictionary_bytes = 9 + d*4 + count as usize*if narrow {3} else {5};
        if d == 0 || dictionary_bytes >= legacy {
            out.truncate(start); stats.legacy_bytes += raw_escape(row,out)?; stats.escapes += 1; continue;
        }
        out[start+4..start+8].copy_from_slice(&count.to_le_bytes()); out[start+8] = d as u8;
        for (i,bits) in dictionary[..d].iter().enumerate() { out[start+9+i*4..start+13+i*4].copy_from_slice(&bits.to_le_bytes()); }
        let end = out.len();
        out.copy_within(entries_start..end, start+9+d*4);
        out.truncate(start+dictionary_bytes);
        stats.legacy_bytes += legacy; stats.dictionaries += 1;
    }
    stats.selected = force || out.len()-begin < stats.legacy_bytes;
    if !stats.selected { out.truncate(begin); encode_sparse_reference(rows,out)?; }
    Ok(stats)
}
// Trusted diagnostic parser, NOT the production untrusted-input decoder.
fn decode(bytes: &[u8], rows: usize, dictionary_mode: bool) -> Vec<Option<Vec<u32>>> {
    fn take<'a>(bytes: &mut &'a [u8], n: usize) -> &'a [u8] { let (a,b)=bytes.split_at(n); *bytes=b; a }
    fn u32v(bytes: &mut &[u8]) -> u32 { u32::from_le_bytes(take(bytes,4).try_into().unwrap()) }
    let mut input=bytes; let mut result=Vec::new();
    for _ in 0..rows {
        let code=u32v(&mut input); if code==0 {result.push(None);continue;}
        let dim=(code-1) as usize; let count=u32v(&mut input) as usize; assert!(count<=dim);
        let d=if dictionary_mode {take(&mut input,1)[0] as usize} else {0}; assert!(d<=16);
        let dict=(0..d).map(|_|u32v(&mut input)).collect::<Vec<_>>();
        assert!(dict.iter().all(|&x|x!=0));
        let mut values=vec![0u32;dim]; let mut previous=None;
        for _ in 0..count {
            let index=if d>0 && dim<=65536 {u16::from_le_bytes(take(&mut input,2).try_into().unwrap()) as usize} else {u32v(&mut input) as usize};
            assert!(index<dim && previous.is_none_or(|p|p<index)); previous=Some(index);
            let bits=if d>0 {let ordinal=take(&mut input,1)[0] as usize; dict[ordinal]} else {u32v(&mut input)};
            assert_ne!(bits,0);values[index]=bits;
        }
        result.push(Some(values));
    }
    assert!(input.is_empty()); result
}
fn expected(rows: &[Option<Arc<[f32]>>]) -> Vec<Option<Vec<u32>>> {
    rows.iter().map(|v|v.as_ref().map(|v|v.iter().map(|x|x.to_bits()).collect())).collect()
}
fn check(rows: &[Option<Arc<[f32]>>]) -> (usize,usize,DictStats) {
    let mut old=Vec::new(); encode_sparse_reference(rows,&mut old).unwrap();
    let mut new=Vec::new(); let stats=encode_dictionary(rows,&mut new,false).unwrap();
    assert_eq!(stats.legacy_bytes,old.len());
    assert_eq!(decode(&old,rows.len(),false),expected(rows));
    assert_eq!(decode(&new,rows.len(),stats.selected),expected(rows));
    let mut forced=vec![1,2,3];let forced_stats=encode_dictionary(rows,&mut forced,true).unwrap();assert!(forced_stats.selected);
    assert_eq!(&forced[..3],&[1,2,3]);assert_eq!(decode(&forced[3..],rows.len(),true),expected(rows));
    (old.len(),new.len(),stats)
}
fn fixture(n:usize) -> Vec<Option<Arc<[f32]>>> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").iter().map(|r|{let Cell::Vec(v)=&r["embedding"] else {panic!("embedding")};assert_eq!(v.len(),768);Some(v.clone())}).collect()
}
fn timings(rows:&[Option<Arc<[f32]>>], loops:usize, process:usize) -> (Vec<f64>,Vec<f64>) {
    let reserve=rows.iter().flatten().map(|v|v.len()*8+80).sum::<usize>();
    let mut a=Vec::with_capacity(reserve);let mut b=Vec::with_capacity(reserve);
    let mut old=Vec::new();let mut new=Vec::new();
    for round in 0..12 {
        for variant in if (round+process)%2==0 {[0,1]} else {[1,0]} {
            let buffer=if variant==0 {&mut a}else{&mut b};
            let start=Instant::now();
            for _ in 0..loops {
                buffer.clear();
                if variant==0 {encode_sparse_reference(black_box(rows),buffer).unwrap();}
                else {let s=encode_dictionary(black_box(rows),buffer,false).unwrap();black_box(s.selected);}
                black_box(buffer.as_slice());
            }
            let ns=start.elapsed().as_nanos() as f64/loops as f64;
            if variant==0 {old.push(ns)} else {new.push(ns)}
        }
    }
    (old,new)
}
fn main() {
    let process=std::env::args().nth(1).unwrap_or("1".into()).parse::<usize>().unwrap();
    let mut special:Vec<Option<Arc<[f32]>>>=vec![None,Some(Arc::from([])),Some(Arc::from([0.0;9])),Some(Arc::from([-0.0,f32::from_bits(0x7fc00001),f32::from_bits(0x7fc00002),f32::INFINITY,f32::NEG_INFINITY,-1.25,-0.0]))];
    for dim in [8,9,65535,65536,65537] {let mut v=vec![0.0;dim];v[0]=-0.0;v[dim/2]=f32::from_bits(0x7fa00001);v[dim-1]=1.5;special.push(Some(v.into()));}
    special.push(Some((1..=64).map(|i|f32::from_bits(0x3f000000+i)).collect::<Vec<_>>().into()));
    check(&special);
    for (name,rows,loops) in [
        ("default_1k",fixture(1000),48),
        ("default_10k",fixture(10000),6),
        ("high_cardinality_sparse",(0..1000).map(|_|{let mut v=vec![0.0;768];for (i,x) in v[..64].iter_mut().enumerate(){*x=f32::from_bits(0x3f000000+i as u32+1);}Some(v.into())}).collect(),32),
        ("mixed_bit_boundaries",special,32),
    ] {
        let (old_bytes,new_bytes,stats)=check(&rows);let (old,new)=timings(&rows,loops,process);
        println!("{}",serde_json::json!({"case":name,"rows":rows.len(),"loops":loops,"rounds":12,"sparse_bytes":old_bytes,"candidate_bytes":new_bytes,"dictionary_selected":stats.selected,"dictionary_rows":stats.dictionaries,"raw_escape_rows":stats.escapes,"exact_bits_roundtrip":true,"sparse_ns":old,"candidate_ns":new}));
    }
}
