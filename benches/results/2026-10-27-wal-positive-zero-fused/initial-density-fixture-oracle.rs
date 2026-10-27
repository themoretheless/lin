#[cfg(test)]
mod wal_positive_zero_oracle {
    use super::*;
    fn row(values: Vec<f32>) -> Row {
        Row::from([
            ("id".into(), crate::Cell::from_text("a")),
            ("embedding".into(), crate::Cell::Vec(values.into())),
            ("negative_zero".into(), crate::Cell::Float(-0.0)),
            ("null".into(), crate::Cell::Null),
            ("time".into(), crate::Cell::Time(-4)),
            ("bool".into(), crate::Cell::Bool(true)),
        ])
    }
    #[test]
    fn legacy_decoder_preserves_complete_records_and_ieee_bits() {
        let words = [0u32, 0x80000000, 1, 0x007fffff, 0x00800000, 0x3f800000, 0x7f800000, 0xff800000, 0x7fc12345, 0xffc54321];
        let rows = vec![row(words.map(f32::from_bits).to_vec()), row(vec![])];
        let packs = [Pack::Update {collection:"docs".into(),rows:rows.clone()}, Pack::Delete {collection:"docs".into(),rows:rows.clone()}, Pack::Batch {packs:vec![Pack::Update {collection:"docs".into(),rows:rows.clone()},Pack::Batch {packs:vec![Pack::Delete {collection:"docs".into(),rows:rows.clone()}]}]}];
        for pack in packs {
            let rec = LogRecord {r#gen:4,next_id:7,pack};
            let canonical = rmp_serde::to_vec_named(&rec).unwrap();
            let mut compact=Vec::new();wal_positive_zero::encode(&mut compact,&rec).unwrap();
            assert!(compact.len() < canonical.len());
            let decoded: LogRecord = rmp_serde::from_slice(&compact).unwrap();
            assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(), canonical);
        }
    }
    #[test]
    fn dense_or_unrelated_records_keep_canonical_wire_bytes() {
        for pack in [Pack::Update {collection:"docs".into(),rows:vec![row(vec![-0.0,1.0,-1.0])]},Pack::Insert {collection:"docs".into(),row:row(vec![0.0]),edges:vec![]},Pack::Batch {packs:vec![]}] {
            let rec=LogRecord {r#gen:1,next_id:2,pack};let canonical=rmp_serde::to_vec_named(&rec).unwrap();let mut compact=Vec::new();wal_positive_zero::encode(&mut compact,&rec).unwrap();assert_eq!(compact,canonical);
        }
    }
    #[test]
    fn canonical_frame_budget_is_not_relaxed_by_zero_compaction() {
        let rec=LogRecord {r#gen:1,next_id:2,pack:Pack::Update {collection:"docs".into(),rows:vec![row(vec![0.0;MAX_RECORD as usize/5])]}};
        let canonical=rmp_serde::to_vec_named(&rec).unwrap();assert!(canonical.len()+5>MAX_RECORD as usize);
        let mut compact=Vec::new();let error=wal_positive_zero::encode(&mut compact,&rec).unwrap_err();assert!(error.to_string().contains("16MiB"));assert!(compact.len()<MAX_RECORD as usize);
    }
}
#[cfg(test)]
mod wal_positive_zero_measurement {
    use super::*;
    fn doc_row() -> Row {
        let mut db=crate::Db::empty();
        db.run(r#"insert docs {id:"d-20",uri:"bench://20",title:"doc 20 wal note",layer:"wiki",wing:"rag",body:"body 20",ts:timestamp(1790000000000)}"#).unwrap();
        db.run(r#"update docs[id=="d-20"] cas each {wing:"sys"}"#).unwrap().rows.pop().unwrap()
    }
    fn record(pack:Pack)->LogRecord {LogRecord {r#gen:4,next_id:1,pack}}
    fn update(row:Row)->Pack {Pack::Update {collection:"docs".into(),rows:vec![row]}}
    #[test]
    fn actual_row_frames_keep_legacy_decode_and_report_wire_savings() {
        let row=doc_row();
        let zeros=row.values().filter_map(|v|if let crate::Cell::Vec(v)=v{Some(v.iter().filter(|x|x.to_bits()==0).count())}else{None}).sum::<usize>();
        assert!(zeros>0);
        for pack in [update(row.clone()),Pack::Delete {collection:"docs".into(),rows:vec![row.clone()]}] {
            let rec=record(pack);let canonical=rmp_serde::to_vec_named(&rec).unwrap();let mut encoded=Vec::new();wal_positive_zero::encode(&mut encoded,&rec).unwrap();assert_eq!(canonical.len()-encoded.len(),zeros*4);
            let root=std::env::temp_dir().join(format!("lin-zero-frame-{}-{}",std::process::id(),canonical.len()));std::fs::create_dir(&root).unwrap();let path=log_path(&root);let mut log=std::fs::OpenOptions::new().create_new(true).read(true).write(true).open(&path).unwrap();let mut count=0;append_record(&mut log,&rec,&mut Vec::new(),SyncMode::Full,&mut count).unwrap();drop(log);let frame=std::fs::read(&path).unwrap();assert_eq!(frame.len(),encoded.len()+13);assert_eq!(&frame[..4],&LOG_MAGIC_CHECKSUM);assert_eq!(frame[8],1);
            let mut decoded=Vec::new();for_each_wal_frame(&frame,|value|{decoded.push(rmp_serde::to_vec_named(&value).unwrap());Ok(())}).unwrap();assert_eq!(decoded,vec![canonical.clone()]);
            eprintln!("ZERO_WIRE_PROBE {}",serde_json::json!({"canonical_payload":canonical.len(),"compact_payload":encoded.len(),"canonical_frame":canonical.len()+13,"compact_frame":frame.len(),"positive_zeros":zeros,"codec":frame[8]}));std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn serialization_cpu_probe() {
        if std::env::var_os("LIN_WAL_ZERO_CPU").is_none(){return;}
        use std::{hint::black_box,time::Instant};
        let sparse=doc_row();let mut dense=sparse.clone();dense.insert("embedding".into(),crate::Cell::Vec(vec![0.12345;768].into()));let mut negative=sparse.clone();negative.insert("embedding".into(),crate::Cell::Vec(vec![-0.0;768].into()));let mut zeros=sparse.clone();zeros.insert("embedding".into(),crate::Cell::Vec(vec![0.0;768].into()));let mut empty=sparse.clone();empty.insert("embedding".into(),crate::Cell::Vec(vec![].into()));let mut novec=sparse.clone();novec.remove("embedding");
        let mut prefix=sparse.clone();let mut values=vec![0.12345;768];values[..8].fill(0.0);prefix.insert("embedding".into(),crate::Cell::Vec(values.into()));
        let mut late=sparse.clone();let mut values=vec![0.0;768];values[..8].fill(0.12345);late.insert("embedding".into(),crate::Cell::Vec(values.into()));
        let mut mixed=sparse.clone();mixed.insert("z_dense_vector".into(),crate::Cell::Vec(vec![0.12345;768].into()));
        let cases=[("doc_update",record(update(sparse.clone()))),("doc_delete",record(Pack::Delete {collection:"docs".into(),rows:vec![sparse.clone()]})),("dense_update",record(update(dense))),("negative_zero_update",record(update(negative))),("zero_update",record(update(zeros))),("empty_update",record(update(empty))),("no_vector_update",record(update(novec))),("unrelated_insert",record(Pack::Insert {collection:"docs".into(),row:sparse.clone(),edges:vec![]})),("prefix_only_zeros",record(update(prefix))),("late_zeros_update",record(update(late))),("mixed_vectors",record(update(mixed))),("batch",record(Pack::Batch {packs:vec![update(sparse.clone()),Pack::Delete {collection:"docs".into(),rows:vec![sparse]}]}))];
        let order=std::env::var("LIN_WAL_ZERO_ORDER").unwrap().parse::<usize>().unwrap();
        for offset in 0..cases.len() {
            let (name,rec)=&cases[(offset+order)%cases.len()];let mut baseline=Vec::new();let mut compact=Vec::new();rmp_serde::encode::write_named(&mut baseline,rec).unwrap();wal_positive_zero::encode(&mut compact,rec).unwrap();let decoded:LogRecord=rmp_serde::from_slice(&compact).unwrap();assert_eq!(rmp_serde::to_vec_named(&decoded).unwrap(),baseline);
            let mut modes=["baseline","control","candidate"];modes.rotate_left((offset+order)%3);
            for mode in modes {
                let encoder: fn(&mut Vec<u8>, &LogRecord) = if mode=="candidate" {
                    |buf,rec|wal_positive_zero::encode(buf,rec).unwrap()
                } else {
                    |buf,rec|{rmp_serde::encode::write_named(&mut *buf,rec).unwrap();assert!(buf.len()<=MAX_RECORD as usize);}
                };
                let mut buf=Vec::with_capacity(65536);let loops=4000usize;let started=Instant::now();let mut witness=0;
                for _ in 0..loops {
                    buf.clear();encoder(&mut buf,black_box(rec));
                    witness^=black_box(buf.as_slice()).len();
                }
                let ns=started.elapsed().as_nanos()as f64/loops as f64;black_box(witness);assert!(!buf.is_empty());eprintln!("ZERO_CPU_PROBE {}",serde_json::json!({"case":name,"role":mode,"ns_per_op":ns,"operations":loops,"payload_bytes":buf.len()}));
            }
        }
    }
}
