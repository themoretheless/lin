use std::cell::RefCell;
use std::io::{Read, Write};
use std::sync::Arc;

use crate::error::Error;
use crate::exact_sum::ExactSum;
use crate::store::{Cell, InlineStr};

const MAX_SPILL_STR_BYTES: usize = 64 * 1024 * 1024;
const MAX_SPILL_VEC_LEN: usize = 16 * 1024 * 1024;

thread_local! {
    static SPILL_SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static LAST_SPILL_TEXT: RefCell<Option<Arc<str>>> = const { RefCell::new(None) };
}

#[inline]
fn io_err(e: std::io::Error) -> Error {
    Error::runtime(e.to_string())
}

pub trait Spillable: Sized {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error>;
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error>;
}

impl Spillable for () {
    #[inline]
    fn serialize(&self, _writer: &mut impl Write) -> Result<(), Error> {
        Ok(())
    }

    #[inline]
    fn deserialize(_reader: &mut impl Read) -> Result<Self, Error> {
        Ok(())
    }
}

impl Spillable for ExactSum {
    #[inline]
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        let (int, float, comp) = self.into_parts();
        let mut buf = [0u8; 32];
        buf[..16].copy_from_slice(&int.to_le_bytes());
        buf[16..24].copy_from_slice(&float.to_le_bytes());
        buf[24..32].copy_from_slice(&comp.to_le_bytes());
        writer.write_all(&buf).map_err(io_err)
    }

    #[inline]
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf).map_err(io_err)?;
        let int = i128::from_le_bytes(buf[..16].try_into().unwrap());
        let float = f64::from_le_bytes(buf[16..24].try_into().unwrap());
        let comp = f64::from_le_bytes(buf[24..32].try_into().unwrap());
        Ok(ExactSum::from_parts(int, float, comp))
    }
}

