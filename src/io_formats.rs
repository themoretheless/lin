//! Bounded streaming JSON Lines (`JSONL`), RFC-4180 CSV, and columnar wire IPC import/export
//! with schema inference, field/type/default mapping, row-level reject routing, and cancellation (`D47`: `LIN-1841`..`LIN-1880`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, Write};
use std::sync::Arc;

use rustc_hash::FxHashSet;

use crate::ast::{Query, Source, Step, Stmt};
use crate::batch::RecordBatch;
use crate::catalog::Type;
use crate::error::Error;
use crate::exec::Db;
use crate::store::{Cell, Row, row_text};

const IPC_STREAM_MAGIC: &[u8; 8] = b"LINIPC01";

/// Hard resource ceilings for untrusted streaming imports (`LIN-1864`, `LIN-1869`..`LIN-1872`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportLimits {
    pub max_row_bytes: usize,
    pub max_total_bytes: usize,
    pub max_nesting_depth: usize,
    pub batch_size: usize,
    pub expected_vector_dim: Option<usize>,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_row_bytes: 4 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
            max_nesting_depth: 32,
            batch_size: 1024,
            expected_vector_dim: None,
        }
    }
}

/// Declarative field rename, default value, null-sentinel, type coercion, and deduplication mapping
/// (`LIN-1850`, `LIN-1854`..`LIN-1856`, `LIN-1865`, `LIN-1867`).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportSchemaMapping {
    pub field_renames: BTreeMap<String, String>,
    pub defaults: BTreeMap<String, Cell>,
    pub explicit_types: BTreeMap<String, Type>,
    pub empty_csv_as_null: bool,
    pub null_sentinel: String,
    pub id_field: Option<String>,
    pub deduplicate_ids: bool,
    pub start_row_offset: usize,
}

impl Default for ImportSchemaMapping {
    fn default() -> Self {
        Self {
            field_renames: BTreeMap::new(),
            defaults: BTreeMap::new(),
            explicit_types: BTreeMap::new(),
            empty_csv_as_null: true,
            null_sentinel: String::new(),
            id_field: None,
            deduplicate_ids: false,
            start_row_offset: 0,
        }
    }
}

/// Sampled schema inference summary (`LIN-1853`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredSchemaReport {
    pub fields: BTreeMap<String, Type>,
    pub nullable_fields: BTreeSet<String>,
    pub sampled_rows: usize,
}

/// Per-row diagnostic captured during import (`LIN-1857`, `LIN-1858`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRowError {
    pub row_number: usize,
    pub byte_offset: u64,
    pub message: String,
}

/// Deterministic summary receipt emitted by streaming importers (`LIN-1859`, `LIN-1860`, `LIN-1866`, `LIN-1880`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReceipt {
    pub imported_rows: usize,
    pub skipped_rows: usize,
    pub rejected_rows: usize,
    pub bytes_read: u64,
    pub batches_committed: usize,
    pub cancelled: bool,
    pub source_sha256_fnv: u64,
    pub errors: Vec<ImportRowError>,
}

#[inline]
fn fnv1a_init() -> u64 {
    0xcbf2_9ce4_8422_2325
}

#[inline]
fn fnv1a_update(mut h: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------
// Zero-external-dep bounded JSON value/object parser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum JsonVal {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<JsonVal>),
    Object(Vec<(String, JsonVal)>),
}

struct JsonParser<'a> {
    src: &'a [u8],
    pos: usize,
    max_depth: usize,
}

impl<'a> JsonParser<'a> {
    fn new(src: &'a [u8], max_depth: usize) -> Self {
        Self {
            src,
            pos: 0,
            max_depth: max_depth.max(1),
        }
    }

    fn skip_ws(&mut self) {
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn parse_top_object(&mut self) -> Result<Vec<(String, JsonVal)>, String> {
        self.skip_ws();
        if self.pos >= self.src.len() || self.src[self.pos] != b'{' {
            return Err("expected JSON object `{...}` on JSONL line".to_string());
        }
        let val = self.parse_value(1)?;
        self.skip_ws();
        if self.pos != self.src.len() {
            return Err(format!("trailing characters at byte {}", self.pos));
        }
        match val {
            JsonVal::Object(entries) => Ok(entries),
            _ => Err("expected JSON object".to_string()),
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<JsonVal, String> {
        self.skip_ws();
        let Some(&b) = self.src.get(self.pos) else {
            return Err("unexpected end of JSON input".to_string());
        };
        match b {
            b'{' => {
                if depth > self.max_depth {
                    return Err(format!(
                        "JSON nesting depth {depth} exceeds max_nesting_depth {}",
                        self.max_depth
                    ));
                }
                self.parse_object(depth)
            }
            b'[' => {
                if depth > self.max_depth {
                    return Err(format!(
                        "JSON nesting depth {depth} exceeds max_nesting_depth {}",
                        self.max_depth
                    ));
                }
                self.parse_array(depth)
            }
            b'"' => self.parse_string().map(JsonVal::Str),
            b't' => {
                if self.src[self.pos..].starts_with(b"true") {
                    self.pos += 4;
                    Ok(JsonVal::Bool(true))
                } else {
                    Err(format!("invalid literal at byte {}", self.pos))
                }
            }
            b'f' => {
                if self.src[self.pos..].starts_with(b"false") {
                    self.pos += 5;
                    Ok(JsonVal::Bool(false))
                } else {
                    Err(format!("invalid literal at byte {}", self.pos))
                }
            }
            b'n' => {
                if self.src[self.pos..].starts_with(b"null") {
                    self.pos += 4;
                    Ok(JsonVal::Null)
                } else {
                    Err(format!("invalid literal at byte {}", self.pos))
                }
            }
            b'-' | b'0'..=b'9' => self.parse_number(),
            other => Err(format!(
                "unexpected character {:?} at byte {}",
                other as char, self.pos
            )),
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<JsonVal, String> {
        self.pos += 1; // '{'
        self.skip_ws();
        let mut entries = Vec::new();
        if self.src.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(JsonVal::Object(entries));
        }
        loop {
            self.skip_ws();
            if self.src.get(self.pos) != Some(&b'"') {
                return Err(format!("expected object key string at byte {}", self.pos));
            }
            let key = self.parse_string()?;
            self.skip_ws();
            if self.src.get(self.pos) != Some(&b':') {
                return Err(format!("expected `:` after key `{key}` at byte {}", self.pos));
            }
            self.pos += 1;
            let val = self.parse_value(depth + 1)?;
            entries.push((key, val));
            self.skip_ws();
            match self.src.get(self.pos) {
                Some(&b',') => {
                    self.pos += 1;
                }
                Some(&b'}') => {
                    self.pos += 1;
                    return Ok(JsonVal::Object(entries));
                }
                _ => {
                    return Err(format!(
                        "expected `,` or `}}` in object at byte {}",
                        self.pos
                    ));
                }
            }
        }
    }

    fn parse_array(&mut self, depth: usize) -> Result<JsonVal, String> {
        self.pos += 1; // '['
        self.skip_ws();
        let mut items = Vec::new();
        if self.src.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(JsonVal::Array(items));
        }
        loop {
            let v = self.parse_value(depth + 1)?;
            items.push(v);
            self.skip_ws();
            match self.src.get(self.pos) {
                Some(&b',') => {
                    self.pos += 1;
                }
                Some(&b']') => {
                    self.pos += 1;
                    return Ok(JsonVal::Array(items));
                }
                _ => {
                    return Err(format!(
                        "expected `,` or `]` in array at byte {}",
                        self.pos
                    ));
                }
            }
        }
    }

    fn parse_hex4(&mut self) -> Result<u16, String> {
        if self.pos + 4 > self.src.len() {
            return Err("truncated \\uXXXX escape".to_string());
        }
        let mut code: u16 = 0;
        for _ in 0..4 {
            let b = self.src[self.pos];
            self.pos += 1;
            let digit = match b {
                b'0'..=b'9' => u16::from(b - b'0'),
                b'a'..=b'f' => u16::from(b - b'a' + 10),
                b'A'..=b'F' => u16::from(b - b'A' + 10),
                _ => return Err("invalid hex digit in \\uXXXX escape".to_string()),
            };
            code = (code << 4) | digit;
        }
        Ok(code)
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.pos += 1; // '"'
        let mut out = String::new();
        while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if b == b'"' {
                self.pos += 1;
                return Ok(out);
            }
            if b == b'\\' {
                self.pos += 1;
                let Some(&esc) = self.src.get(self.pos) else {
                    return Err("unterminated escape sequence".to_string());
                };
                self.pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    b'u' => {
                        let hi = self.parse_hex4()?;
                        if (0xD800..=0xDBFF).contains(&hi) {
                            if self.pos + 2 <= self.src.len()
                                && self.src[self.pos] == b'\\'
                                && self.src[self.pos + 1] == b'u'
                            {
                                self.pos += 2;
                                let lo = self.parse_hex4()?;
                                if !(0xDC00..=0xDFFF).contains(&lo) {
                                    return Err("invalid UTF-16 low surrogate".to_string());
                                }
                                let cp = 0x10000
                                    + ((u32::from(hi - 0xD800) << 10) | u32::from(lo - 0xDC00));
                                let ch = char::from_u32(cp)
                                    .ok_or_else(|| "invalid Unicode scalar value".to_string())?;
                                out.push(ch);
                            } else {
                                return Err("unpaired UTF-16 high surrogate".to_string());
                            }
                        } else {
                            let ch = char::from_u32(u32::from(hi))
                                .ok_or_else(|| "invalid Unicode scalar in \\u escape".to_string())?;
                            out.push(ch);
                        }
                    }
                    other => {
                        return Err(format!("invalid JSON escape `\\{}`", other as char));
                    }
                }
                continue;
            }
            if b < 0x20 {
                return Err("unescaped control character in JSON string".to_string());
            }
            let rest = std::str::from_utf8(&self.src[self.pos..])
                .map_err(|_| "invalid UTF-8 in JSON string".to_string())?;
            let ch = rest.chars().next().unwrap();
            out.push(ch);
            self.pos += ch.len_utf8();
        }
        Err("unterminated JSON string".to_string())
    }

    fn parse_number(&mut self) -> Result<JsonVal, String> {
        let start = self.pos;
        if self.src.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        let int_start = self.pos;
        while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.pos == int_start {
            return Err(format!("invalid number at byte {start}"));
        }
        let mut is_float = false;
        if self.src.get(self.pos) == Some(&b'.') {
            is_float = true;
            self.pos += 1;
            let frac_start = self.pos;
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if self.pos == frac_start {
                return Err(format!("invalid decimal fraction at byte {start}"));
            }
        }
        if matches!(self.src.get(self.pos), Some(b'e' | b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.src.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while self.pos < self.src.len() && self.src[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if self.pos == exp_start {
                return Err(format!("invalid exponent at byte {start}"));
            }
        }
        let s = std::str::from_utf8(&self.src[start..self.pos])
            .map_err(|_| "invalid UTF-8 number".to_string())?;
        if !is_float && let Ok(i) = s.parse::<i64>() {
            return Ok(JsonVal::Int(i));
        }
        let f = s
            .parse::<f64>()
            .map_err(|e| format!("invalid float `{s}`: {e}"))?;
        if !f.is_finite() {
            return Err(format!("non-finite JSON number `{s}`"));
        }
        Ok(JsonVal::Float(f))
    }
}

fn json_val_to_compact_string(val: &JsonVal, out: &mut String) {
    match val {
        JsonVal::Null => out.push_str("null"),
        JsonVal::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonVal::Int(n) => {
            use std::fmt::Write as _;
            let _ = write!(out, "{n}");
        }
        JsonVal::Float(f) => {
            use std::fmt::Write as _;
            let _ = write!(out, "{f}");
        }
        JsonVal::Str(s) => write_json_escaped_str(out, s),
        JsonVal::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                json_val_to_compact_string(item, out);
            }
            out.push(']');
        }
        JsonVal::Object(entries) => {
            out.push('{');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json_escaped_str(out, k);
                out.push(':');
                json_val_to_compact_string(v, out);
            }
            out.push('}');
        }
    }
}

