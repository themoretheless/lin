#[cfg(test)] mod patch_undo_oracle {
use super::*;
#[test] fn restores_patch_added_fields_hash_and_repeated_updates(){
 let mut before=Row::from([("body".into(),Cell::text_arc("old")),("hash".into(),Cell::text_arc("oldhash")),("float".into(),Cell::Float(-0.0)),("длинное_имя_поля_без_усечения".into(),Cell::Bool(true))]);
 for i in 0..8{before.insert(format!("wide_{i}"),Cell::Int(i));}
 let mut patch=Row::from([("body".into(),Cell::text_arc("new")),("added".into(),Cell::Null),("float".into(),Cell::Float(0.0))]);for i in 0..8{patch.insert(format!("wide_{i}"),Cell::Int(i+100));}
 let mut after=before.clone();for(k,v)in&patch{after.insert(k.clone(),v.clone());}after.insert("hash".into(),Cell::text_arc("newhash"));
 let first=RowDelta::capture(&before,&patch,true);assert_eq!(first.0.len(),patch.len()+1);
 let second_patch=Row::from([("body".into(),Cell::text_arc("later")),("added2".into(),Cell::Bool(false)),("hash".into(),Cell::text_arc("explicit"))]);
 let second=RowDelta::capture(&after,&second_patch,true);assert_eq!(second.0.len(),second_patch.len());
 let mut later=after.clone();for(k,v)in second_patch{later.insert(k,v);}second.apply(&mut later);assert_eq!(later,after);first.apply(&mut later);assert_eq!(later,before);
}
#[test] fn absent_hash_is_removed_and_nan_payload_preserved(){
 let before=Row::from([("nan".into(),Cell::Float(f64::from_bits(0x7ff8000000000001)))]);
 let patch=Row::from([("body".into(),Cell::text_arc("text")),("nan".into(),Cell::Float(1.0))]);
 let delta=RowDelta::capture(&before,&patch,true);let mut after=patch;after.insert("hash".into(),Cell::text_arc("computed"));delta.apply(&mut after);assert_eq!(after,before);
}
}