impl Spillable for Cell {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        match self {
            Cell::Null => writer.write_all(&[0u8]).map_err(io_err),
            Cell::Text(s) => {
                let b = s.as_bytes();
                if b.len() <= InlineStr::MAX_LEN {
                    let mut buf = [0u8; 2 + InlineStr::MAX_LEN];
                    buf[0] = 2;
                    buf[1] = b.len() as u8;
                    buf[2..2 + b.len()].copy_from_slice(b);
                    return writer.write_all(&buf[..2 + b.len()]).map_err(io_err);
                }
                let len = u32::try_from(b.len())
                    .map_err(|_| Error::runtime("spill text exceeds u32 length"))?;
                if b.len() <= 250 {
                    let mut buf = [0u8; 256];
                    buf[0] = 1;
                    buf[1..5].copy_from_slice(&len.to_le_bytes());
                    buf[5..5 + b.len()].copy_from_slice(b);
                    return writer.write_all(&buf[..5 + b.len()]).map_err(io_err);
                }
                let mut hdr = [0u8; 5];
                hdr[0] = 1;
                hdr[1..5].copy_from_slice(&len.to_le_bytes());
                writer.write_all(&hdr).map_err(io_err)?;
                writer.write_all(b).map_err(io_err)
            }
            Cell::SmallText(s) => {
                let b = s.as_bytes();
                let mut buf = [0u8; 2 + InlineStr::MAX_LEN];
                buf[0] = 2;
                buf[1] = b.len() as u8;
                buf[2..2 + b.len()].copy_from_slice(b);
                writer.write_all(&buf[..2 + b.len()]).map_err(io_err)
            }
            Cell::Int(n) => {
                let mut buf = [0u8; 9];
                buf[0] = 3;
                buf[1..9].copy_from_slice(&n.to_le_bytes());
                writer.write_all(&buf).map_err(io_err)
            }
            Cell::Float(f) => {
                let mut buf = [0u8; 9];
                buf[0] = 4;
                buf[1..9].copy_from_slice(&f.to_le_bytes());
                writer.write_all(&buf).map_err(io_err)
            }
            Cell::Bool(b) => writer.write_all(&[5u8, u8::from(*b)]).map_err(io_err),
            Cell::Time(t) => {
                let mut buf = [0u8; 9];
                buf[0] = 6;
                buf[1..9].copy_from_slice(&t.to_le_bytes());
                writer.write_all(&buf).map_err(io_err)
            }
            Cell::Vec(v) => {
                let len = u32::try_from(v.len())
                    .map_err(|_| Error::runtime("spill vector exceeds u32 length"))?;
                #[cfg(target_endian = "little")]
                {
                    let byte_len = v.len() * std::mem::size_of::<f32>();
                    // SAFETY: `[f32]` on little-endian targets has exact IEEE-754 LE byte layout.
                    let raw = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, byte_len) };
                    if byte_len <= 250 {
                        let mut buf = [0u8; 256];
                        buf[0] = 7;
                        buf[1..5].copy_from_slice(&len.to_le_bytes());
                        buf[5..5 + byte_len].copy_from_slice(raw);
                        return writer.write_all(&buf[..5 + byte_len]).map_err(io_err);
                    }
                    let mut hdr = [0u8; 5];
                    hdr[0] = 7;
                    hdr[1..5].copy_from_slice(&len.to_le_bytes());
                    writer.write_all(&hdr).map_err(io_err)?;
                    writer.write_all(raw).map_err(io_err)
                }
                #[cfg(not(target_endian = "little"))]
                {
                    let mut hdr = [0u8; 5];
                    hdr[0] = 7;
                    hdr[1..5].copy_from_slice(&len.to_le_bytes());
                    writer.write_all(&hdr).map_err(io_err)?;
                    let mut chunk_buf = [0u8; 256];
                    for chunk in v.chunks(64) {
                        let byte_len = chunk.len() * 4;
                        for (i, &x) in chunk.iter().enumerate() {
                            chunk_buf[i * 4..i * 4 + 4].copy_from_slice(&x.to_le_bytes());
                        }
                        writer.write_all(&chunk_buf[..byte_len]).map_err(io_err)?;
                    }
                    Ok(())
                }
            }
        }
    }

    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut tag = [0u8; 1];
        reader.read_exact(&mut tag).map_err(io_err)?;
        match tag[0] {
            0 => Ok(Cell::Null),
            1 => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf).map_err(io_err)?;
                let len = u32::from_le_bytes(len_buf) as usize;
                if len > MAX_SPILL_STR_BYTES {
                    return Err(Error::runtime("corrupt spill text length"));
                }
                if len <= InlineStr::MAX_LEN {
                    let mut buf = [0u8; InlineStr::MAX_LEN];
                    reader.read_exact(&mut buf[..len]).map_err(io_err)?;
                    let s = std::str::from_utf8(&buf[..len])
                        .map_err(|e| Error::runtime(e.to_string()))?;
                    return Ok(Cell::from_text(s));
                }
                SPILL_SCRATCH.with(|cell| {
                    let mut scratch = cell.borrow_mut();
                    scratch.resize(len, 0);
                    reader.read_exact(&mut scratch[..len]).map_err(io_err)?;
                    let s = std::str::from_utf8(&scratch[..len])
                        .map_err(|e| Error::runtime(e.to_string()))?;
                    let arc = LAST_SPILL_TEXT.with(|last| {
                        let mut slot = last.borrow_mut();
                        if let Some(prev) = slot.as_ref()
                            && prev.as_ref() == s
                        {
                            return Arc::clone(prev);
                        }
                        let created: Arc<str> = Arc::from(s);
                        if len <= 256 {
                            *slot = Some(Arc::clone(&created));
                        }
                        created
                    });
                    Ok(Cell::Text(arc))
                })
            }
            2 => {
                let mut len_buf = [0u8; 1];
                reader.read_exact(&mut len_buf).map_err(io_err)?;
                let len = len_buf[0] as usize;
                if len > InlineStr::MAX_LEN {
                    return Err(Error::runtime("corrupt inline string length in spill"));
                }
                let mut buf = [0u8; InlineStr::MAX_LEN];
                reader.read_exact(&mut buf[..len]).map_err(io_err)?;
                let s = std::str::from_utf8(&buf[..len])
                    .map_err(|e| Error::runtime(e.to_string()))?;
                Ok(Cell::SmallText(
                    InlineStr::try_new(s)
                        .ok_or_else(|| Error::runtime("invalid inline string"))?,
                ))
            }
            3 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(io_err)?;
                Ok(Cell::Int(i64::from_le_bytes(buf)))
            }
            4 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(io_err)?;
                Ok(Cell::Float(f64::from_le_bytes(buf)))
            }
            5 => {
                let mut buf = [0u8; 1];
                reader.read_exact(&mut buf).map_err(io_err)?;
                Ok(Cell::Bool(buf[0] != 0))
            }
            6 => {
                let mut buf = [0u8; 8];
                reader.read_exact(&mut buf).map_err(io_err)?;
                Ok(Cell::Time(i64::from_le_bytes(buf)))
            }
            7 => {
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf).map_err(io_err)?;
                let len = u32::from_le_bytes(len_buf) as usize;
                if len > MAX_SPILL_VEC_LEN {
                    return Err(Error::runtime("corrupt spill vector length"));
                }
                let byte_len = len * 4;
                #[cfg(target_endian = "little")]
                {
                    let mut v = vec![0.0f32; len];
                    if byte_len > 0 {
                        // SAFETY: `v` has `len` initialized `f32` elements (`byte_len` bytes),
                        // and every 4-byte pattern is a valid `f32` bit pattern.
                        let raw = unsafe {
                            std::slice::from_raw_parts_mut(v.as_mut_ptr() as *mut u8, byte_len)
                        };
                        reader.read_exact(raw).map_err(io_err)?;
                    }
                    Ok(Cell::Vec(Arc::from(v)))
                }
                #[cfg(not(target_endian = "little"))]
                {
                    SPILL_SCRATCH.with(|cell| {
                        let mut scratch = cell.borrow_mut();
                        scratch.resize(byte_len, 0);
                        reader.read_exact(&mut scratch[..byte_len]).map_err(io_err)?;
                        let v: Vec<f32> = scratch[..byte_len]
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();
                        Ok(Cell::Vec(Arc::from(v)))
                    })
                }
            }
            _ => Err(Error::runtime("invalid spill cell tag")),
        }
    }
}