fn write_json_escaped_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_val_to_cell(
    val: JsonVal,
    target_ty: Option<Type>,
    expected_vec_dim: Option<usize>,
) -> Result<Cell, String> {
    match val {
        JsonVal::Null => Ok(Cell::Null),
        JsonVal::Bool(b) => match target_ty {
            Some(Type::Text | Type::Var) => Ok(Cell::from_text(if b { "true" } else { "false" })),
            Some(other) if other != Type::Bool => {
                Err(format!("expected {}, got bool", other.name()))
            }
            _ => Ok(Cell::Bool(b)),
        },
        JsonVal::Int(n) => match target_ty {
            Some(Type::F64) => Ok(Cell::Float(n as f64)),
            Some(Type::Time | Type::Dur) => Ok(Cell::Time(n)),
            Some(Type::Text | Type::Var | Type::Id | Type::Uri | Type::Rel) => {
                Ok(Cell::from_text(&n.to_string()))
            }
            Some(other) if other != Type::I64 => {
                Err(format!("expected {}, got int", other.name()))
            }
            _ => Ok(Cell::Int(n)),
        },
        JsonVal::Float(f) => match target_ty {
            Some(Type::I64) if f.fract() == 0.0 && f >= i64::MIN as f64 && f < i64::MAX as f64 => {
                Ok(Cell::Int(f as i64))
            }
            Some(Type::Text | Type::Var) => Ok(Cell::from_text(&f.to_string())),
            Some(other) if other != Type::F64 => {
                Err(format!("expected {}, got float", other.name()))
            }
            _ => Ok(Cell::Float(f)),
        },
        JsonVal::Str(s) => match target_ty {
            Some(Type::I64) => s
                .trim()
                .parse::<i64>()
                .map(Cell::Int)
                .map_err(|_| format!("cannot parse `{s}` as i64")),
            Some(Type::F64) => s
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(Cell::Float)
                .ok_or_else(|| format!("cannot parse `{s}` as finite f64")),
            Some(Type::Bool) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Ok(Cell::Bool(true)),
                "false" | "0" => Ok(Cell::Bool(false)),
                _ => Err(format!("cannot parse `{s}` as bool")),
            },
            Some(Type::Time | Type::Dur) => s
                .trim()
                .parse::<i64>()
                .map(Cell::Time)
                .map_err(|_| format!("cannot parse `{s}` as timestamp i64")),
            Some(Type::Vec) => parse_vec_str(&s, expected_vec_dim),
            _ => Ok(Cell::from_text(&s)),
        },
        JsonVal::Array(items) => {
            if matches!(target_ty, Some(Type::Text | Type::Var)) {
                let mut s = String::new();
                json_val_to_compact_string(&JsonVal::Array(items), &mut s);
                return Ok(Cell::from_text(&s));
            }
            let mut vec = Vec::with_capacity(items.len());
            for item in &items {
                match item {
                    JsonVal::Int(i) => vec.push(*i as f32),
                    JsonVal::Float(f) => vec.push(*f as f32),
                    _ => {
                        if target_ty.is_none() {
                            let mut s = String::new();
                            json_val_to_compact_string(&JsonVal::Array(items), &mut s);
                            return Ok(Cell::from_text(&s));
                        }
                        return Err("expected numeric array for vector field".to_string());
                    }
                }
            }
            if let Some(exp) = expected_vec_dim
                && vec.len() != exp
            {
                return Err(format!(
                    "embedding dimension mismatch: expected {exp}, got {}",
                    vec.len()
                ));
            }
            Ok(Cell::Vec(Arc::from(vec)))
        }
        JsonVal::Object(entries) => {
            if matches!(target_ty, None | Some(Type::Text | Type::Var)) {
                let mut s = String::new();
                json_val_to_compact_string(&JsonVal::Object(entries), &mut s);
                Ok(Cell::from_text(&s))
            } else {
                Err(format!(
                    "cannot convert nested JSON object to {}",
                    target_ty.unwrap().name()
                ))
            }
        }
    }
}

