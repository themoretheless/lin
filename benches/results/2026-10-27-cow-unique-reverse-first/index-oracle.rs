
#[cfg(test)]mod reverse_first_oracle{
 use super::*;
 #[test]fn unchanged_unique_key_keeps_conflicts_and_key_release_correct(){
  let def=crate::catalog::IndexDef{collection:"docs".into(),unique:true,fields:vec!["title".into()],pred:None,bitmap_card_field:None};
  let mut index=LiveIndex::new(def);
  let a:crate::Row=[("title".into(),crate::Cell::text_arc("one"))].into_iter().collect();
  let b:crate::Row=[("title".into(),crate::Cell::text_arc("two"))].into_iter().collect();
  let c:crate::Row=[("title".into(),crate::Cell::text_arc("three"))].into_iter().collect();
  index.insert_at(0,&a).unwrap();index.insert_at(1,&b).unwrap();
  for _ in 0..8{index.insert_at(0,&a).unwrap();}
  assert!(index.insert_at(1,&a).unwrap_err().contains("unique"));
  assert_eq!(index.reverse.get(&0),Some(&index.key_of(&a)));assert_eq!(index.reverse.get(&1),Some(&index.key_of(&b)));
  index.insert_at(0,&c).unwrap();index.insert_at(1,&a).unwrap();
  assert_eq!(index.forward.len(),2);assert!(!index.forward.contains_key(&index.key_of(&b)));
  assert_eq!(index.forward[&index.key_of(&a)].to_indices(),vec![1]);assert_eq!(index.forward[&index.key_of(&c)].to_indices(),vec![0]);
 }
}