impl<T: Spillable> Spillable for Option<T> {
    #[inline]
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        match self {
            Some(v) => {
                writer.write_all(&[1u8]).map_err(io_err)?;
                v.serialize(writer)
            }
            None => writer.write_all(&[0u8]).map_err(io_err),
        }
    }

    #[inline]
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut tag = [0u8; 1];
        reader.read_exact(&mut tag).map_err(io_err)?;
        match tag[0] {
            1 => Ok(Some(T::deserialize(reader)?)),
            0 => Ok(None),
            _ => Err(Error::runtime("invalid spill option tag")),
        }
    }
}

impl<A: Spillable, B: Spillable> Spillable for (A, B) {
    #[inline]
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        self.0.serialize(writer)?;
        self.1.serialize(writer)
    }

    #[inline]
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        Ok((A::deserialize(reader)?, B::deserialize(reader)?))
    }
}

impl Spillable for f64 {
    #[inline]
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        writer.write_all(&self.to_le_bytes()).map_err(io_err)
    }

    #[inline]
    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut buf = [0u8; 8];
        reader.read_exact(&mut buf).map_err(io_err)?;
        Ok(f64::from_le_bytes(buf))
    }
}

impl Spillable for crate::store::Row {
    fn serialize(&self, writer: &mut impl Write) -> Result<(), Error> {
        let len = u32::try_from(self.len())
            .map_err(|_| Error::runtime("spill row field count exceeds u32"))?;
        writer.write_all(&len.to_le_bytes()).map_err(io_err)?;
        for (name, cell) in self.iter() {
            let nb = name.as_bytes();
            let nlen = u32::try_from(nb.len())
                .map_err(|_| Error::runtime("spill row field name exceeds u32"))?;
            writer.write_all(&nlen.to_le_bytes()).map_err(io_err)?;
            writer.write_all(nb).map_err(io_err)?;
            cell.serialize(writer)?;
        }
        Ok(())
    }

