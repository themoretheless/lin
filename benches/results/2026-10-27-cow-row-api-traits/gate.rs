use lin::{Row,Cell};
use std::collections::{BTreeMap,HashSet};
use std::hash::{Hash,Hasher};
fn traits<T:Eq+Hash+Send+Sync+std::panic::UnwindSafe+std::panic::RefUnwindSafe>(){}
fn digest<T:Hash>(v:&T)->u64{let mut h=std::collections::hash_map::DefaultHasher::new();v.hash(&mut h);h.finish()}
fn main(){
 traits::<Row>();
 let original:Row=[("name".to_owned(),Cell::text_arc("ёж shared payload longer than twenty two bytes")),("number".to_owned(),Cell::Float(1.25)),("nil".to_owned(),Cell::Null)].into_iter().collect();
 let map:BTreeMap<String,Cell>=original.iter().map(|(k,v)|(k.clone(),v.clone())).collect();
 assert_eq!(digest(&original),digest(&map));
 let mut set=HashSet::new();set.insert(original.clone());
 let mut changed=original.clone();changed.insert("number".to_owned(),Cell::Float(2.5));
 assert!(set.contains(&original));assert!(!set.contains(&changed));assert_eq!(original["number"],Cell::Float(1.25));
 assert_eq!(serde_json::to_vec(&original).unwrap(),serde_json::to_vec(&map).unwrap());
 assert_eq!(rmp_serde::to_vec(&original).unwrap(),rmp_serde::to_vec(&map).unwrap());
 println!("Eq/Hash/Send/Sync/unwind traits, map hash equivalence, HashSet clone isolation and wire bytes passed");
}
