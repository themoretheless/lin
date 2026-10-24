use lin::{Cell,Row,Value};
use std::{collections::BTreeMap,hint::black_box,time::Instant};
#[derive(Clone,Debug,PartialEq)]struct Record{fields:Vec<(String,Value)>}
fn value_cell(v: &Value, now: i64) -> Cell {
    match v {
        Value::String(s) => Cell::Text(std::sync::Arc::clone(s)),
        Value::Int(n) => Cell::Int(*n),
        Value::Float(n) => Cell::Float(*n),
        Value::Bool(b) => Cell::Bool(*b),
        Value::Timestamp(millis) => Cell::Time(*millis),
        Value::Now => Cell::Time(now),
        Value::NowMinus(d) => Cell::Time(now - d.as_millis()),
        Value::Duration(d) => Cell::Int(d.as_millis()),
        Value::Name(n) => Cell::text_arc(n.as_str()),
    }
}
fn append_batch_shape_rows(rows: &mut Vec<Row>, records: &[Record], now: i64) {
    let Some(first) = records.first() else {
        return;
    };
    if records.len() < 128 || first.fields.is_empty() || first.fields.len() > 32 {
        rows.extend(records.iter().map(|record| record_row(record, now)));
        return;
    }
    let template = first
        .fields
        .iter()
        .map(|(key, _)| (key.clone(), Cell::Null))
        .collect::<Row>();
    let slots = template
        .keys()
        .map(|key| {
            first
                .fields
                .iter()
                .rposition(|(field, _)| field == key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    for record in records {
        if record.fields.len() != first.fields.len()
            || !record
                .fields
                .iter()
                .map(|(key, _)| key.as_str())
                .eq(first.fields.iter().map(|(key, _)| key.as_str()))
        {
            rows.push(record_row(record, now));
            continue;
        }
        let mut row = template.clone();
        for (cell, &source) in row.values_mut().zip(&slots) {
            *cell = value_cell(&record.fields[source].1, now);
        }
        rows.push(row);
    }
}

fn record_row(record: &Record, now: i64) -> Row {
    let mut row = BTreeMap::new();
    for (k, v) in &record.fields {
        row.insert(k.clone(), value_cell(v, now));
    }
    row
}
fn append_collect_rows(rows: &mut Vec<Row>, records: &[Record], now: i64) {
    let Some(first) = records.first() else {
        return;
    };
    if records.len() < 128 || first.fields.is_empty() || first.fields.len() > 32 {
        rows.extend(records.iter().map(|record| record_row(record, now)));
        return;
    }
    let template = first
        .fields
        .iter()
        .map(|(key, _)| (key.clone(), Cell::Null))
        .collect::<Row>();
    let slots = template
        .keys()
        .map(|key| {
            first
                .fields
                .iter()
                .rposition(|(field, _)| field == key)
                .unwrap()
        })
        .collect::<Vec<_>>();
    for record in records {
        if record.fields.len() != first.fields.len()
            || !record
                .fields
                .iter()
                .map(|(key, _)| key.as_str())
                .eq(first.fields.iter().map(|(key, _)| key.as_str()))
        {
            rows.push(record_row(record, now));
            continue;
        }
        let row = template.keys().zip(&slots)
            .map(|(key,&source)|(key.clone(),value_cell(&record.fields[source].1,now)))
            .collect::<Row>();
        rows.push(row);
    }
}

fn fixture(n:usize)->Vec<Record>{(0..n).map(|i|Record{fields:vec![("id".into(),Value::String(format!("d-{i}").into())),("uri".into(),Value::String(format!("bench://{i}").into())),("title".into(),Value::String(format!("doc {i} plain").into())),("layer".into(),Value::String("wiki".into())),("wing".into(),Value::String("rag".into())),("body".into(),Value::String(format!("body {i}").into())),("ts".into(),Value::Timestamp(1700000000000))]}).collect()}
fn main(){let process:usize=std::env::args().nth(1).unwrap().parse().unwrap();
 for n in [0,1,127,128,129,4095,4096]{for width in [0,1,7,32,33]{let records:Vec<Record>=(0..n).map(|i|Record{fields:(0..width).map(|j|(format!("f{}",j%5),if j%3==0{Value::Now}else{Value::Int((i+j)as i64)})).collect()}).collect();let copy=records.clone();for now in [1,1700000000000]{let expected:Vec<Row>=records.iter().map(|r|record_row(r,now)).collect();for f in [append_batch_shape_rows,append_collect_rows]{let mut got=vec![Row::new()];f(&mut got,&records,now);assert_eq!(&got[1..],expected);}}assert_eq!(records,copy);}}
 let mut mixed=fixture(1000);for (i,r) in mixed.iter_mut().enumerate(){if i%7==0{r.fields.reverse();}if i%11==0{r.fields.push(("title".into(),Value::String("last duplicate".into())));}}
 for(name,records,loops)in[("default_1k",fixture(1000),32),("default_10k",fixture(10000),4),("heterogeneous_1k",mixed,32),("small_127",fixture(127),128)]{
 let expected:Vec<Row>=records.iter().map(|r|record_row(r,123)).collect();let funcs:[fn(&mut Vec<Row>,&[Record],i64);2]=[append_batch_shape_rows,append_collect_rows];for f in funcs{let mut got=Vec::new();f(&mut got,&records,123);assert_eq!(got,expected);}drop(expected);let mut ns=vec![Vec::<f64>::new();2];
 for round in 0..24{for pos in 0..2{let v=(pos+round+process)%2;let t=Instant::now();for _ in 0..loops{let mut out=Vec::with_capacity(records.len());funcs[v](&mut out,black_box(&records),123);black_box(&out);drop(out);}ns[v].push(t.elapsed().as_nanos()as f64/loops as f64);}}
 println!("{}",serde_json::json!({"case":name,"ns":ns,"exact_rows":true}));
 }}
