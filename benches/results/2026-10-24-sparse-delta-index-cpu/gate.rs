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
fn delta(
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
        let mut previous_plus_one=0u32;
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
                    let plus_one=index+1;
                    let mut gap=plus_one-previous_plus_one;previous_plus_one=plus_one;
                    while gap>=128 {buf.push((gap as u8 &127)|128);gap>>=7;}
                    buf.push(gap as u8);buf.extend_from_slice(&bits.to_le_bytes());
                    count += 1;
                }
            }
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
fn restore_legacy(bytes:&[u8],rows:usize)->Vec<u8>{
 let mut at=0usize;let mut out=Vec::new();
 fn word(bytes:&[u8],at:&mut usize)->u32{let x=u32::from_le_bytes(bytes[*at..*at+4].try_into().unwrap());*at+=4;x}
 for _ in 0..rows{let dim=word(bytes,&mut at);out.extend_from_slice(&dim.to_le_bytes());if dim==0{continue;}let count=word(bytes,&mut at);out.extend_from_slice(&count.to_le_bytes());let mut previous=0u32;
 for _ in 0..count {let mut value=0u32;let mut shift=0;loop{assert!(shift<=28);let b=bytes[at];at+=1;if shift==28{assert!(b<=15);}value|=((b&127)as u32)<<shift;if b&128==0{break;}shift+=7;}
 assert!(value>0);previous=previous.checked_add(value).unwrap();let index=previous-1;assert!(index<dim-1);out.extend_from_slice(&index.to_le_bytes());let bits=word(bytes,&mut at);out.extend_from_slice(&bits.to_le_bytes());}
 }assert_eq!(at,bytes.len());out
}
fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 let special=vec![None,Some(Arc::from([])),Some(Arc::from([0.0;35])),Some(Arc::from([-0.0,f32::from_bits(0x7fc00001),f32::INFINITY,f32::NEG_INFINITY,-1.25])),Some({let mut x=vec![0.0;65539];for i in [0,127,128,16383,16384,65538]{x[i]=f32::from_bits(0x7fa00001);}Arc::from(x)})];
 for(name,rows,loops)in[("default_1k",fixture(1000),32),("default_10k",fixture(10000),4),("dense",(0..1000).map(|_|Some(vec![1.25;768].into())).collect(),4),("boundaries",special,32)]{
 let mut reference=Vec::new();append_sparse_rows_serial(&rows,&mut reference).unwrap();let mut proposed=vec![1,2,3];delta(&rows,&mut proposed).unwrap();assert_eq!(restore_legacy(&proposed[3..],rows.len()),reference);let sizes=[reference.len(),proposed.len()-3];
 let funcs:[fn(&[Option<Arc<[f32]>>],&mut Vec<u8>)->Result<(),Error>;2]=[append_sparse_rows_serial,delta];let mut buffers=[Vec::with_capacity(sizes[0]),Vec::with_capacity(sizes[1])];let mut ns=vec![Vec::<f64>::new();2];
 for round in 0..24{for pos in 0..2{let v=(pos+round+process)%2;let t=Instant::now();for _ in 0..loops{buffers[v].clear();funcs[v](black_box(&rows),&mut buffers[v]).unwrap();black_box(buffers[v].as_slice());}ns[v].push(t.elapsed().as_nanos()as f64/loops as f64);}}
 println!("{}",serde_json::json!({"case":name,"ns":ns,"bytes":sizes,"exact_bits":true}));
 }}
