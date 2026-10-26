#[derive(Default, Clone, Copy)]
struct DictStats { aborted: bool, selected: bool, dictionaries: usize, escapes: usize, legacy_bytes: usize }
fn raw_escape(row: &Option<Arc<[f32]>>, out: &mut Vec<u8>) -> Result<usize, Error> {
    let start = out.len();
    encode_sparse_reference(std::slice::from_ref(row), out)?;
    let legacy = out.len() - start;
    if row.is_some() { out.insert(start + 8, 0); }
    Ok(legacy)
}
fn encode_dictionary(rows: &[Option<Arc<[f32]>>], out: &mut Vec<u8>, force: bool) -> Result<DictStats, Error> {
 encode_dictionary_limited(rows,out,force,usize::MAX)
}
fn encode_dictionary_limited(rows: &[Option<Arc<[f32]>>], out: &mut Vec<u8>, force: bool, max_costly:usize) -> Result<DictStats, Error> {
    let begin = out.len();
    let mut stats = DictStats::default();let mut costly=0usize;
    let dense_budget=rows.iter().map(|r|4usize.saturating_add(r.as_ref().map_or(0,|v|v.len().saturating_mul(4)))).fold(0usize,usize::saturating_add);
    'rows: for (row_index,row) in rows.iter().enumerate() {
        let Some(values) = row else {
            out.extend_from_slice(&0u32.to_le_bytes()); stats.legacy_bytes += 4; continue;
        };
        if max_costly!=usize::MAX {
            let too_large=values.len().saturating_mul(8).saturating_add(9)>dense_budget.saturating_sub(out.len()-begin);
            let dense=values.len()>=8 && reference::mask(&values[..8]).count_ones()>=6 && values.iter().filter(|x|x.to_bits()!=0).count()>values.len()/4;
            if too_large || (dense && row_index<rows.len()/2) {out.truncate(begin);stats.aborted=true;return Ok(stats);}
            if dense {stats.legacy_bytes+=raw_escape(row,out)?;stats.escapes+=1;costly+=1;if costly>=max_costly{out.truncate(begin);stats.aborted=true;return Ok(stats);}continue;}
        }
        let dim = u32::try_from(values.len()).ok().and_then(|n| n.checked_add(1)).ok_or_else(|| io_err("dimension overflow"))?;
        let start = out.len();
        out.extend_from_slice(&dim.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(0);
        let entries_start = out.len() + 64;
        out.resize(entries_start, 0);
        let mut dictionary = [0u32;16]; let mut d = 0usize; let mut count = 0u32;
        let narrow = values.len() <= 65536;
        for (block, chunk) in values.chunks(8).enumerate() {
            let mut mask=reference::mask(chunk);
            while mask!=0 {
                let offset=mask.trailing_zeros() as usize;mask&=mask-1;
                let bits = chunk[offset].to_bits();
                // Unused slots are zero; bits is nonzero, so prefix probes need no d guard.
                let found = if dictionary[0] == bits {Some(0)}
                    else if dictionary[1] == bits {Some(1)}
                    else if dictionary[2] == bits {Some(2)}
                    else if dictionary[3] == bits {Some(3)}
                    else {dictionary[4..d.max(4)].iter().position(|&x|x==bits).map(|i|i+4)};
                let code = if let Some(code) = found { code } else {
                    if d == 16 {
                        out.truncate(start); stats.legacy_bytes += raw_escape(row,out)?; stats.escapes += 1;costly+=1;if costly>=max_costly{out.truncate(begin);stats.aborted=true;return Ok(stats);}continue 'rows;
                    }
                    dictionary[d] = bits; d += 1; d-1
                };
                let index = (block*8+offset) as u32;
                if narrow {
                    let bytes = (index as u16).to_le_bytes();
                    out.extend_from_slice(&[bytes[0],bytes[1],code as u8]);
                } else {
                    let bytes = index.to_le_bytes();
                    out.extend_from_slice(&[bytes[0],bytes[1],bytes[2],bytes[3],code as u8]);
                }
                count += 1;
            }
        }
        let legacy = 8 + count as usize*8;
        let dictionary_bytes = 9 + d*4 + count as usize*if narrow {3} else {5};
        if d == 0 || dictionary_bytes >= legacy {
            out.truncate(start); stats.legacy_bytes += raw_escape(row,out)?; stats.escapes += 1; continue;
        }
        out[start+4..start+8].copy_from_slice(&count.to_le_bytes()); out[start+8] = d as u8;
        for (i,bits) in dictionary[..d].iter().enumerate() { out[start+9+i*4..start+13+i*4].copy_from_slice(&bits.to_le_bytes()); }
        let end = out.len();
        out.copy_within(entries_start..end, start+9+d*4);
        out.truncate(start+dictionary_bytes);
        stats.legacy_bytes += legacy; stats.dictionaries += 1;
    }
    stats.selected = force || out.len()-begin < stats.legacy_bytes;
    if !stats.selected { out.truncate(begin); encode_sparse_reference(rows,out)?; }
    Ok(stats)
}
// Trusted diagnostic parser, NOT the production untrusted-input decoder.
fn decode(bytes: &[u8], rows: usize, dictionary_mode: bool) -> Vec<Option<Vec<u32>>> {
    fn take<'a>(bytes: &mut &'a [u8], n: usize) -> &'a [u8] { let (a,b)=bytes.split_at(n); *bytes=b; a }
    fn u32v(bytes: &mut &[u8]) -> u32 { u32::from_le_bytes(take(bytes,4).try_into().unwrap()) }
    let mut input=bytes; let mut result=Vec::new();
    for _ in 0..rows {
        let code=u32v(&mut input); if code==0 {result.push(None);continue;}
        let dim=(code-1) as usize; let count=u32v(&mut input) as usize; assert!(count<=dim);
        let d=if dictionary_mode {take(&mut input,1)[0] as usize} else {0}; assert!(d<=16);
        let dict=(0..d).map(|_|u32v(&mut input)).collect::<Vec<_>>();
        assert!(dict.iter().all(|&x|x!=0));
        let mut values=vec![0u32;dim]; let mut previous=None;
        for _ in 0..count {
            let index=if d>0 && dim<=65536 {u16::from_le_bytes(take(&mut input,2).try_into().unwrap()) as usize} else {u32v(&mut input) as usize};
            assert!(index<dim && previous.is_none_or(|p|p<index)); previous=Some(index);
            let bits=if d>0 {let ordinal=take(&mut input,1)[0] as usize; dict[ordinal]} else {u32v(&mut input)};
            assert_ne!(bits,0);values[index]=bits;
        }
        result.push(Some(values));
    }
    assert!(input.is_empty()); result
}
fn expected(rows: &[Option<Arc<[f32]>>]) -> Vec<Option<Vec<u32>>> {
    rows.iter().map(|v|v.as_ref().map(|v|v.iter().map(|x|x.to_bits()).collect())).collect()
}
fn check(rows: &[Option<Arc<[f32]>>]) -> (usize,usize,DictStats) {
    let mut old=Vec::new(); encode_sparse_reference(rows,&mut old).unwrap();
    let mut new=Vec::new(); let stats=encode_dictionary(rows,&mut new,false).unwrap();
    assert_eq!(stats.legacy_bytes,old.len());
    assert_eq!(decode(&old,rows.len(),false),expected(rows));
    assert_eq!(decode(&new,rows.len(),stats.selected),expected(rows));
    let mut forced=vec![1,2,3];let forced_stats=encode_dictionary(rows,&mut forced,true).unwrap();assert!(forced_stats.selected);
    assert_eq!(&forced[..3],&[1,2,3]);assert_eq!(decode(&forced[3..],rows.len(),true),expected(rows));
    let mut a=Vec::new();let old_mode=encode_current_choice(rows,&mut a).unwrap();
    let mut b=Vec::new();let (new_mode,actual_stats)=encode_proposed_choice(rows,&mut b).unwrap();
    assert_eq!(decode_choice(&a,rows.len(),old_mode),expected(rows));
    assert_eq!(decode_choice(&b,rows.len(),new_mode),expected(rows));
    assert_eq!(actual_stats.selected,new_mode==2);
    (a.len(),b.len(),actual_stats)
}

