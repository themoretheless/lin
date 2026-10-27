use lin::{Cell,Row};
use std::collections::BTreeMap;
fn accepts_map(_: &BTreeMap<String,Cell>) {}
fn traits<T: Eq+std::hash::Hash+Send+Sync>() {}
fn main(){
    traits::<Row>();
    let mut row=Row::from(BTreeMap::from([("id".to_owned(),Cell::from("a"))]));
    accepts_map(&row);
    row.entry("title".to_owned()).or_insert(Cell::from("hello"));
    let _: Vec<(String,Cell)>=row.into_iter().collect();
}
