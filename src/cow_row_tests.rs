#[cfg(test)] mod cow_row_oracle {
 use super::*;
 #[test] fn cloned_rows_do_not_alias_mutations_and_serde_stays_map_shaped(){
  let original=Row::from([("name".into(),Cell::text_arc("ёж")),("float".into(),Cell::Float(-0.0)),("long_payload".into(),Cell::text_arc("shared long payload shared long payload"))]);
  let map:BTreeMap<String,Cell>=original.iter().map(|(k,v)|(k.clone(),v.clone())).collect();
  assert_eq!(serde_json::to_vec(&original).unwrap(),serde_json::to_vec(&map).unwrap());
  assert_eq!(rmp_serde::to_vec(&original).unwrap(),rmp_serde::to_vec(&map).unwrap());
  let mut changed=original.clone();changed.insert("name".into(),Cell::text_arc("new"));changed.remove("float");changed.entry("added".into()).or_insert(Cell::Int(4));
  assert_eq!(original.get("name").and_then(Cell::text),Some("ёж"));assert!(original.contains_key("float"));assert!(!original.contains_key("added"));
  let restored:Row=rmp_serde::from_slice(&rmp_serde::to_vec(&original).unwrap()).unwrap();assert_eq!(restored,original);
  let mut via_iter=original.clone();for(_,v)in&mut via_iter{*v=Cell::Null;}assert_eq!(original.get("name").and_then(Cell::text),Some("ёж"));
  let owned:Vec<_>=original.clone().into_iter().collect();assert_eq!(owned.len(),original.len());assert_eq!(original.get("name").and_then(Cell::text),Some("ёж"));
 }
}
#[cfg(test)] mod inplace_patch_engine_oracle {
 use super::*;
 #[test] fn held_row_isolated_and_body_hash_fts_maps_restore_on_rollback(){
  let mut db=crate::Db::empty();
  db.run(r#"insert docs {id:"d",uri:"fixture://d",title:"oldtoken",layer:"wiki",wing:"rag",body:"old body"}"#).unwrap();
  let held=db.run(r#"docs | id == "d""#).unwrap();
  let before=db.store.get_by_id("docs","d").unwrap().clone();let hash=before["hash"].text().unwrap().to_owned();
  let prepared=db.prepare(&format!(r#"update docs[id=="d"] cas "{hash}" {{layer:"wiki",body:"new body",title:"newtoken",wing:"sys"}}"#)).unwrap();
  {
   let mut tx=db.transaction().unwrap();let result=tx.run_prepared(&prepared).unwrap();assert_eq!(result.done.n,1);
   let now=tx.db().store.get_by_id("docs","d").unwrap();assert_eq!(now["title"].text(),Some("newtoken"));assert_ne!(now["hash"],before["hash"]);
   assert_eq!(held.rows[0],before);
   let fts=tx.db().store.fts.get("docs").unwrap();assert_eq!(fts.matches("newtoken"),Some(vec![0]));assert!(fts.matches("oldtoken").unwrap_or_default().is_empty());
   drop(result);tx.rollback().unwrap();
  }
  assert_eq!(db.store.get_by_id("docs","d").unwrap(),&before);assert_eq!(held.rows[0],before);
  let fts=db.store.fts.get("docs").unwrap();assert_eq!(fts.matches("oldtoken"),Some(vec![0]));assert!(fts.matches("newtoken").unwrap_or_default().is_empty());
 }
}

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

#[cfg(test)]
mod rollback_hnsw_oracle {
    use super::*;
    #[test]
    fn mixed_rollback_restores_hnsw_slots_after_multiple_swap_deletes() {
        for mode in ["rows", "append-first", "append-last"] {
            let mut db = crate::Db::empty();
            for i in 0..8 {
                db.run(&format!(r#"insert docs {{id:"d{i}",title:"vector document number {i}"}}"#)).unwrap();
            }
            db.build_hnsw("docs").unwrap();
            let before = db.store.collection("docs").to_vec();
            let Cell::Vec(probe) = &before[2]["embedding"] else { panic!("embedding missing") };
            {
                let mut tx = db.transaction().unwrap();
                if mode == "append-first" {
                    tx.run(r#"insert docs {id:"temporary",title:"temporary document"}"#).unwrap();
                }
                tx.run(r#"delete docs[id=="d2"] cas """#).unwrap();
                tx.run(r#"delete docs[id=="d5"] cas """#).unwrap();
                if mode == "append-last" {
                    tx.run(r#"insert docs {id:"temporary",title:"temporary document"}"#).unwrap();
                }
                tx.rollback().unwrap();
            }
            assert_eq!(db.store.collection("docs"), before, "{mode}");
            let mut ids = db.store.hnsw["docs"].search(probe, 100, 128).into_iter().map(|(_,id)| id).collect::<Vec<_>>();
            ids.sort_unstable();
            assert_eq!(ids, (0..8).collect::<Vec<_>>(), "{mode}");
            for (_, id) in db.store.hnsw["docs"].search(probe, 100, 128) {
                let Cell::Vec(vector) = &db.store.collection("docs")[id]["embedding"] else { panic!("embedding missing") };
                let found = db.store.hnsw["docs"].search(vector, 1, 128);
                assert_eq!(found[0].1, id, "{mode}");
            }
        }
    }

    #[test]
    fn append_rollback_removes_hnsw_nodes_at_truncated_slots() {
        let mut db = crate::Db::empty();
        db.run(r#"insert docs [{id:"a",title:"first document"},{id:"b",title:"second document"}]"#).unwrap();
        db.build_hnsw("docs").unwrap();
        for _ in 0..3 {
            let probe;
            {
                let mut tx = db.transaction().unwrap();
                let inserted = tx.run(r#"insert docs {id:"temporary",title:"temporary document"}"#).unwrap();
                let Cell::Vec(vector) = &inserted.rows[0]["embedding"] else { panic!("embedding missing") };
                probe = vector.clone();
                tx.rollback().unwrap();
            }
            let mut actual = db.store.hnsw["docs"].search(&probe, 20, 64).into_iter().map(|(_,id)| id).collect::<Vec<_>>();
            actual.sort_unstable();
            assert_eq!(actual, vec![0,1]);
        }
    }

    #[test]
    fn rollback_restores_hnsw_after_delete() {
        let mut db = crate::Db::empty();
        db.run(r#"insert docs {id:"d",title:"old vector document"}"#).unwrap();
        db.build_hnsw("docs").unwrap();
        let before = db.store.get_by_id("docs", "d").unwrap().clone();
        let Cell::Vec(vector) = &before["embedding"] else { panic!("embedding missing") };
        let expected = db.store.hnsw["docs"].search(vector, 5, 64);
        assert_eq!(expected.len(), 1);
        let query = format!(r#"delete docs[id=="d"] cas "{}""#, before.get("hash").and_then(Cell::text).unwrap_or(""));
        {
            let mut tx = db.transaction().unwrap();
            tx.run(&query).unwrap();
            assert!(tx.db().store.hnsw["docs"].search(vector, 5, 64).is_empty());
            tx.rollback().unwrap();
        }
        assert_eq!(db.store.get_by_id("docs", "d").unwrap(), &before);
        assert_eq!(db.store.hnsw["docs"].search(vector, 5, 64), expected);
    }
}