fn parse_vec_str(s: &str, expected_vec_dim: Option<usize>) -> Result<Cell, String> {
    let trimmed = s.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|t| t.strip_suffix(']'))
        .ok_or_else(|| format!("expected `[f32, ...]` vector literal, got `{s}`"))?;
    let mut vec = Vec::new();
    if !inner.trim().is_empty() {
        for part in inner.split(',') {
            let v = part
                .trim()
                .parse::<f32>()
                .map_err(|_| format!("invalid vector element `{}`", part.trim()))?;
            if !v.is_finite() {
                return Err("non-finite vector element".to_string());
            }
            vec.push(v);
        }
    }
    if let Some(exp) = expected_vec_dim
        && vec.len() != exp
    {
        return Err(format!(
            "embedding dimension mismatch: expected {exp}, got {}",
            vec.len()
        ));
    }
    Ok(Cell::Vec(Arc::from(vec)))
}

// ---------------------------------------------------------------------------
// Bounded line / RFC-4180 CSV streaming readers
// ---------------------------------------------------------------------------

enum BoundedReadOutcome {
    Eof,
    Record {
        raw: Vec<u8>,
        byte_offset: u64,
    },
    RowTooLong {
        raw: Vec<u8>,
        byte_offset: u64,
        len: usize,
    },
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    max_row_bytes: usize,
    max_total_bytes: usize,
    bytes_read: &mut u64,
    fnv: &mut u64,
) -> Result<BoundedReadOutcome, Error> {
    let start_offset = *bytes_read;
    let mut raw = Vec::new();
    let mut over_limit = false;

    loop {
        let buf = reader
            .fill_buf()
            .map_err(|e| Error::io(format!("import read error: {e}")))?;
        if buf.is_empty() {
            if raw.is_empty() {
                return Ok(BoundedReadOutcome::Eof);
            }
            break;
        }
        let nl_pos = memchr::memchr(b'\n', buf);
        let take_n = nl_pos.map_or(buf.len(), |p| p + 1);

        let next_total = bytes_read.saturating_add(take_n as u64);
        if next_total > max_total_bytes as u64 {
            return Err(Error::resource_limit(format!(
                "import stream exceeded max_total_bytes quota ({max_total_bytes} bytes)"
            )));
        }

        let chunk = &buf[..take_n];
        *fnv = fnv1a_update(*fnv, chunk);
        *bytes_read = next_total;

        if !over_limit {
            if raw.len() + take_n > max_row_bytes + 2 {
                let keep = (max_row_bytes + 2).saturating_sub(raw.len());
                raw.extend_from_slice(&chunk[..keep]);
                over_limit = true;
            } else {
                raw.extend_from_slice(chunk);
            }
        }
        let hit_nl = nl_pos.is_some();
        reader.consume(take_n);
        if hit_nl {
            break;
        }
    }

    // Strip trailing \r\n or \n for length check while keeping raw record clean
    while matches!(raw.last(), Some(b'\n' | b'\r')) {
        raw.pop();
    }
    let actual_len = (*bytes_read - start_offset) as usize;
    if over_limit || raw.len() > max_row_bytes {
        return Ok(BoundedReadOutcome::RowTooLong {
            raw,
            byte_offset: start_offset,
            len: actual_len.max(max_row_bytes + 1),
        });
    }
    Ok(BoundedReadOutcome::Record {
        raw,
        byte_offset: start_offset,
    })
}

#[derive(Debug, Clone)]
struct CsvField {
    value: String,
    was_quoted: bool,
}

enum CsvRecordOutcome {
    Eof,
    Record {
        fields: Vec<CsvField>,
        raw: Vec<u8>,
        byte_offset: u64,
    },
    Error {
        raw: Vec<u8>,
        byte_offset: u64,
        message: String,
    },
}

/// Streaming RFC-4180 compliant record parser over `BufRead`.
/// Handles quoted commas, embedded `\n` and `\r\n`, `""` escaped quotes,
/// and tracks `was_quoted` to distinguish `,,` from `,"",`.
fn read_csv_record<R: BufRead>(
    reader: &mut R,
    max_row_bytes: usize,
    max_total_bytes: usize,
    bytes_read: &mut u64,
    fnv: &mut u64,
) -> Result<CsvRecordOutcome, Error> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum State {
        FieldStart,
        Unquoted,
        Quoted,
        QuoteInQuoted,
        ExpectRecordEndLf,
    }

    let start_offset = *bytes_read;
    let mut raw: Vec<u8> = Vec::new();
    let mut fields: Vec<CsvField> = Vec::new();
    let mut cur_bytes: Vec<u8> = Vec::new();
    let mut cur_quoted = false;
    let mut state = State::FieldStart;
    let mut parse_err: Option<String> = None;

    loop {
        let buf = reader
            .fill_buf()
            .map_err(|e| Error::io(format!("csv read error: {e}")))?;
        if buf.is_empty() {
            if raw.is_empty() && fields.is_empty() && cur_bytes.is_empty() && state == State::FieldStart {
                return Ok(CsvRecordOutcome::Eof);
            }
            if state == State::Quoted {
                parse_err = Some("unterminated quoted CSV field at EOF".to_string());
            }
            break;
        }

        let mut consumed = 0usize;
        let mut finished_record = false;

        while consumed < buf.len() {
            let b = buf[consumed];
            consumed += 1;

            if *bytes_read + (consumed as u64) > max_total_bytes as u64 {
                return Err(Error::resource_limit(format!(
                    "CSV import exceeded max_total_bytes quota ({max_total_bytes} bytes)"
                )));
            }

            if raw.len() <= max_row_bytes + 2 {
                raw.push(b);
            }

            if raw.len() > max_row_bytes + 2 && parse_err.is_none() {
                parse_err = Some(format!(
                    "CSV record exceeds max_row_bytes ({max_row_bytes} bytes)"
                ));
            }

            match state {
                State::ExpectRecordEndLf => {
                    if b == b'\n' {
                        finished_record = true;
                        break;
                    }
                    // Bare `\r` line ending: push back `b` for the next record
                    consumed -= 1;
                    raw.pop();
                    finished_record = true;
                    break;
                }
                State::FieldStart => match b {
                    b'"' => {
                        cur_quoted = true;
                        state = State::Quoted;
                    }
                    b',' => {
                        fields.push(CsvField {
                            value: String::new(),
                            was_quoted: false,
                        });
                        cur_bytes.clear();
                        cur_quoted = false;
                    }
                    b'\r' => {
                        state = State::ExpectRecordEndLf;
                    }
                    b'\n' => {
                        finished_record = true;
                        break;
                    }
                    other => {
                        if parse_err.is_none() {
                            cur_bytes.push(other);
                        }
                        state = State::Unquoted;
                    }
                },
                State::Unquoted => match b {
                    b',' => {
                        let s = String::from_utf8(std::mem::take(&mut cur_bytes)).unwrap_or_else(
                            |e| {
                                if parse_err.is_none() {
                                    parse_err = Some(format!("invalid UTF-8 in CSV field: {e}"));
                                }
                                String::new()
                            },
                        );
                        fields.push(CsvField {
                            value: s,
                            was_quoted: false,
                        });
                        cur_quoted = false;
                        state = State::FieldStart;
                    }
                    b'\r' => {
                        state = State::ExpectRecordEndLf;
                    }
                    b'\n' => {
                        finished_record = true;
                        break;
                    }
                    b'"' => {
                        if parse_err.is_none() {
                            parse_err = Some("unescaped quote inside unquoted CSV field".to_string());
                        }
                    }
                    other => {
                        if parse_err.is_none() {
                            cur_bytes.push(other);
                        }
                    }
                },
                State::Quoted => {
                    if b == b'"' {
                        state = State::QuoteInQuoted;
                    } else if parse_err.is_none() {
                        cur_bytes.push(b);
                    }
                }
                State::QuoteInQuoted => match b {
                    b'"' => {
                        if parse_err.is_none() {
                            cur_bytes.push(b'"');
                        }
                        state = State::Quoted;
                    }
                    b',' => {
                        let s = String::from_utf8(std::mem::take(&mut cur_bytes)).unwrap_or_else(
                            |e| {
                                if parse_err.is_none() {
                                    parse_err = Some(format!("invalid UTF-8 in CSV field: {e}"));
                                }
                                String::new()
                            },
                        );
                        fields.push(CsvField {
                            value: s,
                            was_quoted: true,
                        });
                        cur_quoted = false;
                        state = State::FieldStart;
                    }
                    b'\r' => {
                        state = State::ExpectRecordEndLf;
                    }
                    b'\n' => {
                        finished_record = true;
                        break;
                    }
                    _ => {
                        if parse_err.is_none() {
                            parse_err =
                                Some("unexpected character after closing quote in CSV field".to_string());
                        }
                        state = State::Unquoted;
                    }
                },
            }
        }

        let chunk = &buf[..consumed];
        *fnv = fnv1a_update(*fnv, chunk);
        *bytes_read += consumed as u64;
        reader.consume(consumed);

        if finished_record {
            break;
        }
    }

    while matches!(raw.last(), Some(b'\n' | b'\r')) {
        raw.pop();
    }
    if raw.len() > max_row_bytes && parse_err.is_none() {
        parse_err = Some(format!(
            "CSV record length {} exceeds max_row_bytes ({max_row_bytes})",
            raw.len()
        ));
    }

    if let Some(msg) = parse_err {
        return Ok(CsvRecordOutcome::Error {
            raw,
            byte_offset: start_offset,
            message: msg,
        });
    }

    let last_val = match String::from_utf8(cur_bytes) {
        Ok(s) => s,
        Err(e) => {
            return Ok(CsvRecordOutcome::Error {
                raw,
                byte_offset: start_offset,
                message: format!("invalid UTF-8 in CSV field: {e}"),
            });
        }
    };
    fields.push(CsvField {
        value: last_val,
        was_quoted: cur_quoted,
    });

    Ok(CsvRecordOutcome::Record {
        fields,
        raw,
        byte_offset: start_offset,
    })
}

