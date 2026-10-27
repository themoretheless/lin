
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
