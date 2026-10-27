use lin::{Db, Cell};
use std::{fs, path::Path};
fn snapshot(root: &Path, phase: usize, db: &Db, since: u64) {
    fs::write(root.join(format!("{phase}.wal")), db.export_wal_since(since).unwrap()).unwrap();
    fs::write(root.join(format!("{phase}.rows")), rmp_serde::to_vec_named(db.store.collection("docs")).unwrap()).unwrap();
    fs::write(root.join(format!("{phase}.generation")), db.store.r#gen.to_string()).unwrap();
}
fn produce(root: &Path) {
    assert!(!root.exists()); fs::create_dir(root).unwrap();
    let mut db=Db::open(root.join("db")).unwrap();
    db.run(r#"insert docs [{id:"a",uri:"u:a",body:"sparse wal compatibility"},{id:"b",uri:"u:b",body:"complete row validation"}]"#).unwrap();
    snapshot(root,1,&db,0);
    let first=db.store.r#gen;
    {let mut tx=db.transaction().unwrap();tx.run(r#"update docs[id=="a"] cas each {wing:"sys"}"#).unwrap();tx.run(r#"update docs[id=="b"] cas each {wing:"sys"}"#).unwrap();tx.commit().unwrap();}
    snapshot(root,2,&db,first);
    let second=db.store.r#gen;
    db.run(r#"delete docs[id=="a"] cas each"#).unwrap();
    snapshot(root,3,&db,second);
    let third=db.store.r#gen;
    {let mut tx=db.transaction().unwrap();tx.run(r#"update docs[id=="b"] cas each {wing:"rag"}"#).unwrap();tx.rollback().unwrap();}
    snapshot(root,4,&db,third);assert!(fs::read(root.join("4.wal")).unwrap().is_empty());
    {let mut tx=db.transaction().unwrap();tx.run(r#"insert docs {id:"c",uri:"u:c",body:"mixed batch fallback"}"#).unwrap();tx.run(r#"update docs[id=="b"] cas each {wing:"rag"}"#).unwrap();tx.commit().unwrap();}
    snapshot(root,5,&db,third);
    let words=[0u32,0x80000000,1,0x007fffff,0x00800000,0x7f800000,0xff800000,0x7fc12345,0xffc54321];
    let mut vector=vec![0.0;768];for(i,w)in words.into_iter().enumerate(){vector[i]=f32::from_bits(w);}
    let row=lin::Row::from([("id".into(),Cell::from_text("edge")),("embedding".into(),Cell::Vec(vector.into())),("empty".into(),Cell::Vec(vec![].into())),("scalar".into(),Cell::Float(-0.0)),("null".into(),Cell::Null), ("time".into(),Cell::Time(-7))]);
    let record=lin::persist::LogRecord{r#gen:99,next_id:200,pack:lin::persist::Pack::Update{collection:"edge".into(),rows:vec![row]}};
    fs::write(root.join("edge.expected"),rmp_serde::to_vec_named(&record).unwrap()).unwrap();
    let mut file=fs::OpenOptions::new().write(true).create_new(true).open(root.join("edge.wal")).unwrap();let mut bytes=0;
    lin::persist::append_record(&mut file,&record,&mut Vec::new(),lin::SyncMode::Full,&mut bytes).unwrap();drop(file);
    println!("{}",serde_json::json!({"producer_complete":true,"phases":5,"edge_frame_bytes":bytes}));
}
fn verify(root: &Path) {
    let mut db=Db::empty();let mut outcomes=Vec::new();
    for phase in 1..=5 {
        let frame=fs::read(root.join(format!("{phase}.wal"))).unwrap();
        let applied=db.apply_wal(&frame).unwrap();
        assert_eq!(rmp_serde::to_vec_named(db.store.collection("docs")).unwrap(),fs::read(root.join(format!("{phase}.rows"))).unwrap());
        let generation=fs::read_to_string(root.join(format!("{phase}.generation"))).unwrap().parse::<u64>().unwrap();assert_eq!(db.store.r#gen,generation);
        let mut canonical=0;lin::persist::for_each_wal_frame(&frame,|record|{canonical+=rmp_serde::to_vec_named(&record).unwrap().len()+13;Ok(())}).unwrap();
        if phase==2{assert_eq!(frame.len(),canonical);assert_eq!(frame[8],1);} if phase==3{assert!(frame.len()<canonical);assert_eq!(frame[8],1);}
        if phase==4{assert_eq!(applied,0);}
        outcomes.push(serde_json::json!({"phase":phase,"applied":applied,"generation":generation,"frame_bytes":frame.len(),"canonical_frame_bytes":canonical}));
    }
    let mut edge=Vec::new();lin::persist::for_each_wal_frame(&fs::read(root.join("edge.wal")).unwrap(),|record|{edge.push(rmp_serde::to_vec_named(&record).unwrap());Ok(())}).unwrap();assert_eq!(edge,vec![fs::read(root.join("edge.expected")).unwrap()]);
    println!("{}",serde_json::json!({"legacy_reader_passed":true,"full_canonical_rows":true,"ieee_edge_record":true,"outcomes":outcomes}));
}
fn main(){let args:Vec<String>=std::env::args().collect();assert_eq!(args.len(),3);match args[1].as_str(){"produce"=>produce(Path::new(&args[2])),"verify"=>verify(Path::new(&args[2])),_=>panic!("mode")}}
