//! Experimental fixed-extent container for existing checksummed WAL frames.
//! Not wired into Db, migration, snapshots, checkpointing or replication.
use std::{fs::{File, OpenOptions}, io::{Read, Seek, SeekFrom, Write}, path::Path};
use crate::{Error, persist::{durable_sync, for_each_wal_frame_raw, io_err}};
const MAGIC: &[u8;8] = b"LINEXT01";
const DATA_START: u64 = 4096;
const HEADER_BYTES: usize = 64;
const MAX_EXTENT: u64 = 256 * 1024 * 1024;

/// A bounded, fully written extent. Appends always use Full durability.
/// Any write/sync error poisons the handle; reopening must revalidate the file.
pub struct ExtentWal { file: File, physical: u64, logical: u64, sequence: u64, poisoned: bool }
fn header(physical:u64, logical:u64, sequence:u64)->[u8;HEADER_BYTES] {
    let mut h=[0;HEADER_BYTES];h[..8].copy_from_slice(MAGIC);
    h[8..16].copy_from_slice(&physical.to_le_bytes());h[16..24].copy_from_slice(&logical.to_le_bytes());h[24..32].copy_from_slice(&sequence.to_le_bytes());
    let crc=crate::crc::hardware_crc32c(&h[..32]);h[32..36].copy_from_slice(&crc.to_le_bytes());h
}
fn validate_frames(frames:&[u8])->Result<(),Error> {
    for_each_wal_frame_raw(frames,|raw,_| {
        if raw[..4]!=*b"LIN\x06" { return Err(io_err("extent requires checksummed WAL frames")); }
        Ok(())
    })?;Ok(())
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
        let mut file=OpenOptions::new().read(true).write(true).open(path).map_err(io_err)?;
        file.try_lock().map_err(io_err)?;
        let length=file.metadata().map_err(io_err)?.len();let mut h=[0u8;HEADER_BYTES];file.read_exact(&mut h).map_err(io_err)?;
        if &h[..8]!=MAGIC||h[36..].iter().any(|b|*b!=0)||crate::crc::hardware_crc32c(&h[..32])!=u32::from_le_bytes(h[32..36].try_into().unwrap()) {return Err(io_err("corrupt extent header"));}
        let physical=u64::from_le_bytes(h[8..16].try_into().unwrap());let logical=u64::from_le_bytes(h[16..24].try_into().unwrap());let sequence=u64::from_le_bytes(h[24..32].try_into().unwrap());
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
        self.file.seek(SeekFrom::Start(DATA_START+self.logical)).map_err(io_err)?;self.file.write_all(frames).map_err(io_err)?;
        self.file.seek(SeekFrom::Start(0)).map_err(io_err)?;self.file.write_all(&header(self.physical,logical,sequence)).map_err(io_err)?;
        durable_sync(&self.file).map_err(io_err)?;
        self.logical=logical;self.sequence=sequence;self.poisoned=false;Ok(())
    }
    pub fn logical_bytes(&self)->u64 {self.logical}
    pub fn physical_bytes(&self)->u64 {self.physical}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Row,Cell,persist::{LogRecord,Pack,SyncMode,append_record}};
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new()->Self {static SEQUENCE:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);let nonce=SEQUENCE.fetch_add(1,std::sync::atomic::Ordering::Relaxed);let path=std::env::temp_dir().join(format!("lin-extent-test-{}-{}-{}",std::process::id(),nonce,std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir(&path).unwrap();Self(path)}
        fn path(&self,name:&str)->std::path::PathBuf{self.0.join(name)}
        fn frame(&self)->Vec<u8>{
            let path=self.path("encoded-frame");let mut file=OpenOptions::new().create_new(true).read(true).write(true).open(&path).unwrap();
            let rec=LogRecord{r#gen:1,next_id:2,pack:Pack::InsertBulk{collection:"docs".into(),rows:vec![Row::from([("id".into(),Cell::from("one")),("body".into(),Cell::from("complete durable payload"))])],edges:vec![]}};
            append_record(&mut file,&rec,&mut Vec::new(),SyncMode::Normal,&mut 0).unwrap();drop(file);std::fs::read(path).unwrap()
        }
    }
    impl Drop for Temp {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}
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
        assert!(wal.append_frames(&[]).is_err());assert!(wal.read_frames().is_err());drop(wal);
        let mut clean=ExtentWal::open(&path).unwrap();assert_eq!(clean.logical_bytes(),0);assert!(clean.read_frames().unwrap().is_empty());drop(clean);
        let bytes=std::fs::read(&path).unwrap();assert!(crate::persist::for_each_wal_frame(&bytes,|_|Ok(())).is_err());
        std::fs::remove_file(path).unwrap();
    }
}
