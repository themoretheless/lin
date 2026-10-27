#[cfg(test)]
mod lazy_row_oracle {
    use super::*;
    use std::hash::{Hash,Hasher};
    fn hash<T:Hash>(v:&T)->u64 {let mut h=std::collections::hash_map::DefaultHasher::new();v.hash(&mut h);h.finish()}
    #[test]
    fn representation_preserves_eq_hash_wire_debug_and_snapshot_isolation() {
        fn traits<T:Send+Sync+Eq+Hash+std::panic::UnwindSafe+std::panic::RefUnwindSafe>(){} traits::<Row>();
        let mut inline=Row::from([("a".into(),Cell::Float(-0.0)),("b".into(),Cell::Vec(vec![1.,2.,3.].into()))]);
        let map:BTreeMap<String,Cell>=inline.iter().map(|(k,v)|(k.clone(),v.clone())).collect();
        assert!(matches!(inline.0,RowStorage::Inline(_)));let debug=format!("{inline:?}");let json=serde_json::to_vec(&map).unwrap();let packed=rmp_serde::to_vec_named(&map).unwrap();
        inline.ensure_shared();assert!(matches!(inline.0,RowStorage::Shared(_)));assert_eq!(hash(&inline),hash(&map));assert_eq!(format!("{inline:?}"),debug);
        assert_eq!(serde_json::to_vec(&inline).unwrap(),json);assert_eq!(rmp_serde::to_vec_named(&inline).unwrap(),packed);
        let decoded:Row=rmp_serde::from_slice(&packed).unwrap();assert!(matches!(decoded.0,RowStorage::Inline(_)));assert_eq!(decoded,inline);assert_eq!(hash(&decoded),hash(&inline));
        let held=inline.clone();inline.insert("a".into(),Cell::Float(0.0));assert_ne!(held,inline);assert_eq!(held["a"],Cell::Float(-0.0));assert_eq!(hash(&held),hash(&map));
        let mut raw=Row::from([("x".into(),Cell::Float(f64::from_bits(0x7ff8000000000001)))]);let before=raw.clone();raw.ensure_shared();assert_eq!(raw,before);assert_eq!(hash(&raw),hash(&before));
    }
    #[test]
    fn deterministic_map_model_across_promotions_and_held_clones() {
        let mut row=Row::new();let mut model=BTreeMap::new();let mut snapshots=Vec::new();let mut seed=17u64;
        for step in 0..1024 {
            seed=seed.wrapping_mul(6364136223846793005).wrapping_add(1);let key=format!("k{}",(seed>>32)%7);
            match step%7 {
                0|1=>{let value=Cell::Int((seed>>32)as i64);assert_eq!(row.insert(key.clone(),value.clone()),model.insert(key,value));}
                2=>{assert_eq!(row.remove(&key),model.remove(&key));}
                3=>{snapshots.push((row.clone(),model.clone()));}
                4=>{row.ensure_shared();}
                5=>{row.entry(key.clone()).or_insert(Cell::Bool(true));model.entry(key).or_insert(Cell::Bool(true));}
                _=>{for(k,v)in row.iter_mut(){*v=Cell::Int(k.len()as i64)}for(k,v)in model.iter_mut(){*v=Cell::Int(k.len()as i64)}}
            }
            assert_eq!(row.as_map(),&model);assert_eq!(hash(&row),hash(&model));
            for(held,want)in &snapshots{assert_eq!(held.as_map(),want);assert_eq!(hash(held),hash(want));}
            if snapshots.len()>16 {snapshots.remove(0);}
        }
        let owned:BTreeMap<String,Cell>=row.into();assert_eq!(owned,model);
    }
    #[test]
    fn actual_update_promotes_only_touched_row_and_keeps_held_result() {
        let mut db=crate::Db::empty();db.run(r#"insert docs [{ id:"a", uri:"u:a",title:"a",body:"a",wing:"rag"},{ id:"b",uri:"u:b",title:"b",body:"b",wing:"rag"}]"#).unwrap();
        assert!(db.store.collection("docs").iter().all(|r|matches!(r.0,RowStorage::Inline(_))));
        let before=db.store.get_by_id("docs","a").unwrap();let cas=before.get("hash").unwrap().text().unwrap().to_owned();let before=before.clone();
        let out=db.run(&format!(r#"update docs[id=="a"] cas "{cas}" {{wing:"sys"}}"#)).unwrap();assert_eq!(out.done.n,1);assert_eq!(before["wing"].text(),Some("rag"));
        assert!(matches!(db.store.get_by_id("docs","a").unwrap().0,RowStorage::Shared(_)));assert!(matches!(db.store.get_by_id("docs","b").unwrap().0,RowStorage::Inline(_)));
        let _next=db.run(&format!(r#"update docs[id=="a"] cas "{cas}" {{wing:"next"}}"#)).unwrap();assert_eq!(out.rows[0]["wing"].text(),Some("sys"));
    }
}