// Existing classifier + actual serial/parallel sparse writer; no field/frame overhead here.
fn encode_dense_reference(rows: &[Option<Arc<[f32]>>], buf: &mut Vec<u8>) {
                for row in rows {
                    match row {
                        Some(emb) if !emb.is_empty() => {
                            buf.extend_from_slice(&(emb.len() as u32).to_le_bytes());
                            #[cfg(target_endian = "little")]
                            {
                                // Safety: [f32] on little-endian has the exact IEEE 754 LE byte representation.
                                let bytes = unsafe {
                                    std::slice::from_raw_parts(
                                        emb.as_ptr() as *const u8,
                                        emb.len() * std::mem::size_of::<f32>(),
                                    )
                                };
                                buf.extend_from_slice(bytes);
                            }
                            #[cfg(not(target_endian = "little"))]
                            {
                                for x in emb.iter() {
                                    buf.extend_from_slice(&x.to_le_bytes());
                                }
                            }
                        }
                        _ => buf.extend_from_slice(&0u32.to_le_bytes()),
                    }
                }
}
fn encode_current_choice(rows: &[Option<Arc<[f32]>>], buf: &mut Vec<u8>) -> Result<u8, Error> {
    if use_sparse_vectors(rows) {
        if rows.len() >= 4096 && rows.iter().flatten().fold(0usize,|n,v|n.saturating_add(v.len())) >= 2*1024*1024 {
            append_sparse_rows_parallel(rows,buf)?;
        } else { encode_sparse_reference(rows,buf)?; }
        Ok(1)
    } else {encode_dense_reference(rows,buf);Ok(0)}
}
fn encode_proposed_choice(rows: &[Option<Arc<[f32]>>], buf: &mut Vec<u8>) -> Result<(u8,DictStats), Error> {
    let dense=rows.iter().map(|v|4+v.as_ref().map_or(0,|v|v.len()*4)).sum::<usize>();
    if rows.len()<128 || rows.len()>=4096 || !(64*1024..=8*1024*1024).contains(&dense) {
        return Ok((encode_current_choice(rows,buf)?,DictStats::default()));
    }
    // Cheap entropy eligibility probe; keep existing path for high-cardinality controls.
    let mut eligible=false;
    for slot in [0, rows.len()/3, rows.len()*2/3, rows.len()-1] {
        let Some(row)=rows[slot].as_ref() else {continue;};
        let mut seen=[0u32;16];let mut d=0;let mut count=0;
        for value in row.iter() {
            let bits=value.to_bits();if bits==0 {continue;}count+=1;if count>row.len()/4{return Ok((encode_current_choice(rows,buf)?,DictStats::default()));}
            if !seen[..d].contains(&bits) {if d==16 {return Ok((encode_current_choice(rows,buf)?,DictStats::default()));}seen[d]=bits;d+=1;}
        }
        if d>0 && 9+4*d+count*if row.len()<=65536 {3}else{5}<8+count*8 {eligible=true;}
    }
    if !eligible {return Ok((encode_current_choice(rows,buf)?,DictStats::default()));}
    let begin=buf.len();let stats=encode_dictionary_limited(rows,buf,true,2)?;if stats.aborted{return Ok((encode_current_choice(rows,buf)?,stats));}
    let bytes=buf.len()-begin;
    // Require >=20% vector-section savings; nonbeneficial mixed cases retain old choice.
    if bytes*5 < stats.legacy_bytes.min(dense)*4 {return Ok((2,stats));}
    buf.truncate(begin);Ok((encode_current_choice(rows,buf)?,DictStats::default()))
}
fn decode_choice(bytes:&[u8], rows:usize, mode:u8) -> Vec<Option<Vec<u32>>> {
    if mode!=0 {return decode(bytes,rows,mode==2);}
    let mut input=bytes;let mut out=Vec::new();
    for _ in 0..rows {
        let n=u32::from_le_bytes(input[..4].try_into().unwrap()) as usize;input=&input[4..];
        if n==0 {out.push(None);continue;}
        let values=input[..n*4].chunks_exact(4).map(|b|u32::from_le_bytes(b.try_into().unwrap())).collect();
        input=&input[n*4..];out.push(Some(values));
    }
    assert!(input.is_empty());out
}
fn fixture(n:usize) -> Vec<Option<Arc<[f32]>>> {
    let source=format!("insert docs [{}]",(0..n).map(|i|{
        let title=if i%10==0 {format!("doc {i} wal note")} else {format!("doc {i} plain")};
        let wing=if i%2==0 {"rag"}else{"sys"};
        format!(r#"{{id:"d-{i}",uri:"bench://{i}",title:"{title}",layer:"wiki",wing:"{wing}",body:"body {i}",ts:timestamp({})}}"#,1700000000000i64-if i%2==0 {86400000}else{30*86400000})
    }).collect::<Vec<_>>().join(","));
    let mut db=Db::empty();db.run("index docs [wing, ts]").unwrap();assert_eq!(db.run(&source).unwrap().done.n,n);
    db.store.collection("docs").iter().map(|r|{let Cell::Vec(v)=&r["embedding"] else {panic!("embedding")};assert_eq!(v.len(),768);Some(v.clone())}).collect()
}
