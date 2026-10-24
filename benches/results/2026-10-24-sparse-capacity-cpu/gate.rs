use lin::{Cell,Db};
use std::{sync::Arc,hint::black_box,time::Instant};
type Error=String;
fn io_err(s:&str)->Error{s.to_owned()}
fn append_sparse_rows_serial(
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

fn encode_pointer(
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
            // At most values.len() eight-byte entries; reserve before taking pointer.
            buf.reserve(values.len() * 8);
            let start = buf.len();
            let mut written = 0usize;
            let out = buf.as_mut_ptr();
            for (offset, value) in values.iter().enumerate() {
                let bits = value.to_bits();
                if bits != 0 {
                    let index = (block * 8 + offset) as u32;
                    let entry = ((bits as u64) << 32) | index as u64;
                    // All writes fit reserved spare capacity; no allocation until set_len.
                    unsafe { std::ptr::write_unaligned(out.add(start + written) as *mut u64, entry.to_le()); }
                    written += 8;
                    count += 1;
                }
            }
            // Exactly written bytes were initialized above, including empty blocks.
            unsafe { buf.set_len(start + written); }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
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


fn main(){
 let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 let special=vec![None,Some(Arc::from([])),Some(Arc::from([0.0;35])),Some(Arc::from([-0.0,f32::from_bits(0x7fc00001),f32::from_bits(0x7fa00002),f32::INFINITY,f32::NEG_INFINITY,-1.25]))];
 for (name,rows,loops) in [("default_1k",fixture(1000),32),("default_10k",fixture(10000),4),("dense",(0..1000).map(|_|Some(vec![1.25;768].into())).collect(),4),("boundaries",special,256)] {
  let funcs:[fn(&[Option<Arc<[f32]>>],&mut Vec<u8>)->Result<(),Error>;2]=[append_sparse_rows_serial,encode_pointer];
  let mut expected=Vec::new();append_sparse_rows_serial(&rows,&mut expected).unwrap();
  for f in funcs {let mut got=vec![12,34,56];f(&rows,&mut got).unwrap();assert_eq!(&got[3..],expected);}
  let mut buffers:Vec<Vec<u8>>=(0..2).map(|_|Vec::with_capacity(expected.len())).collect();let mut times=vec![Vec::<f64>::new();2];
  for round in 0..12 {for pos in 0..2 {let v=(pos+round+process)%2;let buf=&mut buffers[v];let t=Instant::now();for _ in 0..loops {buf.clear();funcs[v](black_box(&rows),buf).unwrap();black_box(buf.as_slice());}times[v].push(t.elapsed().as_nanos() as f64/loops as f64);}}
  println!("{}",serde_json::json!({"case":name,"bytes":expected.len(),"variants":["current","spare_capacity"],"ns":times,"loops":loops,"exact_bytes":true}));
 }
}
