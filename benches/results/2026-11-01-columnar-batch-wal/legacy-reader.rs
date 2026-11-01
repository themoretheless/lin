use lin::Db;
use std::{fs,path::PathBuf};
fn row_value(row: &lin::Row) -> serde_json::Value {
    serde_json::Value::Object(row.iter().map(|(k,v)| {
        let value=match v.text() { Some(s)=>serde_json::Value::String(s.to_owned()), None=>serde_json::to_value(v).unwrap() };
        (k.clone(),value)
    }).collect())
}
fn main() {
    let p=PathBuf::from(std::env::args().nth(1).expect("fixture directory"));
    let bytes=fs::read(p.join("log.wal")).unwrap();
    let mut expected:serde_json::Value=serde_json::from_slice(&fs::read(p.join("expected.json")).unwrap()).unwrap();
    let generation:u64=fs::read_to_string(p.join("expected-generation.txt")).unwrap().parse().unwrap();
    let mut db=Db::empty(); assert!(db.apply_wal(&bytes).unwrap()>0);
    assert_eq!(db.store.r#gen,generation);
    // JSON's f64 parser can round the decimal rendering of a stored f32 by
    // one f64 ULP. Compare the declared f32 values by bits, then normalize only
    // this typed embedding column for the complete JSON object comparison.
    for expected_row in expected.as_array_mut().unwrap() {
        let id = expected_row["id"].as_str().unwrap().to_owned();
        let actual = db.store.get_by_id("docs", &id).unwrap()["embedding"].as_vec().unwrap();
        assert_eq!(expected_row["embedding"]["t"].as_str(), Some("Vec"));
        let values = expected_row["embedding"]["v"].as_array_mut().unwrap();
        assert_eq!(actual.len(), values.len());
        for (a,e) in actual.iter().zip(values) {
            let value = e.as_f64().unwrap(); assert!(value.is_finite());
            let expected_f32 = value as f32;
            assert_eq!(a.to_bits(), expected_f32.to_bits(), "legacy embedding f32 bits");
            *e = serde_json::Value::from(expected_f32 as f64);
        }
    }
    let mut rows:Vec<_>=db.store.collection("docs").iter().map(row_value).collect();
    rows.sort_by_key(|v|v["id"].to_string());
    assert_eq!(serde_json::Value::Array(rows),expected,"full legacy-reader row values");
    assert_eq!(db.apply_wal(&bytes).unwrap(),0,"duplicate publication must be a noop");
    assert_eq!(db.store.collection("docs").len(),expected.as_array().unwrap().len());
    let mut broken=bytes;*broken.last_mut().unwrap()^=1;
    assert!(lin::persist::for_each_wal_frame(&broken,|_|Ok(())).is_err(),"CRC corruption must fail");
    println!("{}",serde_json::json!({"legacy_full_values_verified":true,"embedding_f32_bits_verified":true,"rows":expected.as_array().unwrap().len(),"generation_verified":generation,"duplicate_apply_noop":true,"crc_corruption_rejected":true,"performance_qualified":false}));
}
