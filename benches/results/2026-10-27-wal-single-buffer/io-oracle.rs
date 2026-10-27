
#[cfg(test)]
mod single_buffer_oracle {
 use super::*;
 use std::sync::atomic::{AtomicU64,Ordering};
 static SEQ:AtomicU64=AtomicU64::new(0);
 fn logfile()->(std::path::PathBuf,File){let p=std::env::temp_dir().join(format!("lin-single-buffer-{}-{}",std::process::id(),SEQ.fetch_add(1,Ordering::Relaxed)));let f=OpenOptions::new().create_new(true).read(true).write(true).open(&p).unwrap();(p,f)}
 fn record(n:usize)->LogRecord{let mut v=vec![0.0f32;n];for (x,b)in v.iter_mut().zip([0x80000000,0x7fc01234,0x7f800000,1,0xff800000]){*x=f32::from_bits(b);}let row=std::collections::BTreeMap::from([("embedding".into(),crate::Cell::Vec(v.into())),("id".into(),crate::Cell::Text("a".into()))]);LogRecord{r#gen:7,next_id:19,pack:Pack::Update{collection:"docs".into(),rows:vec![row]}}}
 #[test]
 fn exact_reference_frames_and_legacy_decode(){
  for n in [0,1,768,14000,220000]{for sync in [SyncMode::Normal,SyncMode::Full]{
   let rec=record(n);let payload=rmp_serde::to_vec_named(&rec).unwrap();let (pa,mut a)=logfile();let (pb,mut b)=logfile();let mut ca=0;let mut cb=0;
   write_frame(&mut a,&LOG_MAGIC_V1,&payload,sync,&mut ca).unwrap();append_record_buffered(&mut b,&rec,&mut Vec::new(),sync,&mut cb).unwrap();assert_eq!(ca,cb);drop(a);drop(b);let bytes=std::fs::read(&pb).unwrap();assert_eq!(std::fs::read(&pa).unwrap(),bytes);let mut decoded=Vec::new();for_each_wal_frame(&bytes,|r|{decoded.push(rmp_serde::to_vec_named(&r).unwrap());Ok(())}).unwrap();assert_eq!(decoded,vec![payload]);std::fs::remove_file(pa).unwrap();std::fs::remove_file(pb).unwrap();
  }}
 }
 #[test]
 fn rejected_record_does_not_write_or_advance_count(){let rec=record(MAX_RECORD as usize/5+1);let (path,mut file)=logfile();file.write_all(b"previous").unwrap();let mut count=8;let error=append_record_buffered(&mut file,&rec,&mut Vec::new(),SyncMode::Full,&mut count).unwrap_err();assert!(error.to_string().contains("16MiB"));assert_eq!(count,8);drop(file);assert_eq!(std::fs::read(&path).unwrap(),b"previous");std::fs::remove_file(path).unwrap();}

 #[test]
 fn io_measurement(){
  if std::env::var_os("LIN_SINGLE_BUFFER_IO").is_none(){return;}
  let order:usize=std::env::var("LIN_SINGLE_BUFFER_ORDER").unwrap().parse().unwrap();
  for (case,n) in [("scalar",0),("vector768",768),("vector14000",14000),("vector220000",220000)]{
   let mut rec=record(n);if case=="scalar"{if let Pack::Update{rows,..}=&mut rec.pack{rows[0].remove("embedding");rows[0].insert("value".into(),crate::Cell::Int(7));}}
   let payload=rmp_serde::to_vec_named(&rec).unwrap();let (rp,mut rf)=logfile();write_frame(&mut rf,&LOG_MAGIC_V1,&payload,SyncMode::Normal,&mut 0).unwrap();drop(rf);let expected=std::fs::read(&rp).unwrap();std::fs::remove_file(rp).unwrap();
   for (mode,sync)in [("normal",SyncMode::Normal),("full",SyncMode::Full)]{
    for sample in 0..12{for step in 0..3{let role=(order+sample+step)%3;let (path,mut file)=logfile();let prefix=vec![0x5a;4096];file.write_all(&prefix).unwrap();durable_sync(&file).unwrap();let mut buf=Vec::with_capacity(payload.len()+13);let mut count=4096;
     let start=std::time::Instant::now();if role==2{append_record_buffered(&mut file,std::hint::black_box(&rec),&mut buf,sync,&mut count).unwrap();}else{append_record_v1(&mut file,std::hint::black_box(&rec),&mut buf,sync,&mut count).unwrap();}let elapsed=start.elapsed().as_nanos();
     assert_eq!(count,4096+expected.len()as u64);drop(file);let bytes=std::fs::read(&path).unwrap();assert_eq!(&bytes[..4096],&prefix);assert_eq!(&bytes[4096..],&expected);std::fs::remove_file(path).unwrap();
     eprintln!("SINGLE_BUFFER_IO {}",serde_json::json!({"case":case,"mode":mode,"sample":sample,"role":(["baseline","control","candidate"][role]),"ns":elapsed,"payload_bytes":payload.len(),"frame_bytes":expected.len()}));
    }}
   }
  }
 }
}