// ---------------------------------------------------------------------------
// Schema inference
// ---------------------------------------------------------------------------

fn merge_inferred_type(existing: Option<Type>, next: Type) -> Type {
    let Some(prev) = existing else {
        return next;
    };
    if prev == next {
        return prev;
    }
    match (prev, next) {
        (Type::I64, Type::F64) | (Type::F64, Type::I64) => Type::F64,
        _ => Type::Text,
    }
}

fn infer_json_val_type(val: &JsonVal) -> Option<Type> {
    match val {
        JsonVal::Null => None,
        JsonVal::Bool(_) => Some(Type::Bool),
        JsonVal::Int(_) => Some(Type::I64),
        JsonVal::Float(_) => Some(Type::F64),
        JsonVal::Str(_) => Some(Type::Text),
        JsonVal::Array(items) => {
            if !items.is_empty()
                && items
                    .iter()
                    .all(|x| matches!(x, JsonVal::Int(_) | JsonVal::Float(_)))
            {
                Some(Type::Vec)
            } else {
                Some(Type::Text)
            }
        }
        JsonVal::Object(_) => Some(Type::Text),
    }
}

fn infer_csv_scalar_type(s: &str, was_quoted: bool) -> Type {
    if was_quoted {
        return Type::Text;
    }
    let trimmed = s.trim();
    if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
        return Type::Bool;
    }
    if trimmed.parse::<i64>().is_ok() {
        return Type::I64;
    }
    if let Ok(f) = trimmed.parse::<f64>()
        && f.is_finite()
    {
        return Type::F64;
    }
    Type::Text
}

/// Sample up to `max_sample_rows` JSONL objects from `reader` and infer column types + nullability (`LIN-1853`).
pub fn infer_jsonl_schema<R: BufRead>(
    mut reader: R,
    max_sample_rows: usize,
    limits: &ImportLimits,
) -> Result<InferredSchemaReport, Error> {
    let mut fields: BTreeMap<String, Option<Type>> = BTreeMap::new();
    let mut non_null_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut nullable_fields = BTreeSet::new();
    let mut sampled_rows = 0usize;
    let mut bytes_read = 0u64;
    let mut fnv = fnv1a_init();

    while sampled_rows < max_sample_rows {
        match read_bounded_line(
            &mut reader,
            limits.max_row_bytes,
            limits.max_total_bytes,
            &mut bytes_read,
            &mut fnv,
        )? {
            BoundedReadOutcome::Eof => break,
            BoundedReadOutcome::RowTooLong { byte_offset, len, .. } => {
                return Err(Error::resource_limit(format!(
                    "row {} (byte {byte_offset}): length {len} exceeds max_row_bytes {}",
                    sampled_rows + 1,
                    limits.max_row_bytes
                )));
            }
            BoundedReadOutcome::Record { raw, byte_offset } => {
                if raw.iter().all(|b| b.is_ascii_whitespace()) {
                    continue;
                }
                let mut parser = JsonParser::new(&raw, limits.max_nesting_depth);
                let entries = parser.parse_top_object().map_err(|msg| {
                    Error::runtime(format!(
                        "row {} (byte {byte_offset}): {msg}",
                        sampled_rows + 1
                    ))
                })?;
                sampled_rows += 1;
                let mut present = BTreeSet::new();
                for (k, v) in entries {
                    present.insert(k.clone());
                    match infer_json_val_type(&v) {
                        Some(ty) => {
                            let merged = merge_inferred_type(fields.get(&k).copied().flatten(), ty);
                            fields.insert(k.clone(), Some(merged));
                            *non_null_counts.entry(k).or_insert(0) += 1;
                        }
                        None => {
                            fields.entry(k.clone()).or_insert(None);
                            nullable_fields.insert(k);
                        }
                    }
                }
                for existing_k in fields.keys() {
                    if !present.contains(existing_k) {
                        nullable_fields.insert(existing_k.clone());
                    }
                }
            }
        }
    }

    let mut resolved = BTreeMap::new();
    for (k, ty_opt) in fields {
        if non_null_counts.get(&k).copied().unwrap_or(0) < sampled_rows {
            nullable_fields.insert(k.clone());
        }
        resolved.insert(k, ty_opt.unwrap_or(Type::Text));
    }

    Ok(InferredSchemaReport {
        fields: resolved,
        nullable_fields,
        sampled_rows,
    })
}

/// Sample up to `max_sample_rows` CSV data rows (after the header row) and infer column types + nullability (`LIN-1853`).
pub fn infer_csv_schema<R: BufRead>(
    mut reader: R,
    max_sample_rows: usize,
    limits: &ImportLimits,
    mapping: &ImportSchemaMapping,
) -> Result<InferredSchemaReport, Error> {
    let mut bytes_read = 0u64;
    let mut fnv = fnv1a_init();

    let headers = match read_csv_record(
        &mut reader,
        limits.max_row_bytes,
        limits.max_total_bytes,
        &mut bytes_read,
        &mut fnv,
    )? {
        CsvRecordOutcome::Eof => {
            return Ok(InferredSchemaReport {
                fields: BTreeMap::new(),
                nullable_fields: BTreeSet::new(),
                sampled_rows: 0,
            });
        }
        CsvRecordOutcome::Error {
            byte_offset,
            message,
            ..
        } => {
            return Err(Error::runtime(format!(
                "CSV header (byte {byte_offset}): {message}"
            )));
        }
        CsvRecordOutcome::Record { fields, .. } => fields
            .into_iter()
            .map(|f| {
                mapping
                    .field_renames
                    .get(&f.value)
                    .cloned()
                    .unwrap_or(f.value)
            })
            .collect::<Vec<_>>(),
    };

    let mut inferred: BTreeMap<String, Option<Type>> = BTreeMap::new();
    for h in &headers {
        inferred.insert(h.clone(), mapping.explicit_types.get(h).copied());
    }
    let mut nullable_fields = BTreeSet::new();
    let mut sampled_rows = 0usize;

    while sampled_rows < max_sample_rows {
        match read_csv_record(
            &mut reader,
            limits.max_row_bytes,
            limits.max_total_bytes,
            &mut bytes_read,
            &mut fnv,
        )? {
            CsvRecordOutcome::Eof => break,
            CsvRecordOutcome::Error {
                byte_offset,
                message,
                ..
            } => {
                return Err(Error::runtime(format!(
                    "row {} (byte {byte_offset}): {message}",
                    sampled_rows + 1
                )));
            }
            CsvRecordOutcome::Record { fields, .. } => {
                if fields.len() == 1 && !fields[0].was_quoted && fields[0].value.is_empty() && headers.len() > 1 {
                    continue;
                }
                sampled_rows += 1;
                for (col_idx, h) in headers.iter().enumerate() {
                    let Some(cell_f) = fields.get(col_idx) else {
                        nullable_fields.insert(h.clone());
                        continue;
                    };
                    let is_null = !cell_f.was_quoted
                        && ((!mapping.null_sentinel.is_empty()
                            && cell_f.value == mapping.null_sentinel)
                            || (cell_f.value.is_empty() && mapping.empty_csv_as_null));
                    if is_null {
                        nullable_fields.insert(h.clone());
                        continue;
                    }
                    if let Some(&exp_ty) = mapping.explicit_types.get(h) {
                        inferred.insert(h.clone(), Some(exp_ty));
                    } else {
                        let ty = infer_csv_scalar_type(&cell_f.value, cell_f.was_quoted);
                        let merged =
                            merge_inferred_type(inferred.get(h).copied().flatten(), ty);
                        inferred.insert(h.clone(), Some(merged));
                    }
                }
            }
        }
    }

    let resolved = inferred
        .into_iter()
        .map(|(k, v)| (k, v.unwrap_or(Type::Text)))
        .collect();

    Ok(InferredSchemaReport {
        fields: resolved,
        nullable_fields,
        sampled_rows,
    })
}

