pub mod row_copy_probe {
 use crate::store::{Row,Cell};use std::sync::atomic::{AtomicU8,Ordering::Relaxed};
 static MODE:AtomicU8=AtomicU8::new(0);
 pub fn set_mode(mode:u8){assert!(mode<=2);MODE.store(mode,Relaxed);}
 #[inline(never)] pub fn copy(row:&Row)->Row {match MODE.load(Relaxed){0=>row.clone(),1=>row.iter().map(|(k,v)|(k.clone(),v.clone())).collect(),2=>staged(row),_=>unreachable!()}}
 #[inline(never)] fn staged(row:&Row)->Row {
  let keys:smallvec::SmallVec<[String;16]>={let _span=crate::phase_probe::Span::new(25);row.keys().cloned().collect()};
  let values:smallvec::SmallVec<[Cell;16]>={let _span=crate::phase_probe::Span::new(26);row.values().cloned().collect()};
  let _span=crate::phase_probe::Span::new(27);keys.into_iter().zip(values).collect()
 }
 fn exact(a:&Row,b:&Row){
  assert_eq!(a.len(),b.len());for((ka,a),(kb,b))in a.iter().zip(b){assert_eq!(ka,kb);match(a,b){
   (Cell::Float(a),Cell::Float(b))=>assert_eq!(a.to_bits(),b.to_bits()),
   (Cell::Vec(a),Cell::Vec(b))=>{assert_eq!(a.len(),b.len());assert!(a.iter().zip(b.iter()).all(|(a,b)|a.to_bits()==b.to_bits()));},
   _=>assert_eq!(a,b),
  }}
 }
 pub fn preflight(){
  for fields in [0,1,8,11,12,16,17,32,129]{let mut row=Row::new();for i in 0..fields{row.insert(format!("field-{i:04}-ёж"),match i%8{0=>Cell::Null,1=>Cell::Text(std::sync::Arc::from("owned text")),2=>Cell::SmallText(crate::store::InlineStr::try_new("inline").unwrap()),3=>Cell::Int(i64::MIN),4=>Cell::Float(f64::from_bits(0x7ff8000000000042)),5=>Cell::Bool(true),6=>Cell::Time(i64::MAX),_=>Cell::Vec(std::sync::Arc::from([f32::from_bits(0x7fc00042),-0.0,f32::INFINITY]))});}
   for mode in 0..=2{set_mode(mode);let mut copied=copy(&row);exact(&row,&copied);assert_eq!(rmp_serde::to_vec(&row).unwrap(),rmp_serde::to_vec(&copied).unwrap());copied.insert("extra".into(),Cell::Null);assert!(!row.contains_key("extra"));}
  }
 }
}
