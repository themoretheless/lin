use lin::{Cell,Db,Row};
use std::{sync::Arc,time::Instant,hint::black_box};
type Error=String;fn io_err(s:&str)->Error{s.to_owned()}
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


#[cfg(target_arch="aarch64")]
fn presence_mask8(values:&[f32])->u32 {
    assert_eq!(values.len(),8);
    use std::arch::aarch64::*;
    unsafe {
        let a=vreinterpretq_u32_f32(vld1q_f32(values.as_ptr()));
        let b=vreinterpretq_u32_f32(vld1q_f32(values.as_ptr().add(4)));
        if vmaxvq_u32(vorrq_u32(a,b))==0 {return 0;}
        let weights=vld1q_u32([1u32,2,4,8].as_ptr());
        let zero=vdupq_n_u32(0);
        let lo=vaddvq_u32(vandq_u32(vmvnq_u32(vceqq_u32(a,zero)),weights));
        let hi=vaddvq_u32(vandq_u32(vmvnq_u32(vceqq_u32(b,zero)),weights));
        lo | (hi<<4)
    }
}
#[cfg(not(target_arch="aarch64"))]
fn presence_mask8(values:&[f32])->u32 {values.iter().enumerate().fold(0u32,|m,(i,x)|m | (u32::from(x.to_bits()!=0)<<i))}
fn append_sparse_rows_neon(
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
            let mut mask = if values.len()==8 { presence_mask8(values) } else {
                values.iter().enumerate().fold(0u32,|m,(i,x)| m | (u32::from(x.to_bits()!=0)<<i))
            };
            while mask!=0 {
                let offset=mask.trailing_zeros() as usize;
                let index=(block*8+offset) as u32;
                let mut entry=[0u8;8];entry[..4].copy_from_slice(&index.to_le_bytes());entry[4..].copy_from_slice(&values[offset].to_bits().to_le_bytes());
                buf.extend_from_slice(&entry);count+=1;mask &= mask-1;
            }
        }
        buf[count_offset..count_offset + 4].copy_from_slice(&count.to_le_bytes());
    }
    Ok(())
}

fn fixture(n:usize) -> Vec<Row> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").to_vec()
}


fn encode(rows:&[Option<Arc<[f32]>>],mode:usize)->Vec<u8>{
 let mut out=Vec::with_capacity(rows.len()*(8+768*4)+64);
 if mode==2 {append_sparse_rows_neon(rows,&mut out).unwrap();} else {append_sparse_rows_serial(rows,&mut out).unwrap();}out
}
fn oracle(){
 let mut state=0x0badf00d12345678u64;
 let special=[0u32,0x80000000,0x7f800000,0xff800000,0x7fc01234,0xffc03456,1,0x80000001];
 for offset in 0..4 {for len in 0..=1024 {
  let mut values=Vec::new();for i in 0..len+offset {state^=state<<13;state^=state>>7;state^=state<<17;values.push(f32::from_bits(if i%3==0{state as u32}else{special[i%special.len()]}));}
  if len>=8 {let slice=&values[offset..offset+8];let expected=slice.iter().enumerate().fold(0u32,|m,(i,x)|m | (u32::from(x.to_bits()!=0)<<i));assert_eq!(presence_mask8(slice),expected);}
  let values=Arc::from(&values[offset..]);let rows=vec![None,Some(Arc::from([])),Some(values)];assert_eq!(encode(&rows,0),encode(&rows,2));
 }}
}
fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();oracle();
 for(case,count,cluster)in[("half_spread",383,false),("half_cluster",383,true),("quarter_spread",192,false),("quarter_cluster",192,true)]{
 let mut v=vec![0f32;768];for i in 0..count {v[if cluster{i}else{i*768/count}]=1.0;}
 let rows=vec![Some(Arc::from(v));1000];let expected=encode(&rows,0);let mut ns=vec![Vec::<u128>::new();3];
 for round in 0..24 {for pos in 0..3{let mode=(round+pos+process)%3;let timer=Instant::now();let out=encode(black_box(&rows),mode);ns[mode].push(timer.elapsed().as_nanos());assert_eq!(out,expected);drop(out);}}
 println!("{}",serde_json::json!({"case":case,"ns":ns,"rows":1000,"bytes":expected.len(),"nnz_per_row":count,"exact_bytes":true}));
 }}