// ---------------------------------------------------------------------------
// Streaming Importers (`import_jsonl_reader` & `import_csv_reader`)
// ---------------------------------------------------------------------------

fn ensure_collection_for_rows(
    db: &mut Db,
    collection: &str,
    rows: &[(usize, u64, Vec<u8>, Row)],
    mapping: &ImportSchemaMapping,
) -> Result<(), Error> {
    if db.catalog.collection(collection).is_some() {
        return Ok(());
    }
    let mut field_types: BTreeMap<String, Type> = BTreeMap::new();
    for (k, &ty) in &mapping.explicit_types {
        field_types.insert(k.clone(), ty);
    }
    for (k, cell) in &mapping.defaults {
        field_types
            .entry(k.clone())
            .or_insert_with(|| cell_type_fallback(cell));
    }
    for (_, _, _, row) in rows {
        for (k, cell) in row {
            if matches!(cell, Cell::Null) {
                continue;
            }
            let next_ty = cell_type_fallback(cell);
            let merged = merge_inferred_type(field_types.get(k).copied(), next_ty);
            field_types.insert(k.clone(), merged);
        }
    }
    if !field_types.contains_key("id") {
        field_types.insert("id".to_string(), Type::Text);
    }
    let mut decl = format!("col {collection} {{ ");
    for (idx, (fname, ty)) in field_types.iter().enumerate() {
        if idx > 0 {
            decl.push_str(", ");
        }
        decl.push_str(fname);
        decl.push_str(": ");
        if *ty == Type::Vec {
            let dim = rows
                .iter()
                .find_map(|(_, _, _, r)| match r.get(fname.as_str()) {
                    Some(Cell::Vec(v)) if !v.is_empty() => Some(v.len()),
                    _ => None,
                })
                .or_else(|| match mapping.defaults.get(fname) {
                    Some(Cell::Vec(v)) if !v.is_empty() => Some(v.len()),
                    _ => None,
                })
                .unwrap_or(32);
            let raw_model = db
                .catalog
                .embed_id
                .split('/')
                .next()
                .unwrap_or("hash");
            let clean_model: String = raw_model
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            let model = if clean_model.is_empty() {
                "hash"
            } else {
                clean_model.as_str()
            };
            decl.push_str(&format!("vec[{}]@{model}", dim.max(1)));
        } else {
            decl.push_str(ty.name());
        }
    }
    decl.push_str(" }");
    db.execute(&decl)?;
    Ok(())
}

fn cell_type_fallback(cell: &Cell) -> Type {
    match cell {
        Cell::Int(_) => Type::I64,
        Cell::Float(_) => Type::F64,
        Cell::Bool(_) => Type::Bool,
        Cell::Time(_) => Type::Time,
        Cell::Vec(_) => Type::Vec,
        _ => Type::Text,
    }
}

fn handle_row_rejection(
    row_number: usize,
    byte_offset: u64,
    raw_bytes: &[u8],
    message: String,
    reject_writer: &mut Option<&mut dyn Write>,
    receipt: &mut ImportReceipt,
    is_model_err: bool,
) -> Result<(), Error> {
    if let Some(w) = reject_writer.as_deref_mut() {
        w.write_all(raw_bytes)
            .and_then(|()| w.write_all(b"\n"))
            .map_err(|e| Error::io(format!("failed writing to reject sink: {e}")))?;
        receipt.rejected_rows += 1;
        receipt.errors.push(ImportRowError {
            row_number,
            byte_offset,
            message,
        });
        Ok(())
    } else {
        let full_msg = format!("import error at row {row_number} (byte {byte_offset}): {message}");
        if is_model_err {
            Err(Error::incompatible_model(full_msg))
        } else {
            Err(Error::runtime(full_msg))
        }
    }
}

fn commit_staged_batch(
    db: &mut Db,
    collection: &str,
    staged: &mut Vec<(usize, u64, Vec<u8>, Row)>,
    mapping: &ImportSchemaMapping,
    reject_writer: &mut Option<&mut dyn Write>,
    receipt: &mut ImportReceipt,
) -> Result<(), Error> {
    if staged.is_empty() {
        return Ok(());
    }
    ensure_collection_for_rows(db, collection, staged, mapping)?;

    let batch_items = std::mem::take(staged);
    let rows_only: Vec<Row> = batch_items.iter().map(|(_, _, _, r)| r.clone()).collect();
    match db.insert_imported_rows(collection, rows_only) {
        Ok(done) => {
            receipt.imported_rows += done.n;
            receipt.batches_committed += 1;
            Ok(())
        }
        Err(batch_err) => {
            if reject_writer.is_none() {
                let (first_row_num, first_off, _, _) = &batch_items[0];
                return Err(Error::runtime(format!(
                    "batch starting at row {first_row_num} (byte {first_off}): {}",
                    batch_err.message
                )));
            }
            // Reject mode: isolate offending row(s) so valid rows in the chunk still commit.
            let mut committed_any = false;
            for (row_num, byte_off, raw, single_row) in batch_items {
                match db.insert_imported_rows(collection, vec![single_row]) {
                    Ok(done) => {
                        receipt.imported_rows += done.n;
                        committed_any = true;
                    }
                    Err(row_err) => {
                        handle_row_rejection(
                            row_num,
                            byte_off,
                            &raw,
                            row_err.message,
                            reject_writer,
                            receipt,
                            false,
                        )?;
                    }
                }
            }
            if committed_any {
                receipt.batches_committed += 1;
            }
            Ok(())
        }
    }
}

fn target_field_type(
    db: &Db,
    collection: &str,
    field: &str,
    mapping: &ImportSchemaMapping,
) -> Option<Type> {
    if let Some(&ty) = mapping.explicit_types.get(field) {
        return Some(ty);
    }
    db.catalog
        .collection(collection)
        .and_then(|c| c.field(field))
        .map(|f| f.ty)
}

fn apply_defaults_and_dedup(
    db: &Db,
    collection: &str,
    row: &mut Row,
    mapping: &ImportSchemaMapping,
    seen_ids: &mut FxHashSet<String>,
) -> bool {
    if let Some(ref src_id) = mapping.id_field
        && src_id != "id"
        && row.get("id").is_none_or(Cell::is_null)
        && let Some(val) = row.get(src_id).cloned()
    {
        row.insert("id", val);
    }
    for (def_k, def_v) in &mapping.defaults {
        if row.get(def_k).is_none_or(Cell::is_null) {
            row.insert(def_k.as_str(), def_v.clone());
        }
    }
    // Strip explicit Null cells so sparse Row semantics match Lin insert rules
    row.retain(|_, c| !matches!(c, Cell::Null));

    if mapping.deduplicate_ids
        && let Some(id) = row_text(row, "id")
    {
        if !seen_ids.insert(id.to_string()) || db.store.row_index(collection, id).is_some() {
            return false;
        }
    }
    true
}

