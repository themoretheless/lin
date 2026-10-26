#[cfg(test)] mod compact_undo_oracle {
 use super::*;
 #[test] fn exact_roundtrip_all_cell_types_and_name_boundaries(){
 let names=["", "id", "поле", "1234567890123456789012", "12345678901234567890123", "длинное_имя_поля_без_усечения", "nul\0field"];
 let values=[Cell::Null,Cell::Bool(true),Cell::Int(i64::MIN),Cell::Time(i64::MAX),Cell::Float(-0.0),Cell::Float(f64::from_bits(0x7ff8000000000001)),Cell::Vec(Arc::from([0.0f32,-0.0,1.0]))];
 let mut row=Row::new();for(i,k)in names.iter().enumerate(){row.insert(k.to_string(),values[i].clone());}
 row.insert("short_text".into(),Cell::text_arc("ёж"));row.insert("long_text".into(),Cell::text_arc("long shared payload long shared payload"));
 let captured=UndoRow::capture(&row);let expected=row.clone();row.clear();assert_eq!(captured.into_row(),expected);
 }
}
