use lin::{Cell,Db,Row};
use std::{sync::Arc,time::Instant,hint::black_box,io};
type Error=String;fn io_err(s:&str)->Error{s.to_owned()}
const SPARSE_VECTOR_THRESHOLD:usize=64*1024;
fn use_sparse_vectors(rows: &[Option<std::sync::Arc<[f32]>>]) -> bool {
    let dense = rows
        .iter()
        .map(|row| 4usize.saturating_add(row.as_ref().map_or(0, |v| v.len().saturating_mul(4))))
        .fold(0usize, usize::saturating_add);
    if rows
        .iter()
        .any(|row| row.as_ref().is_some_and(|v| v.is_empty()))
    {
        return true; // Preserve Some(empty), which the legacy vector tag cannot express.
    }
    if dense < SPARSE_VECTOR_THRESHOLD {
        return false;
    }
    if rows.len() >= 4096 && dense >= 8 * 1024 * 1024 {
        return sparse_bytes_parallel(rows) < dense;
    }
    let sparse = rows
        .iter()
        .map(|row| {
            row.as_ref().map_or(4, |v| {
                8usize.saturating_add(
                    v.iter()
                        .filter(|value| value.to_bits() != 0)
                        .count()
                        .saturating_mul(8),
                )
            })
        })
        .fold(0usize, usize::saturating_add);
    sparse < dense
}

fn sparse_bytes_serial(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
    rows.iter()
        .map(|row| {
            row.as_ref().map_or(4, |v| {
                8usize.saturating_add(
                    v.iter()
                        .filter(|value| value.to_bits() != 0)
                        .count()
                        .saturating_mul(8),
                )
            })
        })
        .fold(0usize, usize::saturating_add)
}

fn sparse_count_worker(
    rows: &[Option<std::sync::Arc<[f32]>>],
    worker: io::Result<std::thread::ScopedJoinHandle<'_, usize>>,
) -> usize {
    match worker {
        Ok(worker) => worker.join().expect("sparse size worker panicked"),
        Err(_) => sparse_bytes_serial(rows),
    }
}

fn sparse_bytes_parallel(rows: &[Option<std::sync::Arc<[f32]>>]) -> usize {
    std::thread::scope(|scope| {
        let chunk_len = rows.len().div_ceil(4);
        let (left, rest) = rows.split_at(chunk_len);
        let mut workers = Vec::with_capacity(3);
        for right in rest.chunks(chunk_len.max(1)) {
            let worker =
                std::thread::Builder::new().spawn_scoped(scope, move || sparse_bytes_serial(right));
            workers.push((right, worker));
        }
        let mut size = sparse_bytes_serial(left);
        for (right, worker) in workers {
            size = size.saturating_add(sparse_count_worker(right, worker));
        }
        size
    })
}

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

fn append_sparse_worker(
    buf: &mut Vec<u8>,
    right: &[Option<std::sync::Arc<[f32]>>],
    worker: io::Result<std::thread::ScopedJoinHandle<'_, Result<Vec<u8>, Error>>>,
) -> Result<(), Error> {
    match worker {
        Ok(worker) => buf.extend_from_slice(&worker.join().expect("sparse WAL worker panicked")?),
        Err(_) => append_sparse_rows_serial(right, buf)?,
    }
    Ok(())
}

fn append_sparse_rows_parallel(
    rows: &[Option<std::sync::Arc<[f32]>>],
    buf: &mut Vec<u8>,
) -> Result<(), Error> {
    std::thread::scope(|scope| {
        let chunk_len = rows.len().div_ceil(4);
        let (left, rest) = rows.split_at(chunk_len);
        let mut workers = Vec::with_capacity(3);
        for right in rest.chunks(chunk_len.max(1)) {
            let worker = std::thread::Builder::new().spawn_scoped(scope, move || {
                let mut bytes = Vec::with_capacity(right.len() * 32);
                append_sparse_rows_serial(right, &mut bytes)?;
                Ok(bytes)
            });
            workers.push((right, worker));
        }
        append_sparse_rows_serial(left, buf)?;
        for (right, worker) in workers {
            append_sparse_worker(buf, right, worker)?;
        }
        Ok(())
    })
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

fn encode(vectors:&[Option<Arc<[f32]>>])->Vec<u8>{
 assert!(use_sparse_vectors(vectors));let mut out=Vec::with_capacity(vectors.len()*(8+768*4)+64);
 if vectors.len()>=4096 && vectors.iter().flatten().map(|v|v.len()).sum::<usize>()>=2*1024*1024{append_sparse_rows_parallel(vectors,&mut out).unwrap();}else{append_sparse_rows_serial(vectors,&mut out).unwrap();}out
}
fn maintain(db:&mut Db,rows:&[Row]){
 db.store.collection_mut("docs").reserve(rows.len());db.store.row_maps_reserve("docs",rows.len());
 db.store.index_insert_slab("docs",0,rows).unwrap();db.store.fts_insert_slab("docs",0,rows);db.store.hnsw_insert_slab("docs",0,rows);db.store.row_maps_register_slab("docs",0,rows);
}
fn execute(db:&mut Db,rows:&[Row],vectors:&[Option<Arc<[f32]>>],parallel:bool)->(Vec<u8>,bool){
 if !parallel || rows.len()<128 {maintain(db,rows);return(encode(vectors),false);}
 std::thread::scope(|scope|match std::thread::Builder::new().spawn_scoped(scope,||encode(vectors)){
  Ok(worker)=>{maintain(db,rows);(worker.join().unwrap(),true)},
  Err(_)=>{maintain(db,rows);(encode(vectors),false)}
 })
}
fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 for(name,n)in[("default_1k",1000),("default_10k",10000),("small_127",127)]{
 let rows=fixture(n);let vectors:Vec<Option<Arc<[f32]>>>=rows.iter().map(|r|match r.get("embedding"){Some(Cell::Vec(v))=>Some(v.clone()),_=>None}).collect();let expected=encode(&vectors);let mut ns=vec![Vec::<u128>::new();2];let mut spawned=vec![0usize;2];
 for round in 0..24{for pos in 0..2{let v=(pos+round+process)%2;let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();
 let t=Instant::now();let(bytes,used_worker)=execute(&mut db,black_box(&rows),black_box(&vectors),v==1);ns[v].push(t.elapsed().as_nanos());spawned[v]+=usize::from(used_worker);assert_eq!(bytes,expected);drop(bytes);
 // Publish rows after both tasks finish, as the normal append path does.
 db.store.collection_mut("docs").extend(rows.iter().cloned());assert_eq!(db.run("docs | take all").unwrap().rows,rows);
 let hits=db.run(r#"docs | search lex "wal" | take all"#).unwrap().rows;assert_eq!(hits.len(),n.div_ceil(10));
 for i in 0..n{assert_eq!(db.store.row_index("docs",&format!("d-{i}")),Some(i));}
 let selected=db.run(r#"docs | wing == "rag" | take all"#).unwrap().rows;assert_eq!(selected.len(),n.div_ceil(2));
 }}assert_eq!(spawned[0],0);assert_eq!(spawned[1],if n>=128{24}else{0});
 println!("{}",serde_json::json!({"case":name,"rows":n,"ns":ns,"spawned":spawned,"vector_bytes":expected.len(),"exact_state_and_bytes":true}));
 }}