/// Stream JSON Lines (`JSONL`) records from `reader` into `collection` in bounded `batch_size` commits
/// (`LIN-1841`, `LIN-1843`, `LIN-1854`..`LIN-1872`, `LIN-1880`).
#[allow(clippy::too_many_arguments)]
pub fn import_jsonl_reader<R: BufRead>(
    db: &mut Db,
    collection: &str,
    reader: &mut R,
    limits: &ImportLimits,
    mapping: &ImportSchemaMapping,
    mut reject_writer: Option<&mut dyn Write>,
    on_progress: &mut dyn FnMut(u64, usize),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<ImportReceipt, Error> {
    let batch_cap = limits.batch_size.max(1);
    let mut staged: Vec<(usize, u64, Vec<u8>, Row)> = Vec::with_capacity(batch_cap.min(1024));
    let mut receipt = ImportReceipt {
        imported_rows: 0,
        skipped_rows: 0,
        rejected_rows: 0,
        bytes_read: 0,
        batches_committed: 0,
        cancelled: false,
        source_sha256_fnv: fnv1a_init(),
        errors: Vec::new(),
    };
    let mut seen_ids: FxHashSet<String> = FxHashSet::default();
    let mut data_row_index = 0usize;

    loop {
        if is_cancelled() {
            commit_staged_batch(
                db,
                collection,
                &mut staged,
                mapping,
                &mut reject_writer,
                &mut receipt,
            )?;
            receipt.cancelled = true;
            return Ok(receipt);
        }

        let outcome = read_bounded_line(
            reader,
            limits.max_row_bytes,
            limits.max_total_bytes,
            &mut receipt.bytes_read,
            &mut receipt.source_sha256_fnv,
        )?;

        match outcome {
            BoundedReadOutcome::Eof => break,
            BoundedReadOutcome::RowTooLong {
                raw,
                byte_offset,
                len,
            } => {
                data_row_index += 1;
                handle_row_rejection(
                    data_row_index,
                    byte_offset,
                    &raw,
                    format!(
                        "JSONL line length {len} exceeds max_row_bytes {}",
                        limits.max_row_bytes
                    ),
                    &mut reject_writer,
                    &mut receipt,
                    false,
                )?;
            }
            BoundedReadOutcome::Record { raw, byte_offset } => {
                if raw.iter().all(|b| b.is_ascii_whitespace()) {
                    continue;
                }
                data_row_index += 1;
                if data_row_index <= mapping.start_row_offset {
                    receipt.skipped_rows += 1;
                    continue;
                }

                let mut parser = JsonParser::new(&raw, limits.max_nesting_depth);
                let entries = match parser.parse_top_object() {
                    Ok(e) => e,
                    Err(msg) => {
                        handle_row_rejection(
                            data_row_index,
                            byte_offset,
                            &raw,
                            msg,
                            &mut reject_writer,
                            &mut receipt,
                            false,
                        )?;
                        continue;
                    }
                };

                let mut row = Row::new();
                let mut conv_err: Option<(String, bool)> = None;
                for (src_key, val) in entries {
                    let dst_key = mapping
                        .field_renames
                        .get(&src_key)
                        .cloned()
                        .unwrap_or(src_key);
                    let target_ty = target_field_type(db, collection, &dst_key, mapping);
                    match json_val_to_cell(val, target_ty, limits.expected_vector_dim) {
                        Ok(cell) => {
                            if !matches!(cell, Cell::Null) {
                                row.insert(dst_key, cell);
                            }
                        }
                        Err(msg) => {
                            let is_model = msg.contains("embedding dimension mismatch");
                            conv_err = Some((format!("field `{dst_key}`: {msg}"), is_model));
                            break;
                        }
                    }
                }

                if let Some((msg, is_model)) = conv_err {
                    handle_row_rejection(
                        data_row_index,
                        byte_offset,
                        &raw,
                        msg,
                        &mut reject_writer,
                        &mut receipt,
                        is_model,
                    )?;
                    continue;
                }

                if !apply_defaults_and_dedup(db, collection, &mut row, mapping, &mut seen_ids) {
                    receipt.skipped_rows += 1;
                    continue;
                }

                staged.push((data_row_index, byte_offset, raw, row));
                if staged.len() >= batch_cap {
                    commit_staged_batch(
                        db,
                        collection,
                        &mut staged,
                        mapping,
                        &mut reject_writer,
                        &mut receipt,
                    )?;
                    on_progress(receipt.bytes_read, receipt.imported_rows);
                }
            }
        }
    }

    if !staged.is_empty() {
        commit_staged_batch(
            db,
            collection,
            &mut staged,
            mapping,
            &mut reject_writer,
            &mut receipt,
        )?;
        on_progress(receipt.bytes_read, receipt.imported_rows);
    }

    Ok(receipt)
}

fn coerce_csv_cell(
    raw_val: &str,
    was_quoted: bool,
    target_ty: Option<Type>,
    mapping: &ImportSchemaMapping,
    expected_vec_dim: Option<usize>,
) -> Result<Cell, (String, bool)> {
    if !was_quoted {
        if !mapping.null_sentinel.is_empty() && raw_val == mapping.null_sentinel {
            return Ok(Cell::Null);
        }
        if raw_val.is_empty() && mapping.empty_csv_as_null {
            return Ok(Cell::Null);
        }
    }
    match target_ty {
        Some(Type::Text | Type::Var | Type::Id | Type::Uri | Type::Rel) => {
            Ok(Cell::from_text(raw_val))
        }
        Some(Type::I64) => raw_val
            .trim()
            .parse::<i64>()
            .map(Cell::Int)
            .map_err(|_| (format!("cannot parse `{raw_val}` as i64"), false)),
        Some(Type::F64) => raw_val
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .map(Cell::Float)
            .ok_or_else(|| (format!("cannot parse `{raw_val}` as finite f64"), false)),
        Some(Type::Bool) => match raw_val.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(Cell::Bool(true)),
            "false" | "0" => Ok(Cell::Bool(false)),
            _ => Err((format!("cannot parse `{raw_val}` as bool"), false)),
        },
        Some(Type::Time | Type::Dur) => raw_val
            .trim()
            .parse::<i64>()
            .map(Cell::Time)
            .map_err(|_| (format!("cannot parse `{raw_val}` as timestamp i64"), false)),
        Some(Type::Vec) => parse_vec_str(raw_val, expected_vec_dim)
            .map_err(|msg| {
                let is_model = msg.contains("embedding dimension mismatch");
                (msg, is_model)
            }),
        None => {
            if was_quoted {
                return Ok(Cell::from_text(raw_val));
            }
            let trimmed = raw_val.trim();
            if trimmed.eq_ignore_ascii_case("true") {
                return Ok(Cell::Bool(true));
            }
            if trimmed.eq_ignore_ascii_case("false") {
                return Ok(Cell::Bool(false));
            }
            if let Ok(i) = trimmed.parse::<i64>() {
                return Ok(Cell::Int(i));
            }
            if let Ok(f) = trimmed.parse::<f64>()
                && f.is_finite()
            {
                return Ok(Cell::Float(f));
            }
            Ok(Cell::from_text(raw_val))
        }
    }
}

