use lin::{Cell,Row};
use std::collections::{BTreeMap,btree_map::{IntoKeys,IntoValues}};
use std::iter::FusedIterator;
fn row() -> Row {
 Row::from([("z".into(),Cell::Int(2)),("a".into(),Cell::Text("owned".into())),("v".into(),Cell::Vec(vec![1.,2.,3.].into()))])
}
fn exact_keys(_: &IntoKeys<String,Cell>) {}
fn exact_values(_: &IntoValues<String,Cell>) {}
fn iterator_contract<T:DoubleEndedIterator+ExactSizeIterator+FusedIterator>(_:&T) {}
#[test]
fn owning_map_unique_and_shared_are_isolated() {
 for shared in [false,true] {
  let r=row();let held=shared.then(||r.clone());let expected:BTreeMap<String,Cell>=r.iter().map(|(k,v)|(k.clone(),v.clone())).collect();
  let mut map:BTreeMap<String,Cell>=r.into();assert_eq!(map,expected);
  map.insert("a".into(),Cell::Text("changed".into()));map.remove("z");
  if let Some(held)=held {assert_eq!(held.get("a").and_then(Cell::text),Some("owned"));assert_eq!(held.get("z"),Some(&Cell::Int(2)));}
 }
}
#[test]
fn owning_keys_unique_shared_order_and_iterator_contract() {
 for shared in [false,true] {
  let r=row();let held=shared.then(||r.clone());let mut keys=r.into_keys();exact_keys(&keys);iterator_contract(&keys);
  assert_eq!(keys.len(),3);assert_eq!(keys.next().as_deref(),Some("a"));assert_eq!(keys.next_back().as_deref(),Some("z"));assert_eq!(keys.len(),1);assert_eq!(keys.next().as_deref(),Some("v"));assert!(keys.next().is_none());assert!(keys.next_back().is_none());
  if let Some(held)=held {assert_eq!(held.len(),3);assert_eq!(held.get("a").and_then(Cell::text),Some("owned"));}
 }
}
#[test]
fn owning_values_unique_shared_order_and_iterator_contract() {
 for shared in [false,true] {
  let r=row();let held=shared.then(||r.clone());let expected:Vec<_>=r.values().cloned().collect();let mut values=r.into_values();exact_values(&values);iterator_contract(&values);
  assert_eq!(values.len(),3);assert_eq!(values.next(),Some(expected[0].clone()));assert_eq!(values.next_back(),Some(expected[2].clone()));assert_eq!(values.next(),Some(expected[1].clone()));assert!(values.next().is_none());assert!(values.next_back().is_none());
  if let Some(held)=held {assert_eq!(held.values().cloned().collect::<Vec<_>>(),expected);}
 }
}
#[test]
fn owning_empty_iterators_and_conversion() {
 let keys=Row::new().into_keys();exact_keys(&keys);assert_eq!(keys.len(),0);assert_eq!(keys.count(),0);
 let values=Row::new().into_values();exact_values(&values);assert_eq!(values.len(),0);assert_eq!(values.count(),0);
 let map:BTreeMap<String,Cell>=Row::new().into();assert!(map.is_empty());
}
#[test]
fn consuming_keys_and_early_drop_release_values() {
 for shared in [false,true] {
  for consume in [false,true] {
   let payload:std::sync::Arc<[f32]>=vec![1.,2.,3.].into();
   let r=Row::from([("a".into(),Cell::Vec(payload.clone())),("z".into(),Cell::Null)]);
   let held=shared.then(||r.clone());let mut keys=r.into_keys();
   assert_eq!(std::sync::Arc::strong_count(&payload),if shared {3}else{2});
   if consume {assert_eq!(keys.next().as_deref(),Some("a"));assert_eq!(std::sync::Arc::strong_count(&payload),if shared {2}else{1});}
   drop(keys);assert_eq!(std::sync::Arc::strong_count(&payload),if shared {2}else{1});
   drop(held);assert_eq!(std::sync::Arc::strong_count(&payload),1);
  }
 }
}
#[test]
fn consuming_values_and_owned_map_release_values() {
 for shared in [false,true] {
  let payload:std::sync::Arc<[f32]>=vec![1.,2.,3.].into();
  let r=Row::from([("a".into(),Cell::Vec(payload.clone())),("z".into(),Cell::Null)]);let held=shared.then(||r.clone());let mut values=r.into_values();
  let value=values.next().unwrap();drop(values);
  assert_eq!(std::sync::Arc::strong_count(&payload),if shared {3}else{2});drop(value);
  assert_eq!(std::sync::Arc::strong_count(&payload),if shared {2}else{1});drop(held);assert_eq!(std::sync::Arc::strong_count(&payload),1);
  let r=Row::from([("a".into(),Cell::Vec(payload.clone()))]);let held=shared.then(||r.clone());let map:BTreeMap<String,Cell>=r.into();
  assert_eq!(std::sync::Arc::strong_count(&payload),if shared {3}else{2});drop(map);
  assert_eq!(std::sync::Arc::strong_count(&payload),if shared {2}else{1});drop(held);assert_eq!(std::sync::Arc::strong_count(&payload),1);
 }
}
