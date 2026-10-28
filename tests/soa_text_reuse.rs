use lin::{Cell,Row,Store};
use lin::persist::Pack;
use std::sync::Arc;
fn seed(collection:&str,fields:&[(&str,&str)])->Store {
 let mut row=Row::new();row.insert("id".into(),Cell::from_text("one"));
 for(k,v)in fields {row.insert((*k).into(),Cell::from_text(v));}
 let mut store=Store::empty("soa-regression");store.apply_pack(&Pack::InsertBulk{collection:collection.into(),rows:vec![row],edges:vec![]});store
}
fn replace(store:&mut Store,collection:&str,field:&str,value:Option<Cell>){
 let before=store.collection(collection)[0].clone();let mut after=before.clone();
 if let Some(value)=value {after.insert(field.into(),value);}else{after.remove(field);}
 store.collection_mut(collection)[0]=after.clone();store.row_maps_sync_updated(collection,0,&before,&after);
}
#[test]fn docs_reuse_and_changed_text_invalidate_packed_title(){
 let mut s=seed("docs",&[("uri","soa://one"),("title","old title"),("layer","old layer"),("wing","sys")]);
 assert_eq!(s.docs_title_contains_count("old"),1);let held=Arc::clone(&s.docs_wing()[0]);
 replace(&mut s,"docs","ts",Some(Cell::Int(7)));assert!(Arc::ptr_eq(&held,&s.docs_wing()[0]));
 for title in ["new title","long changed title beyond the inline string capacity","new title"] {
  replace(&mut s,"docs","title",Some(Cell::from_text(title)));assert_eq!(s.docs_title_contains_count(title),1);assert_eq!(s.docs_title_contains_count("old"),0);
 }
 replace(&mut s,"docs","layer",Some(Cell::from_text("new layer")));let counts=s.docs_title_contains_count_by_layer("new");assert_eq!(counts.get("new layer"),Some(&1));assert!(!counts.contains_key("old layer"));
 replace(&mut s,"docs","wing",Some(Cell::from_text("new wing")));assert_eq!(held.as_ref(),"sys");assert_eq!(s.docs_wing()[0].as_ref(),"new wing");
 replace(&mut s,"docs","title",None);assert_eq!(s.docs_title_contains_count("new"),0);
 replace(&mut s,"docs","wing",Some(Cell::Int(1)));assert!(s.docs_wing()[0].is_empty());
}
#[test]fn shared_and_equal_distinct_arc_text_stay_correct(){
 let text="a long immutable shared wing value beyond inline capacity";let mut s=seed("docs",&[("title","t"),("layer","l"),("wing",text)]);let held=Arc::clone(&s.docs_wing()[0]);
 replace(&mut s,"docs","ts",Some(Cell::Int(2)));assert!(Arc::ptr_eq(&held,&s.docs_wing()[0]));
 let distinct:Arc<str>=Arc::from(text);assert!(!Arc::ptr_eq(&held,&distinct));replace(&mut s,"docs","wing",Some(Cell::Text(Arc::clone(&distinct))));assert!(Arc::ptr_eq(&distinct,&s.docs_wing()[0]));assert_eq!(s.docs_wing()[0].as_ref(),text);
}
#[test]fn user_and_order_columns_keep_updates_and_empty_transitions(){
 let mut users=seed("users",&[("email","before")]);let held=Arc::clone(&users.users_email()[0]);replace(&mut users,"users","ts",Some(Cell::Int(1)));assert!(Arc::ptr_eq(&held,&users.users_email()[0]));
 replace(&mut users,"users","email",Some(Cell::from_text("after")));assert_eq!(users.users_email()[0].as_ref(),"after");replace(&mut users,"users","email",None);assert!(users.users_email()[0].is_empty());
 let mut orders=seed("orders",&[("user_id","user-one")]);let held=Arc::clone(&orders.orders_user_id()[0]);replace(&mut orders,"orders","total",Some(Cell::Float(12.5)));assert!(Arc::ptr_eq(&held,&orders.orders_user_id()[0]));assert_eq!(orders.orders_total(),&[12.5]);
 replace(&mut orders,"orders","user_id",Some(Cell::from_text("user-two")));assert_eq!(orders.orders_user_id()[0].as_ref(),"user-two");replace(&mut orders,"orders","user_id",None);assert!(orders.orders_user_id()[0].is_empty());
}
#[test]fn moved_identity_rebuilds_slot_mapping(){
 let mut s=seed("docs",&[("uri","soa://one"),("title","title"),("layer","layer"),("wing","sys")]);replace(&mut s,"docs","id",Some(Cell::from_text("two")));assert_eq!(s.row_index("docs","two"),Some(0));assert_eq!(s.row_index("docs","one"),None);assert_eq!(s.docs_title_contains_count("title"),1);
}
