use crate::ast::{Pred,Field,Value,CmpOp};
use crate::catalog::IndexDef;
use crate::index::{IndexUse,index_covers_pred};
pub struct Probe { pred: Pred, uses: Vec<IndexUse> }
impl Probe {
 pub fn new(shape:&str)->Self {
  let name=if shape=="dotted"{"nested.key"}else{"key"};
  let atom=Pred::Cmp{field:Field::path(name),op:CmpOp::Eq,value:Value::String("rag".into())};
  let other=Pred::Cmp{field:Field::name("other"),op:CmpOp::Eq,value:Value::Int(7)};
  let pred=if shape=="compound"{Pred::And(Box::new(atom.clone()),Box::new(other.clone()))}else{atom.clone()};
  let partial=if shape=="partial"{Some(Pred::And(Box::new(atom),Box::new(other)))}else{None};
  let uses=vec![IndexUse{def:IndexDef{collection:"docs".into(),unique:false,fields:vec![name.into(),"other".into()],pred:partial,bitmap_card_field:None},eqs:vec![(name.into(),Value::String("rag".into())),("other".into(),Value::Int(7))],range:None}];Self{pred,uses}
 }
 pub fn run(&self)->bool{index_covers_pred(&self.pred,&self.uses)}
}
