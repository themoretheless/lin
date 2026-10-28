use lin::{Cell, Db, Row, Store, SyncMode};
use lin::persist::{LogRecord, Pack, append_record, for_each_wal_frame};
use std::{collections::BTreeMap, fs::{self, File, OpenOptions}, io::{self, Seek, Write}, path::PathBuf, time::{Instant, SystemTime, UNIX_EPOCH}};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Default, serde::Serialize)]
struct Calls { write_all: usize, sync_data: usize, pwritev2: usize, interrupted: usize, partial: usize }
fn write_exact(mut data: &[u8], calls: &mut Calls, mut call: impl FnMut(&[u8])->io::Result<usize>) -> io::Result<()> {
    while !data.is_empty() {
        calls.pwritev2 += 1;
        match call(data) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) if n > data.len() => return Err(io::ErrorKind::InvalidData.into()),
            Ok(n) => { if n < data.len() { calls.partial += 1; } data = &data[n..]; }
            Err(e) if e.kind()==io::ErrorKind::Interrupted => { calls.interrupted += 1; }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
#[cfg(all(target_os="linux", target_env="gnu"))]
fn dsync_write(file: &File, data: &[u8], calls: &mut Calls) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    write_exact(data, calls, |remaining| {
        let vector=libc::iovec { iov_base:remaining.as_ptr() as *mut libc::c_void, iov_len:remaining.len() };
        // The kernel reads this live immutable slice during the call. A -1 offset
        // uses and advances the file position. RWF_DSYNC covers only this range.
        let n=unsafe { libc::pwritev2(file.as_raw_fd(), &vector, 1, -1 as libc::off_t, libc::RWF_DSYNC) };
        if n<0 { Err(io::Error::last_os_error()) } else { Ok(n as usize) }
    })
}
#[cfg(not(all(target_os="linux", target_env="gnu")))]
fn dsync_write(_: &File, _: &[u8], _: &mut Calls) -> io::Result<()> { Err(io::ErrorKind::Unsupported.into()) }
fn full_sync(file: &File)->io::Result<()> {
    #[cfg(target_os="linux")] {
        use std::os::fd::AsRawFd;
        if unsafe { libc::fdatasync(file.as_raw_fd()) }==0 { Ok(()) } else { Err(io::Error::last_os_error()) }
    }
    #[cfg(not(target_os="linux"))] { file.sync_data() }
}
struct OwnedDir(PathBuf);
impl Drop for OwnedDir { fn drop(&mut self) { let _=fs::remove_dir_all(&self.0); } }
fn encode(file: &mut File, record: LogRecord, buffer: &mut Vec<u8>, bytes: &mut u64)->Result<()> {
    append_record(file,&record,buffer,SyncMode::Normal,bytes)?; Ok(())
}
fn run()->Result<()> {
    let args:Vec<String>=std::env::args().collect();
    let arg=|name:&str|->Result<&str>{let i=args.iter().position(|a|a==name).ok_or_else(||io::Error::new(io::ErrorKind::InvalidInput,name.to_owned()))?;Ok(args.get(i+1).ok_or(io::ErrorKind::InvalidInput)?.as_str())};
    let mode=arg("--mode")?;assert!(["baseline","control","candidate"].contains(&mode));
    let shape=arg("--shape")?;assert!(["scalar","doc_update","doc_delete","doc_batch"].contains(&shape));
    let sync=arg("--sync")?;assert!(["full","normal"].contains(&sync));assert!(sync=="full"||["scalar","doc_update"].contains(&shape));
    let commits:usize=arg("--commits")?.parse()?;assert!([1,32].contains(&commits));
    let smoke=args.iter().any(|a|a=="--smoke");
    if !smoke { assert!(cfg!(all(target_os="linux",target_env="gnu")),"Linux GNU timing only"); }
    let samples=if smoke {1}else{8};
    let stamp=SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir=OwnedDir(std::env::temp_dir().join(format!("lin-rwf-probe-{}-{stamp}",std::process::id())));fs::create_dir(&dir.0)?;File::open(std::env::temp_dir())?.sync_all()?;
    let mut fixture=Db::empty();
    let collection=if shape=="scalar" {"wal_scalar_control"}else{"docs"};
    if shape=="scalar" { fixture.run("col wal_scalar_control {id:text,value:i64}")?; }
    let rows=(0..32).map(|i|if shape=="scalar" {format!(r#"{{id:"s-{i}",value:{i}}}"#)}else{format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"doc {i} plain",body:"body {i}",layer:"wiki",wing:"rag",ts:timestamp({})}}"#,1700000000000i64+i)}).collect::<Vec<_>>().join(",");
    fixture.run(&format!("insert {collection} [{rows}]"))?;
    let mut initial=fixture.store.collection(collection).to_vec();assert_eq!(initial.len(),32);
    if shape!="scalar" {
        for row in &mut initial {
            let dims=row.get("embedding").and_then(Cell::as_vec).unwrap().len();assert!(dims>0);
            row.insert("embedding".into(),Cell::Vec((0..dims).map(|i|if i%2==0 {0.25f32}else{-0.0f32}).collect::<Vec<_>>().into()));
        }
    }
    let mut targets=Vec::new();let mut frames=Vec::new();let mut buffer=Vec::new();let mut total=0u64;let mut ends=Vec::new();
    let encoded_path=dir.0.join("encode");let mut encoded=OpenOptions::new().read(true).write(true).create_new(true).open(&encoded_path)?;
    encode(&mut encoded,LogRecord{r#gen:1,next_id:1,pack:Pack::InsertBulk{collection:collection.into(),rows:initial.clone(),edges:Vec::new()}},&mut buffer,&mut total)?;ends.push(total as usize);
    for step in 0..32 {
        let mut row=initial[(20+step)%32].clone();
        if shape=="scalar" {row.insert("value".into(),Cell::Int(7));}
        else if shape!="doc_delete" {row.insert("wing".into(),Cell::from_text("sys"));}
        let mut pack=if shape=="doc_delete" {Pack::Delete{collection:collection.into(),rows:vec![row.clone()]}}else{Pack::Update{collection:collection.into(),rows:vec![row.clone()]}};
        if shape=="doc_batch" {pack=Pack::Batch{packs:vec![pack]};}
        targets.push(row);encode(&mut encoded,LogRecord{r#gen:step as u64+2,next_id:1,pack},&mut buffer,&mut total)?;ends.push(total as usize);
    }
    assert_eq!(encoded.metadata()?.len(),total);drop(encoded);
    let golden=fs::read(&encoded_path)?;fs::remove_file(encoded_path)?;
    let prefix=&golden[..ends[0]];
    for i in 0..32 {frames.push(golden[ends[i]..ends[i+1]].to_vec());}
    for sample in 0..samples {
        let path=dir.0.join(format!("sample-{sample}"));let mut file=OpenOptions::new().read(true).write(true).create_new(true).open(&path)?;
        // File-name metadata and the complete prior WAL range are durable before
        // timing. This precondition is stronger than an arbitrary dirty prefix.
        File::open(&dir.0)?.sync_all()?;file.write_all(prefix)?;full_sync(&file)?;
        let mut calls=Calls::default();let started=Instant::now();
        for frame in frames.iter().take(commits) {
            if mode=="candidate"&&sync=="full" { dsync_write(&file,frame,&mut calls)?; }
            else {calls.write_all+=1;file.write_all(frame)?;if sync=="full" {calls.sync_data+=1;full_sync(&file)?;}}
        }
        let elapsed=started.elapsed().as_nanos() as f64;
        let expected_len=prefix.len()+frames.iter().take(commits).map(Vec::len).sum::<usize>();
        assert_eq!(file.stream_position()?,expected_len as u64);assert_eq!(file.metadata()?.len(),expected_len as u64);drop(file);
        let actual=fs::read(&path)?;assert_eq!(&actual[..prefix.len()],prefix);
        let expected_bytes=[prefix,frames.iter().take(commits).flat_map(|v|v.iter()).copied().collect::<Vec<_>>().as_slice()].concat();assert_eq!(actual,expected_bytes);
        let mut store=Store::empty("rwf-probe");let mut generation=0;
        let count=for_each_wal_frame(&actual,|record|{assert_eq!(record.r#gen,generation+1);assert_eq!(record.next_id,1);generation=record.r#gen;store.apply_pack(&record.pack);Ok(())})?;
        assert_eq!(count,commits+1);assert_eq!(generation,commits as u64+1);
        let mut model:BTreeMap<String,Row>=initial.iter().map(|r|(r["id"].text().unwrap().to_owned(),r.clone())).collect();
        for row in targets.iter().take(commits) {let id=row["id"].text().unwrap();if shape=="doc_delete" {assert!(model.remove(id).is_some());}else{model.insert(id.to_owned(),row.clone());}}
        let got:BTreeMap<String,Row>=store.collection(collection).iter().map(|r|(r["id"].text().unwrap().to_owned(),r.clone())).collect();assert_eq!(got,model);assert_eq!(rmp_serde::to_vec_named(&got)?,rmp_serde::to_vec_named(&model)?);
        let mut corrupt=actual.clone();*corrupt.last_mut().unwrap()^=1;assert!(for_each_wal_frame(&corrupt,|_|Ok(())).is_err());
        fs::remove_file(&path)?;
        println!("{}",serde_json::json!({"mode":mode,"shape":shape,"sync":sync,"commits":commits,"sample":sample,"correctness_only":smoke,"ns_per_commit":if smoke {None}else{Some(elapsed/commits as f64)},"prefix_bytes":prefix.len(),"prefix_crc32_ieee":crc32fast::hash(prefix),"target_bytes":expected_len-prefix.len(),"stream_crc32_ieee":crc32fast::hash(&actual),"frame_bytes":frames.iter().take(commits).map(Vec::len).collect::<Vec<_>>(),"calls":calls,"exact_bytes":true,"complete_replay_model":true,"generation_verified":true,"replay_canonical_bits_verified":true,"corruption_rejected":true,"prefix_synced_before_timing":true,"logical_rows":got.len()}));
    }
    fs::remove_dir(&dir.0)?;Ok(())
}
fn main()->Result<()> {run()}
#[cfg(test)] mod tests {
    use super::*;
    #[test]fn partial_and_interrupted_writes_keep_exact_bytes(){let mut calls=Calls::default();let mut out=Vec::new();let mut first=true;write_exact(b"abcdefg",&mut calls,|data|{if first {first=false;return Err(io::ErrorKind::Interrupted.into());}let n=data.len().min(2);out.extend_from_slice(&data[..n]);Ok(n)}).unwrap();assert_eq!(out,b"abcdefg");assert_eq!(calls.interrupted,1);assert_eq!(calls.partial,3);}
    #[test]fn zero_write_is_an_error(){let error=write_exact(b"a",&mut Calls::default(),|_|Ok(0)).unwrap_err();assert_eq!(error.kind(),io::ErrorKind::WriteZero);}
    #[test]fn hard_error_after_partial_never_reports_success(){let mut out=Vec::new();let mut first=true;let result=write_exact(b"abcd",&mut Calls::default(),|data|{if first {first=false;out.push(data[0]);Ok(1)}else{Err(io::ErrorKind::PermissionDenied.into())}});assert!(result.is_err());assert_eq!(out,b"a");}
    #[test]fn impossible_kernel_count_is_rejected(){let error=write_exact(b"a",&mut Calls::default(),|_|Ok(2)).unwrap_err();assert_eq!(error.kind(),io::ErrorKind::InvalidData);}
}
