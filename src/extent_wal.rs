//! Experimental fixed-extent container for existing checksummed WAL frames.
//! Not wired into Db, migration, snapshots, checkpointing or replication.
use std::{fs::{File, OpenOptions}, io::{Read, Seek, SeekFrom, Write}, path::Path};
use crate::{Error, persist::{durable_sync, for_each_wal_frame_raw, io_err}};
const MAGIC: &[u8;8] = b"LINEXT02";
// A complete legacy checksum-envelope header, with a reserved unsupported codec.
// Older checksummed WAL readers must reject it rather than truncate as JSON tail.
const FRAME_PREFIX:[u8;8]=[b'L',b'I',b'N',6,56,0,0,0];
const DATA_START: u64 = 4096;
const HEADER_BYTES: usize = 64;
const MAX_EXTENT: u64 = 256 * 1024 * 1024;

/// A bounded, fully written extent. Appends always use Full durability.
/// Any write/sync error poisons the handle; reopening must revalidate the file.
#[derive(Debug)]
pub struct ExtentWal { file: File, physical: u64, logical: u64, sequence: u64, poisoned: bool }
fn header(physical:u64, logical:u64, sequence:u64)->[u8;HEADER_BYTES] {
    let mut h=[0;HEADER_BYTES];h[..8].copy_from_slice(&FRAME_PREFIX);h[8]=127;h[9..17].copy_from_slice(MAGIC);
    h[17..25].copy_from_slice(&physical.to_le_bytes());h[25..33].copy_from_slice(&logical.to_le_bytes());h[33..41].copy_from_slice(&sequence.to_le_bytes());
    let crc=crate::crc::hardware_crc32c(&h[..60]);h[60..64].copy_from_slice(&crc.to_le_bytes());h
}
fn validate_frames(frames:&[u8])->Result<(),Error> {
    for_each_wal_frame_raw(frames,|raw,_| {
        if raw[..4]!=*b"LIN\x06" { return Err(io_err("extent requires checksummed WAL frames")); }
        Ok(())
    })?;Ok(())
}
#[cfg(test)]
fn crash_checkpoint(stage: &str) {
    if std::env::var("LIN_EXTENT_TEST_CRASH_STAGE").ok().as_deref() != Some(stage) { return; }
    let ready=std::env::var_os("LIN_EXTENT_TEST_CRASH_READY").expect("crash ready path");
    let mut signal=OpenOptions::new().write(true).create_new(true).open(ready).unwrap();
    signal.write_all(stage.as_bytes()).unwrap();signal.sync_all().unwrap();
    loop { std::thread::park(); }
}
#[cfg(test)]
fn write_with_crash_split(file:&mut File, bytes:&[u8], stage:&str)->Result<(),Error> {
    if std::env::var("LIN_EXTENT_TEST_CRASH_STAGE").ok().as_deref()==Some(stage) {
        let middle=if stage=="mid_header" {29}else{bytes.len()/2};
        file.write_all(&bytes[..middle]).map_err(io_err)?;crash_checkpoint(stage);
        file.write_all(&bytes[middle..]).map_err(io_err)?;
    } else { file.write_all(bytes).map_err(io_err)?; }
    Ok(())
}
impl ExtentWal {
    /// Create a new file; never overwrite an existing database or WAL.
    pub fn create(path:&Path, physical:u64)->Result<Self,Error> {
        if !(DATA_START*2..=MAX_EXTENT).contains(&physical)||physical%4096!=0 {return Err(io_err("invalid extent capacity"));}
        let mut file=OpenOptions::new().read(true).write(true).create_new(true).open(path).map_err(io_err)?;
        file.try_lock().map_err(io_err)?;
        let zeros=[0u8;64*1024];let mut remaining=physical;
        while remaining>0 {let n=remaining.min(zeros.len()as u64)as usize;file.write_all(&zeros[..n]).map_err(io_err)?;remaining-=n as u64;}
        file.seek(SeekFrom::Start(0)).map_err(io_err)?;file.write_all(&header(physical,0,0)).map_err(io_err)?;file.sync_all().map_err(io_err)?;
        Ok(Self{file,physical,logical:0,sequence:0,poisoned:false})
    }
    /// Strictly validate the envelope and every committed frame before use.
    /// Invalid headers do not fall back to an older length or scan the padding.
    pub fn open(path:&Path)->Result<Self,Error> {
        let file=OpenOptions::new().read(true).write(true).open(path).map_err(io_err)?;
        file.try_lock().map_err(io_err)?;
        Self::from_file(file)
    }
    fn from_file(mut file:File)->Result<Self,Error> {
        let length=file.metadata().map_err(io_err)?.len();file.seek(SeekFrom::Start(0)).map_err(io_err)?;let mut h=[0u8;HEADER_BYTES];file.read_exact(&mut h).map_err(io_err)?;
        if h[..8]!=FRAME_PREFIX||h[8]!=127||&h[9..17]!=MAGIC||h[41..60].iter().any(|b|*b!=0)||crate::crc::hardware_crc32c(&h[..60])!=u32::from_le_bytes(h[60..64].try_into().unwrap()) {return Err(io_err("corrupt extent header"));}
        let physical=u64::from_le_bytes(h[17..25].try_into().unwrap());let logical=u64::from_le_bytes(h[25..33].try_into().unwrap());let sequence=u64::from_le_bytes(h[33..41].try_into().unwrap());
        if physical!=length||!(DATA_START*2..=MAX_EXTENT).contains(&physical)||physical%4096!=0||logical>physical-DATA_START {return Err(io_err("invalid extent lengths"));}
        let mut result=Self{file,physical,logical,sequence,poisoned:false};result.read_frames()?;Ok(result)
    }
    pub fn read_frames(&mut self)->Result<Vec<u8>,Error> {
        if self.poisoned {return Err(io_err("extent handle poisoned"));}
        let mut bytes=vec![0;self.logical as usize];self.file.seek(SeekFrom::Start(DATA_START)).map_err(io_err)?;self.file.read_exact(&mut bytes).map_err(io_err)?;validate_frames(&bytes)?;Ok(bytes)
    }
    /// Publish existing frame bytes, then the checksummed boundary, then one sync.
    /// This is not an atomic-write guarantee: interrupted writes may fail recovery.
    pub fn append_frames(&mut self, frames:&[u8])->Result<(),Error> {
        if self.poisoned {return Err(io_err("extent handle poisoned"));}
        validate_frames(frames)?;if frames.is_empty(){return Ok(());}
        let logical=self.logical.checked_add(frames.len()as u64).filter(|n|*n<=self.physical-DATA_START).ok_or_else(||io_err("extent full"))?;
        let sequence=self.sequence.checked_add(1).ok_or_else(||io_err("extent sequence exhausted"))?;
        self.poisoned=true;
        self.file.seek(SeekFrom::Start(DATA_START+self.logical)).map_err(io_err)?;
        #[cfg(test)] write_with_crash_split(&mut self.file,frames,"mid_payload")?;
        #[cfg(not(test))] self.file.write_all(frames).map_err(io_err)?;
        #[cfg(test)] crash_checkpoint("before_header");
        self.file.seek(SeekFrom::Start(0)).map_err(io_err)?;
        let published=header(self.physical,logical,sequence);
        #[cfg(test)] write_with_crash_split(&mut self.file,&published,"mid_header")?;
        #[cfg(not(test))] self.file.write_all(&published).map_err(io_err)?;
        #[cfg(test)] crash_checkpoint("after_header");
        durable_sync(&self.file).map_err(io_err)?;
        #[cfg(test)] crash_checkpoint("after_sync");
        self.logical=logical;self.sequence=sequence;self.poisoned=false;Ok(())
    }
    /// Replay committed records newer than a durable snapshot generation.
    pub fn replay_records(&mut self, min_generation:u64, mut apply:impl FnMut(crate::persist::LogRecord)->Result<(),Error>)->Result<usize,Error> {
        let frames=self.read_frames()?;let mut applied=0;
        crate::persist::for_each_wal_frame(&frames,|record| {
            if record.r#gen>min_generation {apply(record)?;applied+=1;}Ok(())
        })?;Ok(applied)
    }
    /// Publish an empty logical log only after the caller's snapshot is durable.
    /// Existing physical allocation is retained; this does not publish a snapshot.
    pub fn clear_after_durable_snapshot(&mut self)->Result<(),Error> {
        if self.poisoned {return Err(io_err("extent handle poisoned"));}
        let sequence=self.sequence.checked_add(1).ok_or_else(||io_err("extent sequence exhausted"))?;
        self.poisoned=true;self.file.seek(SeekFrom::Start(0)).map_err(io_err)?;
        self.file.write_all(&header(self.physical,0,sequence)).map_err(io_err)?;durable_sync(&self.file).map_err(io_err)?;
        self.logical=0;self.sequence=sequence;self.poisoned=false;Ok(())
    }
    /// Encode a record with the existing codecs and append its exact frame bytes.
    pub fn append_record(&mut self, record:&crate::persist::LogRecord, payload:&mut Vec<u8>, frame:&mut Vec<u8>)->Result<(),Error> {
        crate::persist::encode_extent_frame(record,payload,frame)?;self.append_frames(frame)
    }
    pub(crate) fn flush_for_checkpoint(&self)->Result<(),Error> {
        if self.poisoned {return Err(io_err("extent handle poisoned"));}durable_sync(&self.file).map_err(io_err)
    }
    pub(crate) fn export_since(&self,min_generation:u64)->Result<Vec<u8>,Error> {
        if self.poisoned{return Err(io_err("extent handle poisoned"));}
        let mut reader=Self::from_file(self.file.try_clone().map_err(io_err)?)?;let frames=reader.read_frames()?;let mut out=Vec::new();
        for_each_wal_frame_raw(&frames,|raw,record|{if record.r#gen>min_generation{out.extend_from_slice(raw);}Ok(())})?;Ok(out)
    }
    pub fn logical_bytes(&self)->u64 {self.logical}
    pub fn physical_bytes(&self)->u64 {self.physical}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Row,Cell,persist::{LogRecord,Pack,SyncMode,append_record}};
    pub(super) struct Temp(std::path::PathBuf);
    impl Temp {
        pub(super) fn new()->Self {static SEQUENCE:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);let nonce=SEQUENCE.fetch_add(1,std::sync::atomic::Ordering::Relaxed);let path=std::env::temp_dir().join(format!("lin-extent-test-{}-{}-{}",std::process::id(),nonce,std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir(&path).unwrap();Self(path)}
        pub(super) fn path(&self,name:&str)->std::path::PathBuf{self.0.join(name)}
        pub(super) fn frame(&self)->Vec<u8>{
            let path=self.path("encoded-frame");let mut file=OpenOptions::new().create_new(true).read(true).write(true).open(&path).unwrap();
            let rec=LogRecord{r#gen:1,next_id:2,pack:Pack::InsertBulk{collection:"docs".into(),rows:vec![Row::from([("id".into(),Cell::from("one")),("body".into(),Cell::from("complete durable payload"))])],edges:vec![]}};
            append_record(&mut file,&rec,&mut Vec::new(),SyncMode::Normal,&mut 0).unwrap();drop(file);std::fs::read(path).unwrap()
        }
    }
    impl Drop for Temp {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
    #[test]
    fn codec_byte_parity_replay_snapshot_boundary_and_clear_reuse(){
        let tmp=Temp::new();let path=tmp.path("extent");let rows=vec![Row::from([("id".into(),Cell::from("one")),("count".into(),Cell::Int(7))])];
        let packs=vec![Pack::InsertBulk{collection:"docs".into(),rows:rows.clone(),edges:vec![]},Pack::AppendFactsBulk{s:vec!["one".into()],p:vec!["rel".into()],o:vec!["two".into()]},crate::persist::rows_to_insert_cols("docs",&rows,vec![])];
        let mut wal=ExtentWal::create(&path,8192).unwrap();let mut payload=Vec::new();let mut frame=Vec::new();let mut expected=Vec::new();
        for (index,pack) in packs.into_iter().enumerate(){
            let record=LogRecord{r#gen:index as u64+1,next_id:8,pack};let file_path=tmp.path(&format!("legacy-{index}"));
            let mut file=OpenOptions::new().create_new(true).write(true).open(&file_path).unwrap();append_record(&mut file,&record,&mut Vec::new(),SyncMode::Normal,&mut 0).unwrap();drop(file);
            wal.append_record(&record,&mut payload,&mut frame).unwrap();let legacy=std::fs::read(file_path).unwrap();assert_eq!(frame,legacy);expected.extend_from_slice(&legacy);
        }
        assert_eq!(wal.read_frames().unwrap(),expected);let mut generations=Vec::new();assert_eq!(wal.replay_records(1,|r|{generations.push(r.r#gen);Ok(())}).unwrap(),2);assert_eq!(generations,vec![2,3]);
        wal.clear_after_durable_snapshot().unwrap();assert_eq!(wal.logical_bytes(),0);assert_eq!(wal.physical_bytes(),8192);assert!(wal.read_frames().unwrap().is_empty());drop(wal);
        let mut wal=ExtentWal::open(&path).unwrap();assert!(wal.read_frames().unwrap().is_empty());wal.append_frames(&frame).unwrap();assert_eq!(wal.read_frames().unwrap(),frame);assert_eq!(std::fs::metadata(path).unwrap().len(),8192);
    }
    #[test]
    fn exact_existing_frames_survive_reopen_and_repeated_append(){
        let tmp=Temp::new();let path=tmp.path("extent");let frame=tmp.frame();let mut wal=ExtentWal::create(&path,8192).unwrap();
        assert_eq!(wal.physical_bytes(),8192);assert_eq!(wal.logical_bytes(),0);
        wal.append_frames(&frame).unwrap();wal.append_frames(&frame).unwrap();drop(wal);
        let mut reopened=ExtentWal::open(&path).unwrap();assert_eq!(reopened.logical_bytes(),(2*frame.len())as u64);
        assert_eq!(reopened.read_frames().unwrap(),[frame.clone(),frame].concat());assert_eq!(std::fs::metadata(path).unwrap().len(),8192);
    }
    #[test]
    fn every_header_and_committed_frame_byte_corruption_is_rejected(){
        let tmp=Temp::new();let path=tmp.path("extent");let frame=tmp.frame();let mut wal=ExtentWal::create(&path,8192).unwrap();wal.append_frames(&frame).unwrap();drop(wal);
        let original=std::fs::read(&path).unwrap();
        for offset in (0..HEADER_BYTES).chain(DATA_START as usize..DATA_START as usize+frame.len()){
            let mut bytes=original.clone();bytes[offset]^=1;std::fs::write(&path,bytes).unwrap();assert!(ExtentWal::open(&path).is_err(),"accepted corruption at {offset}");
        }
        std::fs::write(&path,&original[..original.len()-1]).unwrap();assert!(ExtentWal::open(&path).is_err());
        let mut extra=original.clone();extra.push(0);std::fs::write(&path,extra).unwrap();assert!(ExtentWal::open(&path).is_err());
        let mut cut=original;cut[..HEADER_BYTES].copy_from_slice(&header(8192,frame.len()as u64-1,1));std::fs::write(&path,cut).unwrap();assert!(ExtentWal::open(&path).is_err());
    }
    #[test]
    fn invalid_frames_and_capacity_errors_leave_published_bytes_unchanged(){
        let tmp=Temp::new();let path=tmp.path("extent");let frame=tmp.frame();let mut wal=ExtentWal::create(&path,8192).unwrap();
        let original=std::fs::read(&path).unwrap();assert!(wal.append_frames(&frame[..frame.len()-1]).is_err());assert_eq!(std::fs::read(&path).unwrap(),original);
        let many=frame.repeat(8192/frame.len()+1);assert!(wal.append_frames(&many).is_err());assert_eq!(wal.logical_bytes(),0);assert_eq!(std::fs::read(&path).unwrap(),original);
        assert!(ExtentWal::create(&path,8192).is_err());assert_eq!(std::fs::read(path).unwrap(),original);
        assert!(ExtentWal::create(&tmp.path("invalid-capacity"),8193).is_err());assert!(!tmp.path("invalid-capacity").exists());
    }
    #[test]
    fn unpublished_tail_is_excluded_but_published_torn_frame_is_rejected(){
        let tmp=Temp::new();let path=tmp.path("extent");let frame=tmp.frame();drop(ExtentWal::create(&path,8192).unwrap());
        let mut file=OpenOptions::new().write(true).open(&path).unwrap();file.seek(SeekFrom::Start(DATA_START)).unwrap();file.write_all(&frame[..frame.len()/2]).unwrap();file.sync_all().unwrap();drop(file);
        assert!(ExtentWal::open(&path).unwrap().read_frames().unwrap().is_empty());
        let mut file=OpenOptions::new().write(true).open(&path).unwrap();file.write_all(&header(8192,frame.len()as u64,1)).unwrap();file.sync_all().unwrap();drop(file);
        assert!(ExtentWal::open(&path).is_err());
    }
    #[test]
    fn every_interrupted_header_prefix_has_only_old_new_or_error_outcomes(){
        let tmp=Temp::new();let path=tmp.path("extent");let frame=tmp.frame();drop(ExtentWal::create(&path,8192).unwrap());
        let original=std::fs::read(&path).unwrap();let old=header(8192,0,0);let new=header(8192,frame.len()as u64,1);
        for n in 0..=HEADER_BYTES {
            let mut bytes=original.clone();bytes[DATA_START as usize..DATA_START as usize+frame.len()].copy_from_slice(&frame);bytes[..n].copy_from_slice(&new[..n]);
            let valid_old=bytes[..HEADER_BYTES]==old;let valid_new=bytes[..HEADER_BYTES]==new;std::fs::write(&path,bytes).unwrap();
            match ExtentWal::open(&path){Ok(mut wal)=>{assert!(valid_old||valid_new,"accepted torn header prefix {n}");assert_eq!(wal.read_frames().unwrap(),if valid_new{frame.clone()}else{Vec::new()});},Err(_)=>assert!(!valid_old&&!valid_new)}
        }
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[test]
    fn competing_writer_is_rejected_and_failed_write_poisons_handle() {
        let path=std::env::temp_dir().join(format!("lin-extent-owner-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let mut wal=ExtentWal::create(&path,8192).unwrap();
        assert!(ExtentWal::open(&path).is_err());
        let rec=crate::persist::LogRecord{r#gen:1,next_id:1,pack:crate::persist::Pack::InsertBulk{collection:"docs".into(),rows:vec![],edges:vec![]}};
        let mut payload=Vec::new();rmp_serde::encode::write_named(&mut payload,&rec).unwrap();
        let mut checked=vec![1];checked.extend_from_slice(&payload);let crc=crate::crc::hardware_crc32c(&checked);checked.extend_from_slice(&crc.to_le_bytes());
        let mut frame=b"LIN\x06".to_vec();frame.extend_from_slice(&(checked.len()as u32).to_le_bytes());frame.extend_from_slice(&checked);
        // Inject a real write error with a read-only replacement descriptor.
        wal.file=OpenOptions::new().read(true).open(&path).unwrap();
        assert!(wal.append_frames(&frame).is_err());assert!(wal.poisoned);
        assert!(wal.append_frames(&[]).is_err());assert!(wal.read_frames().is_err());assert!(wal.export_since(0).is_err());drop(wal);
        let mut clean=ExtentWal::open(&path).unwrap();assert_eq!(clean.logical_bytes(),0);assert!(clean.read_frames().unwrap().is_empty());drop(clean);
        let bytes=std::fs::read(&path).unwrap();assert!(crate::persist::for_each_wal_frame(&bytes,|_|Ok(())).is_err());
        std::fs::remove_file(path).unwrap();
    }
}

#[cfg(all(test,unix))]
mod crash_tests {
    use super::*;
    use std::process::{Command,Child,Stdio};
    use std::os::unix::process::ExitStatusExt;
    struct ChildGuard(Child);
    impl Drop for ChildGuard {fn drop(&mut self){let _=self.0.kill();let _=self.0.wait();}}
    #[test]
    #[ignore = "child helper invoked by sigkill_recovery_at_each_publication_stage"]
    fn child() {
        let path=std::env::var_os("LIN_EXTENT_TEST_CRASH_PATH").expect("child path");
        let frames=std::fs::read(std::env::var_os("LIN_EXTENT_TEST_FRAME_PATH").expect("frame path")).unwrap();
        let mut wal=ExtentWal::open(Path::new(&path)).unwrap();wal.append_frames(&frames).unwrap();
        panic!("child failed to stop at crash checkpoint");
    }
    #[test]
    fn sigkill_recovery_at_each_publication_stage() {
        for stage in ["mid_payload","before_header","mid_header","after_header","after_sync"] {
            let tmp=super::tests::Temp::new();let path=tmp.path("extent");let frame=tmp.frame();let frame_path=tmp.path("frame.raw");std::fs::write(&frame_path,&frame).unwrap();
            let mut wal=ExtentWal::create(&path,8192).unwrap();wal.append_frames(&frame).unwrap();drop(wal);
            let ready=tmp.path("ready");
            let stderr=OpenOptions::new().write(true).create_new(true).open(tmp.path("child.stderr")).unwrap();
            let mut child=ChildGuard(Command::new(std::env::current_exe().unwrap())
                .args(["--exact","extent_wal::crash_tests::child","--ignored","--nocapture"])
                .env("LIN_EXTENT_TEST_CRASH_PATH",&path).env("LIN_EXTENT_TEST_FRAME_PATH",&frame_path)
                .env("LIN_EXTENT_TEST_CRASH_STAGE",stage).env("LIN_EXTENT_TEST_CRASH_READY",&ready)
                .stdout(Stdio::null()).stderr(stderr).spawn().unwrap());
            let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
            loop {
                if std::fs::read_to_string(&ready).ok().as_deref()==Some(stage){break;}
                assert!(child.0.try_wait().unwrap().is_none(),"child exited before {stage}");
                assert!(std::time::Instant::now()<deadline,"checkpoint timeout {stage}");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            child.0.kill().unwrap();let status=child.0.wait().unwrap();assert_eq!(status.signal(),Some(9));
            assert_eq!(std::fs::read_to_string(&ready).unwrap(),stage);
            if stage=="mid_header" {assert!(ExtentWal::open(&path).is_err(),"accepted torn published header");}
            else {
                let mut reopened=ExtentWal::open(&path).unwrap();
                let copies=if ["mid_payload","before_header"].contains(&stage){1}else{2};
                assert_eq!(reopened.read_frames().unwrap(),frame.repeat(copies),"recovery after {stage}");
            }
            println!("SIGKILL stage={stage}: expected strict recovery outcome verified");
        }
    }
}
