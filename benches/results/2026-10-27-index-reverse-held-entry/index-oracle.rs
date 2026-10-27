
#[cfg(test)]mod bitmap_empty_oracle{
 use super::*;
 fn validate(list:&PostingList,expected:&std::collections::BTreeSet<usize>){
  assert_eq!(list.is_empty(),expected.is_empty());assert_eq!(list.count(),expected.len());
  let mut actual=list.to_indices();actual.sort_unstable();assert_eq!(actual,expected.iter().copied().collect::<Vec<_>>());
 }
 #[test]fn empty_probe_tracks_dense_sparse_and_last_bit_across_promotion_and_growth(){
  for bitmap in [false,true]{for reverse in [false,true]{
   let mut list=PostingList::new(bitmap,0);let mut expected=std::collections::BTreeSet::new();validate(&list,&expected);
   let positions=(0..96).chain([127,128,65535,65536,99999,1_048_575]).collect::<Vec<_>>();
   for &pos in &positions{list.insert(pos);expected.insert(pos);validate(&list,&expected);}
   list.remove(10_000_000);validate(&list,&expected);
   let mut removal=positions.clone();if reverse{removal.reverse();}
   for pos in removal{list.remove(pos);expected.remove(&pos);validate(&list,&expected);}
   for pos in [1_048_575,65536,64,63,0]{list.insert(pos);expected.insert(pos);validate(&list,&expected);list.remove(pos);expected.remove(&pos);validate(&list,&expected);}
  }}
 }
}

#[cfg(test)] mod reverse_entry_oracle {
 use super::*;use std::collections::{BTreeMap,BTreeSet};
 fn row(value:usize,arity:usize,keep:bool)->Row {
  let mut r=Row::new();for field in 0..arity {r.insert(format!("f{field}"),if field%2==0{Cell::text_arc(format!("key-{value}-{}","long".repeat(field*8)))}else{Cell::Int(value as i64)});}r.insert("keep".into(),Cell::Bool(keep));r
 }
 fn expected(index:&LiveIndex,rows:&[Option<Row>])->BTreeMap<IndexKey,BTreeSet<usize>> {
  let mut map:BTreeMap<IndexKey,BTreeSet<usize>>=BTreeMap::new();for(i,row)in rows.iter().enumerate(){if let Some(row)=row {if index.def.pred.as_ref().is_some_and(|p|!crate::exec::eval_pred(p,row,0)){continue;}map.entry(index.key_of(row)).or_default().insert(i);}}map
 }
 fn verify(index:&LiveIndex,rows:&[Option<Row>]) {
  let want=expected(index,rows);let got:BTreeMap<_,_>=index.forward.iter().map(|(k,v)|(k.clone(),v.to_indices().into_iter().collect::<BTreeSet<_>>())).collect();assert_eq!(got,want);
  let reverse=want.iter().flat_map(|(k,ids)|ids.iter().map(move|&i|(i,k.clone()))).collect::<FxHashMap<_,_>>();assert_eq!(index.reverse,reverse);
 }
 #[test] fn held_entry_matches_membership_model_and_keeps_conflicts_atomic(){
  for arity in [1,2,3,5]{for unique in [false,true]{for partial in [false,true]{
   let mut index=LiveIndex::new(IndexDef{collection:"test".into(),fields:(0..arity).map(|i|format!("f{i}")).collect(),unique,pred:partial.then(||crate::parse::parse_pred_src("keep == true").unwrap()),bitmap_card_field:None});
   let mut rows=(0..128).map(|i|Some(row(if unique{i}else{i%3},arity,i%3!=0))).collect::<Vec<_>>();
   for(i,r)in rows.iter().enumerate(){index.insert_at(i,r.as_ref().unwrap()).unwrap();}verify(&index,&rows);
   for step in 0..384 {
    let i=step*17%rows.len();
    if step%9==0 {index.remove_at(i);rows[i]=None;}else{
     let proposed=if step%7==0 {rows[(i+1)%rows.len()].clone().unwrap_or_else(||row(10000+step,arity,true))}else if step%5==0 {rows[i].clone().unwrap_or_else(||row(10000+step,arity,true))}else{row(if unique{10000+step}else{step%3},arity,step%4!=0)};
     let mut next=rows.clone();next[i]=Some(proposed.clone());let want=expected(&index,&next);let conflict=unique&&want.values().any(|ids|ids.len()>1);let forward=index.forward.clone();let reverse=index.reverse.clone();let result=index.insert_at(i,&proposed);assert_eq!(result.is_err(),conflict);
     if conflict {assert_eq!(index.forward,forward);assert_eq!(index.reverse,reverse);}else{rows=next;}
    }
    verify(&index,&rows);
   }
  }}}
 }
}
