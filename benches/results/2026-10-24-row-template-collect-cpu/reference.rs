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