/// Stream RFC-4180 CSV records from `reader` into `collection` in bounded `batch_size` commits
/// (`LIN-1842`, `LIN-1849`, `LIN-1850`, `LIN-1854`..`LIN-1872`, `LIN-1880`).
#[allow(clippy::too_many_arguments)]
pub fn import_csv_reader<R: BufRead>(
    db: &mut Db,
    collection: &str,
    reader: &mut R,
    limits: &ImportLimits,
    mapping: &ImportSchemaMapping,
    mut reject_writer: Option<&mut dyn Write>,
    on_progress: &mut dyn FnMut(u64, usize),
    is_cancelled: &dyn Fn() -> bool,
) -> Result<ImportReceipt, Error> {
    let batch_cap = limits.batch_size.max(1);
    let mut receipt = ImportReceipt {
        imported_rows: 0,
        skipped_rows: 0,
        rejected_rows: 0,
        bytes_read: 0,
        batches_committed: 0,
        cancelled: false,
        source_sha256_fnv: fnv1a_init(),
        errors: Vec::new(),
    };

    let headers = match read_csv_record(
        reader,
        limits.max_row_bytes,
        limits.max_total_bytes,
        &mut receipt.bytes_read,
        &mut receipt.source_sha256_fnv,
    )? {
        CsvRecordOutcome::Eof => return Ok(receipt),
        CsvRecordOutcome::Error {
            byte_offset,
            message,
            ..
        } => {
            return Err(Error::runtime(format!(
                "CSV header error at byte {byte_offset}: {message}"
            )));
        }
        CsvRecordOutcome::Record { fields, .. } => fields
            .into_iter()
            .map(|f| {
                mapping
                    .field_renames
                    .get(&f.value)
                    .cloned()
                    .unwrap_or(f.value)
            })
            .collect::<Vec<_>>(),
    };

    let mut staged: Vec<(usize, u64, Vec<u8>, Row)> = Vec::with_capacity(batch_cap.min(1024));
    let mut seen_ids: FxHashSet<String> = FxHashSet::default();
    let mut data_row_index = 0usize;

    loop {
        if is_cancelled() {
            commit_staged_batch(
                db,
                collection,
                &mut staged,
                mapping,
                &mut reject_writer,
                &mut receipt,
            )?;
            receipt.cancelled = true;
            return Ok(receipt);
        }

        let outcome = read_csv_record(
            reader,
            limits.max_row_bytes,
            limits.max_total_bytes,
            &mut receipt.bytes_read,
            &mut receipt.source_sha256_fnv,
        )?;

        match outcome {
            CsvRecordOutcome::Eof => break,
            CsvRecordOutcome::Error {
                raw,
                byte_offset,
                message,
            } => {
                data_row_index += 1;
                handle_row_rejection(
                    data_row_index,
                    byte_offset,
                    &raw,
                    message,
                    &mut reject_writer,
                    &mut receipt,
                    false,
                )?;
            }
            CsvRecordOutcome::Record {
                fields,
                raw,
                byte_offset,
            } => {
                // Ignore a trailing completely empty line
                if fields.len() == 1
                    && !fields[0].was_quoted
                    && fields[0].value.is_empty()
                    && headers.len() > 1
                {
                    continue;
                }
                data_row_index += 1;
                if data_row_index <= mapping.start_row_offset {
                    receipt.skipped_rows += 1;
                    continue;
                }

                if fields.len() != headers.len() {
                    handle_row_rejection(
                        data_row_index,
                        byte_offset,
                        &raw,
                        format!(
                            "CSV column count mismatch: expected {}, got {}",
                            headers.len(),
                            fields.len()
                        ),
                        &mut reject_writer,
                        &mut receipt,
                        false,
                    )?;
                    continue;
                }

                let mut row = Row::new();
                let mut conv_err: Option<(String, bool)> = None;
                for (h, field) in headers.iter().zip(fields.iter()) {
                    let target_ty = target_field_type(db, collection, h, mapping);
                    match coerce_csv_cell(
                        &field.value,
                        field.was_quoted,
                        target_ty,
                        mapping,
                        limits.expected_vector_dim,
                    ) {
                        Ok(cell) => {
                            if !matches!(cell, Cell::Null) {
                                row.insert(h.as_str(), cell);
                            }
                        }
                        Err((msg, is_model)) => {
                            conv_err = Some((format!("field `{h}`: {msg}"), is_model));
                            break;
                        }
                    }
                }

                if let Some((msg, is_model)) = conv_err {
                    handle_row_rejection(
                        data_row_index,
                        byte_offset,
                        &raw,
                        msg,
                        &mut reject_writer,
                        &mut receipt,
                        is_model,
                    )?;
                    continue;
                }

                if !apply_defaults_and_dedup(db, collection, &mut row, mapping, &mut seen_ids) {
                    receipt.skipped_rows += 1;
                    continue;
                }

                staged.push((data_row_index, byte_offset, raw, row));
                if staged.len() >= batch_cap {
                    commit_staged_batch(
                        db,
                        collection,
                        &mut staged,
                        mapping,
                        &mut reject_writer,
                        &mut receipt,
                    )?;
                    on_progress(receipt.bytes_read, receipt.imported_rows);
                }
            }
        }
    }

    if !staged.is_empty() {
        commit_staged_batch(
            db,
            collection,
            &mut staged,
            mapping,
            &mut reject_writer,
            &mut receipt,
        )?;
        on_progress(receipt.bytes_read, receipt.imported_rows);
    }

    Ok(receipt)
}

// ---------------------------------------------------------------------------
// Deterministic Streaming Exporters (`export_jsonl_writer` & `export_csv_writer`)
// ---------------------------------------------------------------------------

fn write_cell_json(out: &mut String, cell: &Cell) {
    use std::fmt::Write as _;
    match cell {
        Cell::Null => out.push_str("null"),
        Cell::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Cell::Int(n) | Cell::Time(n) => {
            let _ = write!(out, "{n}");
        }
        Cell::Float(f) => {
            if f.is_finite() {
                let _ = write!(out, "{f}");
            } else {
                out.push_str("null");
            }
        }
        Cell::Text(s) => write_json_escaped_str(out, s),
        Cell::SmallText(s) => write_json_escaped_str(out, s.as_str()),
        Cell::Vec(v) => {
            out.push('[');
            for (i, &x) in v.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                if x.is_finite() {
                    let _ = write!(out, "{x}");
                } else {
                    out.push_str("null");
                }
            }
            out.push(']');
        }
    }
}

fn format_row_jsonl(row: &Row, line: &mut String) {
    line.clear();
    line.push('{');
    let mut first = true;
    // `Row::iter()` iterates keys in deterministic sorted order (`Shape::names`).
    for (k, v) in row {
        if !first {
            line.push(',');
        }
        first = false;
        write_json_escaped_str(line, k);
        line.push(':');
        write_cell_json(line, v);
    }
    line.push_str("}\n");
}

fn query_has_explicit_sort(q: &Query) -> bool {
    q.steps
        .iter()
        .any(|s| matches!(s, Step::Sort { .. } | Step::Search { .. } | Step::ArgExtremum { .. }))
}

fn sort_rows_deterministically(rows: &mut [Row]) {
    rows.sort_by(|a, b| match (row_text(a, "id"), row_text(b, "id")) {
        (Some(ia), Some(ib)) => ia.cmp(ib),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => {
            let mut ka = String::new();
            let mut kb = String::new();
            format_row_jsonl(a, &mut ka);
            format_row_jsonl(b, &mut kb);
            ka.cmp(&kb)
        }
    });
}

/// Stream query results as deterministic JSON Lines (`JSONL`) into `writer`, returning exported row count
/// (`LIN-1845`, `LIN-1868`).
pub fn export_jsonl_writer<W: Write>(
    db: &mut Db,
    query: &str,
    writer: &mut W,
) -> Result<usize, Error> {
    let stmt = crate::parse(query)?;
    let mut line = String::with_capacity(256);
    if let Stmt::Query(q) = &stmt
        && query_has_explicit_sort(q)
    {
        let cursor = db.cursor(q)?;
        let mut count = 0usize;
        for res in cursor {
            let row = res?;
            format_row_jsonl(&row, &mut line);
            writer
                .write_all(line.as_bytes())
                .map_err(|e| Error::io(format!("JSONL export write error: {e}")))?;
            count += 1;
        }
        writer
            .flush()
            .map_err(|e| Error::io(format!("JSONL export flush error: {e}")))?;
        return Ok(count);
    }

    let mut rows = db.run(query)?.into_rows();
    if let Stmt::Query(q) = &stmt
        && !query_has_explicit_sort(q)
    {
        sort_rows_deterministically(&mut rows);
    }
    let count = rows.len();
    for row in &rows {
        format_row_jsonl(row, &mut line);
        writer
            .write_all(line.as_bytes())
            .map_err(|e| Error::io(format!("JSONL export write error: {e}")))?;
    }
    writer
        .flush()
        .map_err(|e| Error::io(format!("JSONL export flush error: {e}")))?;
    Ok(count)
}

fn write_csv_field<W: Write>(writer: &mut W, s: &str, force_quotes: bool) -> Result<(), Error> {
    let needs_quotes =
        force_quotes || s.bytes().any(|b| matches!(b, b',' | b'"' | b'\n' | b'\r'));
    if !needs_quotes {
        return writer
            .write_all(s.as_bytes())
            .map_err(|e| Error::io(format!("CSV export write error: {e}")));
    }
    writer
        .write_all(b"\"")
        .map_err(|e| Error::io(format!("CSV export write error: {e}")))?;
    for &b in s.as_bytes() {
        if b == b'"' {
            writer
                .write_all(b"\"\"")
                .map_err(|e| Error::io(format!("CSV export write error: {e}")))?;
        } else {
            writer
                .write_all(&[b])
                .map_err(|e| Error::io(format!("CSV export write error: {e}")))?;
        }
    }
    writer
        .write_all(b"\"")
        .map_err(|e| Error::io(format!("CSV export write error: {e}")))
}

