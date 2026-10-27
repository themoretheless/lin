
#[cfg(test)]mod empty_destination_oracle{
 use super::*;
 fn row(i:usize)->Row{
  let mut r=Row::from([("title".into(),crate::store::Cell::text_arc(format!("common term{} ÉCOLE",i%13))),("body".into(),crate::store::Cell::text_arc(format!("body {}",i%31))),("snippet".into(),crate::store::Cell::text_arc("shared τ"))]);
  if i%7==0{r.insert("body".into(),crate::store::Cell::Null);}r
 }
 fn validate(index:&FtsIndex,rows:&[Row]){
  let rebuilt=FtsIndex::build(rows,&index.fields);
  let terms=index.postings.keys().chain(index.adds.keys()).chain(index.dels.keys()).chain(rebuilt.postings.keys()).collect::<std::collections::BTreeSet<_>>();
  for term in terms{assert_eq!(index.matches(term).unwrap_or_default(),rebuilt.matches(term).unwrap_or_default(),"{term}");}
 }
 #[test]fn empty_destination_matches_generic_state_and_full_rebuild_through_folds(){
  for fields in [vec!["title".into(),"body".into()],vec!["title".into(),"snippet".into()],Vec::<String>::new()]{
   let mut rows=(0..530).map(row).collect::<Vec<_>>();let mut fast=FtsIndex::build(&rows,&fields);let mut reference=fast.clone();
   let mut state=0x12345678u64;
   for step in 0..900{
    state=state.wrapping_mul(6364136223846793005).wrapping_add(1);let pos=state as usize%rows.len();
    if step%5==0{
     let before=rows[pos].clone();let after=row(step+1000);fast.sync_row(pos,&before,&after);reference.sync_row(pos,&before,&after);rows[pos]=after;
    }else{
     let from=rows.len()-1;let doomed=rows.swap_remove(pos);fast.remove_row(pos,&doomed);reference.remove_row(pos,&doomed);
     if from!=pos{fast.move_to_empty_slot(from,pos,&rows[pos]);reference.move_row(from,pos,&rows[pos]);}
     let next=row(step+2000);fast.append_slab(rows.len(),std::slice::from_ref(&next));reference.append_slab(rows.len(),std::slice::from_ref(&next));rows.push(next);
    }
    assert_eq!(fast.postings,reference.postings);assert_eq!(fast.adds,reference.adds);assert_eq!(fast.dels,reference.dels);
    if step%17==0{validate(&fast,&rows);}
    if step%113==0{fast.fold_all();reference.fold_all();}
   }
   validate(&fast,&rows);fast.fold_all();validate(&fast,&rows);
  }
 }
}
