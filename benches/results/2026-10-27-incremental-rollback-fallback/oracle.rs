
#[cfg(test)]
mod incremental_rollback_fallback_oracle {
 use super::*;
 fn original(collection:&str)->Row {
  let mut row=Row::from([("id".into(),Cell::Text("seed".into()))]);
  for (k,v)in match collection{"docs"=>vec![("uri","bench://seed"),("title","oldtitle"),("layer","wiki"),("wing","rag")],"orders"=>vec![("user_id","u1")],"users"=>vec![("email","old@example.com")],_=>vec![]}{row.insert(k.into(),Cell::Text(v.into()));}
  if collection=="orders"{row.insert("total".into(),Cell::Float(1.0));}row
 }
 fn seeded(collection:&str)->(Store,Row){let row=original(collection);let mut s=Store::empty("test");s.collection_mut(collection).push(row.clone());s.rebuild_row_maps_collection(collection);(s,row)}
 fn check(s:&Store,collection:&str,row:&Row){assert_eq!(rmp_serde::to_vec_named(s.collection(collection)).unwrap(),rmp_serde::to_vec_named(&[row]).unwrap());assert_eq!(s.row_index(collection,"seed"),Some(0));match collection{"docs"=>{assert_eq!(s.docs_title[0].as_ref(),"oldtitle");assert_eq!(s.docs_layer[0].as_ref(),"wiki");assert_eq!(s.docs_wing[0].as_ref(),"rag");assert_eq!(s.get_by_uri("bench://seed").unwrap()["id"].text(),Some("seed"));},"orders"=>{assert_eq!(s.orders_user_id[0].as_ref(),"u1");assert_eq!(s.orders_total[0],1.0);},"users"=>assert_eq!(s.users_email[0].as_ref(),"old@example.com"),_=>{}}}
 #[test]
 fn short_columns_rebuild_after_restored_row_is_installed(){for mode in 0..6{let collection=match mode{0..=2=>"docs",3..=4=>"orders",_=>"users"};let(mut s,row)=seeded(collection);let expected=row.clone();match mode{0=>s.docs_title.clear(),1=>s.docs_layer.clear(),2=>s.docs_wing.clear(),3=>s.orders_user_id.clear(),4=>s.orders_total.clear(),_=>s.users_email.clear()};s.restore_row_incremental_owned(collection,0,row);check(&s,collection,&expected);}}
 #[test]
 fn stale_id_slot_is_rebuilt_from_restored_rows(){let(mut s,row)=seeded("docs");let expected=row.clone();s.by_id.get_mut("docs").unwrap().insert("seed".into(),999);s.restore_row_incremental_owned("docs",0,row);check(&s,"docs",&expected);}
 #[test]
 fn absent_position_keeps_rows_and_maps(){let(mut s,row)=seeded("docs");let expected=row.clone();s.restore_row_incremental_owned("docs",99,row);check(&s,"docs",&expected);}
}
