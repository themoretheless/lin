#[cfg(test)] mod delta_undo_oracle {
use super::*;
#[test] fn restores_changed_added_removed_names_and_repeated_edits(){
 let mut before=Row::from([("body".into(),Cell::text_arc("old")),("hash".into(),Cell::text_arc("oldhash")),("remove".into(),Cell::Int(7)),("float".into(),Cell::Float(-0.0)),("длинное_имя_поля_без_усечения".into(),Cell::Bool(true))]);
 for i in 0..8 {before.insert(format!("wide_{i}"),Cell::Int(i));}
 let mut after=before.clone();after.insert("body".into(),Cell::text_arc("new"));after.insert("hash".into(),Cell::text_arc("newhash"));after.insert("added".into(),Cell::Null);after.remove("remove");after.insert("float".into(),Cell::Float(0.0));for i in 0..8{after.insert(format!("wide_{i}"),Cell::Int(i+100));}
 let first=RowDelta::between(&before,&after);let mut later=after.clone();later.insert("body".into(),Cell::text_arc("later"));later.insert("added2".into(),Cell::Bool(false));let second=RowDelta::between(&after,&later);second.apply(&mut later);assert_eq!(later,after);first.apply(&mut later);assert_eq!(later,before);
}
}