    fn deserialize(reader: &mut impl Read) -> Result<Self, Error> {
        let mut len_buf = [0u8; 4];
        reader.read_exact(&mut len_buf).map_err(io_err)?;
        let count = u32::from_le_bytes(len_buf) as usize;
        if count == 0 {
            return Ok(crate::store::Row::new());
        }
        if count > 65_536 {
            return Err(Error::corrupt("corrupt spill row field count"));
        }
        let mut names: Vec<String> = Vec::with_capacity(count);
        let mut cells: Vec<Cell> = Vec::with_capacity(count);
        for _ in 0..count {
            let mut nlen_buf = [0u8; 4];
            reader.read_exact(&mut nlen_buf).map_err(io_err)?;
            let nlen = u32::from_le_bytes(nlen_buf) as usize;
            if nlen > MAX_SPILL_STR_BYTES {
                return Err(Error::corrupt("corrupt spill field name length"));
            }
            let mut nbytes = vec![0u8; nlen];
            reader.read_exact(&mut nbytes).map_err(io_err)?;
            let name = std::str::from_utf8(&nbytes)
                .map_err(|e| Error::corrupt(format!("invalid spill field name utf8: {e}")))?
                .to_owned();
            let cell = Cell::deserialize(reader)?;
            names.push(name);
            cells.push(cell);
        }
        if names.windows(2).all(|w| w[0] < w[1]) {
            Ok(crate::store::Row::from_sorted(
                names.iter().map(String::as_str),
                cells,
            ))
        } else {
            let mut row = crate::store::Row::new();
            for (k, v) in names.into_iter().zip(cells) {
                row.insert(k, v);
            }
            Ok(row)
        }
    }
}

/// String collation policy for in-memory and external sorts (`D20`: `LIN-0781`..`LIN-0784`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CollationPolicy {
    #[default]
    Binary,
    AsciiCaseInsensitive,
    LocaleFolded,
    NaturalSort,
}

/// Compare two UTF-8 strings under `policy` (`NaturalSort` orders embedded digit runs
/// numerically so `"item2" < "item10"` without integer overflow).
pub fn compare_str_collated(a: &str, b: &str, policy: CollationPolicy) -> std::cmp::Ordering {
    match policy {
        CollationPolicy::Binary => a.cmp(b),
        CollationPolicy::AsciiCaseInsensitive => a
            .bytes()
            .map(|c| c.to_ascii_lowercase())
            .cmp(b.bytes().map(|c| c.to_ascii_lowercase())),
        CollationPolicy::LocaleFolded => a
            .chars()
            .flat_map(char::to_lowercase)
            .cmp(b.chars().flat_map(char::to_lowercase)),
        CollationPolicy::NaturalSort => compare_str_natural(a, b),
    }
}

fn compare_str_natural(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let mut ia = 0usize;
    let mut ib = 0usize;
    let mut leading_zero_tie = Ordering::Equal;

    while ia < ab.len() && ib < bb.len() {
        let ca = ab[ia];
        let cb = bb[ib];
        if ca.is_ascii_digit() && cb.is_ascii_digit() {
            let sa = ia;
            while ia < ab.len() && ab[ia].is_ascii_digit() {
                ia += 1;
            }
            let sb = ib;
            while ib < bb.len() && bb[ib].is_ascii_digit() {
                ib += 1;
            }
            let run_a = &ab[sa..ia];
            let run_b = &bb[sb..ib];
            let trim_a_pos = run_a.iter().position(|&d| d != b'0').unwrap_or(run_a.len());
            let trim_b_pos = run_b.iter().position(|&d| d != b'0').unwrap_or(run_b.len());
            let sig_a = &run_a[trim_a_pos..];
            let sig_b = &run_b[trim_b_pos..];

            match sig_a.len().cmp(&sig_b.len()) {
                Ordering::Equal => match sig_a.cmp(sig_b) {
                    Ordering::Equal => {
                        if leading_zero_tie == Ordering::Equal {
                            leading_zero_tie = run_a.len().cmp(&run_b.len());
                        }
                    }
                    non_eq => return non_eq,
                },
                non_eq => return non_eq,
            }
        } else {
            if ca != cb {
                return ca.cmp(&cb);
            }
            ia += 1;
            ib += 1;
        }
    }

    match (ia < ab.len(), ib < bb.len()) {
        (false, false) => leading_zero_tie,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (true, true) => unreachable!(),
    }
}

/// Compare two [`Cell`] values using `policy` for text cells and [`crate::eval::cmp_sort`] otherwise.
#[inline]
pub fn compare_cell_collated(a: &Cell, b: &Cell, policy: CollationPolicy) -> std::cmp::Ordering {
    match (a.text(), b.text()) {
        (Some(sa), Some(sb)) => compare_str_collated(sa, sb, policy),
        _ => crate::eval::cmp_sort(a, b),
    }
}