fn write_csv_cell<W: Write>(writer: &mut W, cell: &Cell) -> Result<(), Error> {
    match cell {
        Cell::Null => Ok(()),
        Cell::Bool(b) => writer
            .write_all(if *b { b"true" } else { b"false" })
            .map_err(|e| Error::io(format!("CSV export write error: {e}"))),
        Cell::Int(n) | Cell::Time(n) => {
            write!(writer, "{n}").map_err(|e| Error::io(format!("CSV export write error: {e}")))
        }
        Cell::Float(f) => {
            if f.is_finite() {
                write!(writer, "{f}")
                    .map_err(|e| Error::io(format!("CSV export write error: {e}")))
            } else {
                Ok(())
            }
        }
        Cell::Text(s) => {
            // Quote empty string `""` so RFC-4180 round-trips distinguish `""` from unquoted null `,,`.
            write_csv_field(writer, s, s.is_empty())
        }
        Cell::SmallText(s) => {
            let txt = s.as_str();
            write_csv_field(writer, txt, txt.is_empty())
        }
        Cell::Vec(v) => {
            let mut buf = String::from("[");
            for (i, &x) in v.iter().enumerate() {
                if i > 0 {
                    buf.push(',');
                }
                use std::fmt::Write as _;
                let _ = write!(&mut buf, "{x}");
            }
            buf.push(']');
            write_csv_field(writer, &buf, true)
        }
    }
}

fn determine_csv_columns(db: &Db, stmt: &Stmt, rows: &[Row]) -> Vec<String> {
    if let Stmt::Query(q) = stmt {
        for step in q.steps.iter().rev() {
            if let Step::Project(fields) = step {
                return fields.iter().map(|f| f.as_str()).collect();
            }
        }
    }
    let mut cols = BTreeSet::new();
    for r in rows {
        for k in r.keys() {
            cols.insert(k.clone());
        }
    }
    if cols.is_empty()
        && let Stmt::Query(q) = stmt
        && let Source::Collection(c) = &q.source
        && let Some(cdef) = db.catalog.collection(c)
    {
        for k in cdef.fields.keys() {
            cols.insert(k.clone());
        }
    }
    let mut ordered = Vec::with_capacity(cols.len());
    if cols.remove("id") {
        ordered.push("id".to_string());
    }
    ordered.extend(cols);
    ordered
}

/// Export query results as deterministic RFC-4180 CSV into `writer`, returning exported row count
/// (`LIN-1846`, `LIN-1849`, `LIN-1850`, `LIN-1868`).
pub fn export_csv_writer<W: Write>(
    db: &mut Db,
    query: &str,
    writer: &mut W,
) -> Result<usize, Error> {
    let stmt = crate::parse(query)?;
    let mut rows = db.run(query)?.into_rows();
    if let Stmt::Query(q) = &stmt
        && !query_has_explicit_sort(q)
    {
        sort_rows_deterministically(&mut rows);
    }

    let columns = determine_csv_columns(db, &stmt, &rows);
    if !columns.is_empty() {
        for (i, col_name) in columns.iter().enumerate() {
            if i > 0 {
                writer
                    .write_all(b",")
                    .map_err(|e| Error::io(format!("CSV export write error: {e}")))?;
            }
            write_csv_field(writer, col_name, false)?;
        }
        writer
            .write_all(b"\r\n")
            .map_err(|e| Error::io(format!("CSV export write error: {e}")))?;
    }

    for row in &rows {
        for (i, col_name) in columns.iter().enumerate() {
            if i > 0 {
                writer
                    .write_all(b",")
                    .map_err(|e| Error::io(format!("CSV export write error: {e}")))?;
            }
            let cell = row.get(col_name.as_str()).unwrap_or(&Cell::Null);
            write_csv_cell(writer, cell)?;
        }
        writer
            .write_all(b"\r\n")
            .map_err(|e| Error::io(format!("CSV export write error: {e}")))?;
    }
    writer
        .flush()
        .map_err(|e| Error::io(format!("CSV export flush error: {e}")))?;
    Ok(rows.len())
}

// ---------------------------------------------------------------------------
// Columnar Wire IPC Batch Interchange (`LIN-1844`, `LIN-1847`, `LIN-1878`)
// ---------------------------------------------------------------------------

/// Write checksummed `RecordBatch` frames (`LINIPC01` + length-prefixed `LINB\x01` wire payloads)
/// to `writer`, returning total rows written (`LIN-1844`, `LIN-1847`, `LIN-1878`).
pub fn export_columnar_ipc_batches<W: Write>(
    batches: &[RecordBatch],
    writer: &mut W,
) -> Result<usize, Error> {
    writer
        .write_all(IPC_STREAM_MAGIC)
        .map_err(|e| Error::io(format!("IPC write header error: {e}")))?;
    let batch_count = u32::try_from(batches.len())
        .map_err(|_| Error::resource_limit("IPC batch count exceeds u32::MAX"))?;
    writer
        .write_all(&batch_count.to_le_bytes())
        .map_err(|e| Error::io(format!("IPC write count error: {e}")))?;

    let mut total_rows = 0usize;
    for batch in batches {
        let wire = batch.to_wire_bytes();
        let frame_len = u32::try_from(wire.len())
            .map_err(|_| Error::resource_limit("IPC frame exceeds u32::MAX bytes"))?;
        writer
            .write_all(&frame_len.to_le_bytes())
            .and_then(|()| writer.write_all(&wire))
            .map_err(|e| Error::io(format!("IPC write frame error: {e}")))?;
        total_rows += batch.n();
    }
    writer
        .flush()
        .map_err(|e| Error::io(format!("IPC flush error: {e}")))?;
    Ok(total_rows)
}

/// Read checksummed `RecordBatch` frames from `reader` within `limits`, validating magic, frame sizes,
/// and per-batch `LINB\x01` + `LCRC` CRC32C trailers (`LIN-1844`, `LIN-1847`, `LIN-1878`).
pub fn import_columnar_ipc_batches<R: BufRead>(
    reader: &mut R,
    limits: &ImportLimits,
) -> Result<Vec<RecordBatch>, Error> {
    let mut magic = [0u8; 8];
    reader
        .read_exact(&mut magic)
        .map_err(|e| Error::io(format!("IPC read header error: {e}")))?;
    if &magic != IPC_STREAM_MAGIC {
        return Err(Error::corrupt("IPC stream: bad magic header"));
    }

    let mut cnt_buf = [0u8; 4];
    reader
        .read_exact(&mut cnt_buf)
        .map_err(|e| Error::io(format!("IPC read count error: {e}")))?;
    let batch_count = u32::from_le_bytes(cnt_buf) as usize;

    let mut total_bytes = 12usize;
    if total_bytes > limits.max_total_bytes {
        return Err(Error::resource_limit("IPC stream exceeds max_total_bytes"));
    }

    let mut out = Vec::with_capacity(batch_count.min(1024));
    for _ in 0..batch_count {
        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|e| Error::io(format!("IPC truncated frame length: {e}")))?;
        let frame_len = u32::from_le_bytes(len_buf) as usize;
        total_bytes = total_bytes
            .checked_add(4)
            .and_then(|n| n.checked_add(frame_len))
            .ok_or_else(|| Error::resource_limit("IPC byte counter overflow"))?;
        if total_bytes > limits.max_total_bytes {
            return Err(Error::resource_limit(format!(
                "IPC stream exceeded max_total_bytes ({})",
                limits.max_total_bytes
            )));
        }
        let mut payload = vec![0u8; frame_len];
        reader
            .read_exact(&mut payload)
            .map_err(|e| Error::corrupt(format!("IPC truncated frame payload: {e}")))?;
        let batch = RecordBatch::from_wire_bytes(&payload)?;
        out.push(batch);
    }
    Ok(out)
}
