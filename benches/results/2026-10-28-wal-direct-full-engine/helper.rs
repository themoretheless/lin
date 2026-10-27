mod wal_direct_zero {
    use super::{LogRecord, MAX_RECORD, Pack, io_err};
    use crate::{Cell, Error};
    fn uint(b: &mut Vec<u8>, n: u64) {
        match n {
            0..=127 => b.push(n as u8),
            128..=255 => b.extend_from_slice(&[0xcc, n as u8]),
            256..=65535 => {
                b.push(0xcd);
                b.extend_from_slice(&(n as u16).to_be_bytes());
            }
            65536..=4294967295 => {
                b.push(0xce);
                b.extend_from_slice(&(n as u32).to_be_bytes());
            }
            _ => {
                b.push(0xcf);
                b.extend_from_slice(&n.to_be_bytes());
            }
        }
    }
    fn sint(b: &mut Vec<u8>, n: i64) {
        if n >= 0 {
            uint(b, n as u64);
        } else if n >= -32 {
            b.push(n as u8);
        } else if n >= -128 {
            b.extend_from_slice(&[0xd0, n as u8]);
        } else if n >= -32768 {
            b.push(0xd1);
            b.extend_from_slice(&(n as i16).to_be_bytes());
        } else if n >= -2147483648 {
            b.push(0xd2);
            b.extend_from_slice(&(n as i32).to_be_bytes());
        } else {
            b.push(0xd3);
            b.extend_from_slice(&n.to_be_bytes());
        }
    }
    fn str_(b: &mut Vec<u8>, v: &str) -> Result<(), Error> {
        let n = u32::try_from(v.len()).map_err(|_| io_err("log record exceeds 16MiB"))?;
        match n {
            0..=31 => b.push(0xa0 | n as u8),
            32..=255 => b.extend_from_slice(&[0xd9, n as u8]),
            256..=65535 => {
                b.push(0xda);
                b.extend_from_slice(&(n as u16).to_be_bytes());
            }
            _ => {
                b.push(0xdb);
                b.extend_from_slice(&n.to_be_bytes());
            }
        }
        b.extend_from_slice(v.as_bytes());
        Ok(())
    }
    fn seq(b: &mut Vec<u8>, n: usize, map: bool) -> Result<(), Error> {
        let n = u32::try_from(n).map_err(|_| io_err("log record exceeds 16MiB"))?;
        if n <= 15 {
            b.push((if map { 0x80 } else { 0x90 }) | n as u8);
        } else if n <= 65535 {
            b.push(if map { 0xde } else { 0xdc });
            b.extend_from_slice(&(n as u16).to_be_bytes());
        } else {
            b.push(if map { 0xdf } else { 0xdd });
            b.extend_from_slice(&n.to_be_bytes());
        }
        Ok(())
    }
    fn cell(b: &mut Vec<u8>, c: &Cell, savings: &mut usize) -> Result<(), Error> {
        if matches!(c, Cell::Null) {
            b.extend_from_slice(b"\x81\xa1t\xa4Null");
            return Ok(());
        }
        b.extend_from_slice(b"\x82\xa1t");
        str_(
            b,
            match c {
                Cell::Text(_) => "Text",
                Cell::SmallText(_) => "SmallText",
                Cell::Int(_) => "Int",
                Cell::Float(_) => "Float",
                Cell::Bool(_) => "Bool",
                Cell::Time(_) => "Time",
                Cell::Vec(_) => "Vec",
                Cell::Null => unreachable!(),
            },
        )?;
        b.extend_from_slice(b"\xa1v");
        match c {
            Cell::Text(v) => str_(b, v)?,
            Cell::SmallText(v) => str_(b, v.as_str())?,
            Cell::Int(v) | Cell::Time(v) => sint(b, *v),
            Cell::Float(v) => {
                b.push(0xcb);
                b.extend_from_slice(&v.to_bits().to_be_bytes());
            }
            Cell::Bool(v) => b.push(if *v { 0xc3 } else { 0xc2 }),
            Cell::Vec(v) => {
                seq(b, v.len(), false)?;
                for x in v.iter() {
                    if x.to_bits() == 0 {
                        b.push(0);
                        *savings = savings.saturating_add(4);
                    } else {
                        b.push(0xca);
                        b.extend_from_slice(&x.to_bits().to_be_bytes());
                    }
                }
            }
            Cell::Null => unreachable!(),
        };
        Ok(())
    }
    pub(super) fn encode(b: &mut Vec<u8>, r: &LogRecord) -> Result<(), Error> {
        let (collection, rows, kind) = match &r.pack {
            Pack::Update { collection, rows } => (collection, rows, "update"),
            Pack::Delete { collection, rows } => (collection, rows, "delete"),
            _ => {
                rmp_serde::encode::write_named(&mut *b, r).map_err(io_err)?;
                if b.len()
                    .checked_add(5)
                    .is_none_or(|n| n > MAX_RECORD as usize)
                {
                    return Err(io_err("log record exceeds 16MiB"));
                }
                return Ok(());
            }
        };
        b.extend_from_slice(b"\x83\xa3gen");
        uint(b, r.r#gen);
        str_(b, "next_id")?;
        uint(b, r.next_id);
        str_(b, "pack")?;
        b.push(0x83);
        str_(b, "type")?;
        str_(b, kind)?;
        str_(b, "collection")?;
        str_(b, collection)?;
        str_(b, "rows")?;
        seq(b, rows.len(), false)?;
        let mut savings = 0usize;
        for row in rows {
            seq(b, row.len(), true)?;
            for (key, value) in row {
                str_(b, key)?;
                cell(b, value, &mut savings)?;
            }
        }
        if b.len()
            .checked_add(savings)
            .and_then(|n| n.checked_add(5))
            .is_none_or(|n| n > MAX_RECORD as usize)
        {
            return Err(io_err("log record exceeds 16MiB"));
        }
        Ok(())
    }
}