/// Telemetry emitted by [`ExternalRowSorter`] and bounded top-k sort operators
/// (`D20`: `LIN-0777`..`LIN-0780`, `LIN-0793`..`LIN-0796`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SortDiagnostics {
    pub input_rows: usize,
    pub estimated_bytes: usize,
    pub comparisons: usize,
    pub topk_rejected: usize,
    pub spilled_runs: usize,
    pub spilled_bytes: usize,
}

static EXTSORT_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

struct TempRunGuard {
    paths: Vec<std::path::PathBuf>,
}

impl TempRunGuard {
    fn new() -> Self {
        Self { paths: Vec::new() }
    }

    fn push(&mut self, path: std::path::PathBuf) {
        self.paths.push(path);
    }
}

impl Drop for TempRunGuard {
    fn drop(&mut self) {
        for p in &self.paths {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Bounded-memory external merge sorter with RAII spill-file cleanup (`D20`: `LIN-0777`..`LIN-0796`).
#[derive(Debug, Clone)]
pub struct ExternalRowSorter {
    pub memory_budget_bytes: usize,
    pub collation: CollationPolicy,
    pub temp_dir: std::path::PathBuf,
}

impl ExternalRowSorter {
    pub fn new(memory_budget_bytes: usize) -> Self {
        Self {
            memory_budget_bytes,
            collation: CollationPolicy::Binary,
            temp_dir: std::env::temp_dir(),
        }
    }

    pub fn with_collation(mut self, policy: CollationPolicy) -> Self {
        self.collation = policy;
        self
    }

    pub fn with_temp_dir(mut self, dir: impl Into<std::path::PathBuf>) -> Self {
        self.temp_dir = dir.into();
        self
    }

    /// Sort `rows` by `sort_field`, spilling sorted runs to `self.temp_dir` whenever
    /// `self.memory_budget_bytes` is exceeded, k-way merging runs via a binary heap,
    /// and cleaning up all temporary run files on completion, error, or cancellation.
    pub fn sort_rows_external(
        &self,
        rows: Vec<crate::store::Row>,
        sort_field: &str,
        desc: bool,
        mut is_cancelled: impl FnMut() -> bool,
    ) -> Result<(Vec<crate::store::Row>, SortDiagnostics), Error> {
        use std::collections::BinaryHeap;
        use std::io::{BufReader, BufWriter};
        use std::sync::atomic::Ordering;

        let input_rows = rows.len();
        if input_rows == 0 {
            return Ok((Vec::new(), SortDiagnostics::default()));
        }

        let budget = self.memory_budget_bytes.max(1);
        let policy = self.collation;
        let run_batch_id = EXTSORT_SEQ.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();

        let mut guard = TempRunGuard::new();
        let mut diag = SortDiagnostics {
            input_rows,
            ..SortDiagnostics::default()
        };

        let mut chunk: Vec<(usize, crate::store::Row)> = Vec::new();
        let mut chunk_bytes = 0usize;

        let flush_chunk = |chunk: &mut Vec<(usize, crate::store::Row)>,
                           run_idx: usize,
                           guard: &mut TempRunGuard,
                           diag: &mut SortDiagnostics|
         -> Result<(), Error> {
            let mut slot = crate::store::row::FieldSlot::new(sort_field);
            static NULL_CELL: Cell = Cell::Null;
            let decorated: Vec<(Cell, usize, crate::store::Row)> = chunk
                .drain(..)
                .map(|(seq, r)| {
                    let key = slot
                        .get(&r)
                        .map(Cell::clone_for_borrow)
                        .unwrap_or(NULL_CELL.clone());
                    (key, seq, r)
                })
                .collect();
            let mut order: Vec<usize> = (0..decorated.len()).collect();
            order.sort_by(|&ia, &ib| {
                diag.comparisons += 1;
                let ord = compare_cell_collated(&decorated[ia].0, &decorated[ib].0, policy);
                let ord = if desc { ord.reverse() } else { ord };
                ord.then_with(|| decorated[ia].1.cmp(&decorated[ib].1))
            });

            let path = self
                .temp_dir
                .join(format!("lin_extsort_{pid}_{run_batch_id}_{run_idx}.bin"));
            let file = std::fs::File::create(&path).map_err(io_err)?;
            guard.push(path.clone());
            let mut writer = BufWriter::with_capacity(64 * 1024, file);
            let count_u32 = u32::try_from(order.len())
                .map_err(|_| Error::runtime("extsort run length overflow"))?;
            writer.write_all(&count_u32.to_le_bytes()).map_err(io_err)?;
            for &pos in &order {
                let (_, seq, ref row) = decorated[pos];
                writer
                    .write_all(&(seq as u64).to_le_bytes())
                    .map_err(io_err)?;
                row.serialize(&mut writer)?;
            }
            writer.flush().map_err(io_err)?;
            drop(writer);
            let bytes = std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
            diag.spilled_runs += 1;
            diag.spilled_bytes = diag.spilled_bytes.saturating_add(bytes);
            Ok(())
        };

        for (seq, row) in rows.into_iter().enumerate() {
            if (seq & 63) == 0 && is_cancelled() {
                return Err(Error::canceled("external sort cancelled during partitioning"));
            }
            let rb = crate::store::row_byte_size(&row);
            diag.estimated_bytes = diag.estimated_bytes.saturating_add(rb);
            if !chunk.is_empty() && chunk_bytes.saturating_add(rb) > budget {
                let run_idx = guard.paths.len();
                flush_chunk(&mut chunk, run_idx, &mut guard, &mut diag)?;
                chunk_bytes = 0;
            }
            chunk_bytes = chunk_bytes.saturating_add(rb);
            chunk.push((seq, row));
        }

        if diag.spilled_runs == 0 {
            let mut slot = crate::store::row::FieldSlot::new(sort_field);
            static NULL_CELL: Cell = Cell::Null;
            let mut decorated: Vec<(Cell, usize, crate::store::Row)> = chunk
                .into_iter()
                .map(|(seq, r)| {
                    let key = slot
                        .get(&r)
                        .map(Cell::clone_for_borrow)
                        .unwrap_or(NULL_CELL.clone());
                    (key, seq, r)
                })
                .collect();
            decorated.sort_by(|a, b| {
                diag.comparisons += 1;
                let ord = compare_cell_collated(&a.0, &b.0, policy);
                let ord = if desc { ord.reverse() } else { ord };
                ord.then_with(|| a.1.cmp(&b.1))
            });
            let sorted = decorated.into_iter().map(|(_, _, r)| r).collect();
            return Ok((sorted, diag));
        }

        if !chunk.is_empty() {
            let run_idx = guard.paths.len();
            flush_chunk(&mut chunk, run_idx, &mut guard, &mut diag)?;
        }

        struct RunStream {
            reader: BufReader<std::fs::File>,
            remaining: usize,
        }

        impl RunStream {
            fn open(path: &std::path::Path) -> Result<Self, Error> {
                let file = std::fs::File::open(path).map_err(io_err)?;
                let mut reader = BufReader::with_capacity(32 * 1024, file);
                let mut len_buf = [0u8; 4];
                reader.read_exact(&mut len_buf).map_err(io_err)?;
                let remaining = u32::from_le_bytes(len_buf) as usize;
                Ok(Self { reader, remaining })
            }

            fn next_item(&mut self) -> Result<Option<(usize, crate::store::Row)>, Error> {
                if self.remaining == 0 {
                    return Ok(None);
                }
                self.remaining -= 1;
                let mut seq_buf = [0u8; 8];
                self.reader.read_exact(&mut seq_buf).map_err(io_err)?;
                let seq = u64::from_le_bytes(seq_buf) as usize;
                let row = crate::store::Row::deserialize(&mut self.reader)?;
                Ok(Some((seq, row)))
            }
        }

        struct HeapNode {
            key: Cell,
            seq: usize,
            run_idx: usize,
            row: crate::store::Row,
            policy: CollationPolicy,
            desc: bool,
        }

        impl PartialEq for HeapNode {
            fn eq(&self, other: &Self) -> bool {
                self.seq == other.seq && self.run_idx == other.run_idx
            }
        }
        impl Eq for HeapNode {}
        impl PartialOrd for HeapNode {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl Ord for HeapNode {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                // Reversed so BinaryHeap (max-heap) pops the smallest logical item first.
                let ord = compare_cell_collated(&self.key, &other.key, self.policy);
                let ord = if self.desc { ord.reverse() } else { ord };
                ord.then_with(|| self.seq.cmp(&other.seq)).reverse()
            }
        }

        let mut streams: Vec<RunStream> = Vec::with_capacity(guard.paths.len());
        for p in &guard.paths {
            streams.push(RunStream::open(p)?);
        }

        let mut key_slot = crate::store::row::FieldSlot::new(sort_field);
        let mut heap: BinaryHeap<HeapNode> = BinaryHeap::with_capacity(streams.len());
        for (run_idx, s) in streams.iter_mut().enumerate() {
            if let Some((seq, row)) = s.next_item()? {
                let key = key_slot
                    .get(&row)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                heap.push(HeapNode {
                    key,
                    seq,
                    run_idx,
                    row,
                    policy,
                    desc,
                });
            }
        }

        let mut out = Vec::with_capacity(input_rows);
        let fan_in_log = (streams.len().max(2) as usize).ilog2().max(1) as usize;
        while let Some(node) = heap.pop() {
            if (out.len() & 63) == 0 && is_cancelled() {
                return Err(Error::canceled("external sort cancelled during k-way merge"));
            }
            diag.comparisons = diag.comparisons.saturating_add(fan_in_log);
            out.push(node.row);
            if let Some((next_seq, next_row)) = streams[node.run_idx].next_item()? {
                let key = key_slot
                    .get(&next_row)
                    .map(Cell::clone_for_borrow)
                    .unwrap_or(Cell::Null);
                heap.push(HeapNode {
                    key,
                    seq: next_seq,
                    run_idx: node.run_idx,
                    row: next_row,
                    policy,
                    desc,
                });
            }
        }
        drop(streams);
        // `guard` drops here and removes every temp spill file.
        Ok((out, diag))
    }

    /// K-way merge pre-sorted in-memory row runs via a binary heap (`LIN-0779`).
    pub fn merge_sorted_row_runs(
        &self,
        runs: Vec<Vec<crate::store::Row>>,
        sort_field: &str,
        desc: bool,
    ) -> Result<(Vec<crate::store::Row>, SortDiagnostics), Error> {
        merge_sorted_row_runs(runs, sort_field, desc, self.collation)
    }

    /// Bounded top-k selection with comparison and rejection telemetry (`LIN-0795`).
    pub fn top_k_rows_with_stats(
        &self,
        rows: &[crate::store::Row],
        sort_field: &str,
        desc: bool,
        k: usize,
    ) -> (Vec<crate::store::Row>, SortDiagnostics) {
        top_k_rows_with_stats(rows, sort_field, desc, k, self.collation)
    }
}

/// K-way merge pre-sorted row runs using `policy` (`D20`: `LIN-0779`).
pub fn merge_sorted_row_runs(
    runs: Vec<Vec<crate::store::Row>>,
    sort_field: &str,
    desc: bool,
    policy: CollationPolicy,
) -> Result<(Vec<crate::store::Row>, SortDiagnostics), Error> {
    use std::collections::BinaryHeap;

    struct MergeNode {
        key: Cell,
        run_idx: usize,
        pos_in_run: usize,
        row: crate::store::Row,
        policy: CollationPolicy,
        desc: bool,
    }
    impl PartialEq for MergeNode {
        fn eq(&self, other: &Self) -> bool {
            self.run_idx == other.run_idx && self.pos_in_run == other.pos_in_run
        }
    }
    impl Eq for MergeNode {}
    impl PartialOrd for MergeNode {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for MergeNode {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            let ord = compare_cell_collated(&self.key, &other.key, self.policy);
            let ord = if self.desc { ord.reverse() } else { ord };
            ord.then_with(|| self.run_idx.cmp(&other.run_idx))
                .then_with(|| self.pos_in_run.cmp(&other.pos_in_run))
                .reverse()
        }
    }

    let total_rows: usize = runs.iter().map(Vec::len).sum();
    let estimated_bytes: usize = runs
        .iter()
        .flat_map(|r| r.iter())
        .map(crate::store::row_byte_size)
        .sum();
    if total_rows == 0 {
        return Ok((Vec::new(), SortDiagnostics::default()));
    }

    let mut iters: Vec<std::iter::Enumerate<std::vec::IntoIter<crate::store::Row>>> = runs
        .into_iter()
        .map(|v| v.into_iter().enumerate())
        .collect();
    let mut slot = crate::store::row::FieldSlot::new(sort_field);
    let mut heap: BinaryHeap<MergeNode> = BinaryHeap::with_capacity(iters.len());

    for (run_idx, it) in iters.iter_mut().enumerate() {
        if let Some((pos, row)) = it.next() {
            let key = slot
                .get(&row)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null);
            heap.push(MergeNode {
                key,
                run_idx,
                pos_in_run: pos,
                row,
                policy,
                desc,
            });
        }
    }

    let mut out = Vec::with_capacity(total_rows);
    let mut comparisons = 0usize;
    let fan_in = (iters.len().max(2)).ilog2().max(1) as usize;
    while let Some(node) = heap.pop() {
        comparisons = comparisons.saturating_add(fan_in);
        out.push(node.row);
        if let Some((pos, next_row)) = iters[node.run_idx].next() {
            let key = slot
                .get(&next_row)
                .map(Cell::clone_for_borrow)
                .unwrap_or(Cell::Null);
            heap.push(MergeNode {
                key,
                run_idx: node.run_idx,
                pos_in_run: pos,
                row: next_row,
                policy,
                desc,
            });
        }
    }

    Ok((
        out,
        SortDiagnostics {
            input_rows: total_rows,
            estimated_bytes,
            comparisons,
            topk_rejected: 0,
            spilled_runs: 0,
            spilled_bytes: 0,
        },
    ))
}

/// Select the top `k` rows ordered by `sort_field` using a bounded `O(k)` heap, reporting
/// `comparisons` and `topk_rejected` (`D20`: `LIN-0795`).
pub fn top_k_rows_with_stats(
    rows: &[crate::store::Row],
    sort_field: &str,
    desc: bool,
    k: usize,
    policy: CollationPolicy,
) -> (Vec<crate::store::Row>, SortDiagnostics) {
    use std::collections::BinaryHeap;

    let input_rows = rows.len();
    let estimated_bytes: usize = rows.iter().map(crate::store::row_byte_size).sum();
    if input_rows == 0 || k == 0 {
        return (
            Vec::new(),
            SortDiagnostics {
                input_rows,
                estimated_bytes,
                comparisons: 0,
                topk_rejected: input_rows,
                spilled_runs: 0,
                spilled_bytes: 0,
            },
        );
    }

    struct TopKEntry {
        key: Cell,
        idx: usize,
        policy: CollationPolicy,
        desc: bool,
    }
    impl PartialEq for TopKEntry {
        fn eq(&self, other: &Self) -> bool {
            self.idx == other.idx
        }
    }
    impl Eq for TopKEntry {}
    impl PartialOrd for TopKEntry {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for TopKEntry {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // Max-heap keeps the worst of the current top-k at the root.
            let ord = compare_cell_collated(&self.key, &other.key, self.policy);
            let ord = if self.desc { ord.reverse() } else { ord };
            ord.then_with(|| self.idx.cmp(&other.idx))
        }
    }

    static NULL_CELL: Cell = Cell::Null;
    let cap = k.min(input_rows);
    let mut slot = crate::store::row::FieldSlot::new(sort_field);
    let mut heap: BinaryHeap<TopKEntry> = BinaryHeap::with_capacity(cap);
    let mut comparisons = 0usize;
    let mut rejected = 0usize;

    for (idx, row) in rows.iter().enumerate() {
        let cand_ref = slot.get(row).unwrap_or(&NULL_CELL);
        if heap.len() < cap {
            comparisons += 1;
            heap.push(TopKEntry {
                key: cand_ref.clone_for_borrow(),
                idx,
                policy,
                desc,
            });
        } else if let Some(mut worst) = heap.peek_mut() {
            comparisons += 1;
            let ord = compare_cell_collated(cand_ref, &worst.key, policy);
            let ord = if desc { ord.reverse() } else { ord };
            if ord == std::cmp::Ordering::Less {
                *worst = TopKEntry {
                    key: cand_ref.clone_for_borrow(),
                    idx,
                    policy,
                    desc,
                };
            }
            rejected += 1;
        }
    }

    let winners = heap.into_sorted_vec();
    let out: Vec<crate::store::Row> = winners.into_iter().map(|e| rows[e.idx].clone()).collect();
    (
        out,
        SortDiagnostics {
            input_rows,
            estimated_bytes,
            comparisons,
            topk_rejected: rejected,
            spilled_runs: 0,
            spilled_bytes: 0,
        },
    )
}
