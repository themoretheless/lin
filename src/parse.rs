use crate::ast::*;
use crate::error::Error;

pub const DEFAULT_MAX_QUERY_BYTES: usize = 16 * 1024 * 1024;
pub const DEFAULT_MAX_AST_DEPTH: usize = 128;
/// Surface language grammar version (`LIN-0472`).
pub const SYNTAX_VERSION: u32 = 1;
/// Maximum decoded UTF-8 byte length of a single unescaped string literal (`4 MiB`, `LIN-0448`).
pub const MAX_DECODED_STRING_BYTES: usize = 4 * 1024 * 1024;

/// Convert a byte offset in `src` into 1-based `(line, col)` coordinates (`LIN-0441`..`LIN-0444`).
pub fn offset_to_line_col(src: &str, offset: usize) -> (usize, usize) {
    let clamped = offset.min(src.len());
    let mut boundary = clamped;
    while boundary > 0 && !src.is_char_boundary(boundary) {
        boundary -= 1;
    }
    let prefix = &src[..boundary];
    let line = 1 + prefix.as_bytes().iter().filter(|&&b| b == b'\n').count();
    let line_start = prefix.rfind('\n').map_or(0, |idx| idx + 1);
    let col = 1 + src[line_start..boundary].chars().count();
    (line, col)
}

fn token_preview(rest: &str) -> String {
    let trimmed = rest.trim_start();
    if trimmed.is_empty() {
        return "EOF".into();
    }
    let mut end = 0usize;
    for (i, ch) in trimmed.char_indices() {
        if i == 0 {
            end = ch.len_utf8();
            if !(ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '"') {
                break;
            }
            continue;
        }
        if ch.is_whitespace() || matches!(ch, '|' | ',' | ')' | ']' | '}' | '{' | '[' | '(') {
            break;
        }
        end = i + ch.len_utf8();
        if end >= 24 {
            break;
        }
    }
    let snippet = &trimmed[..end];
    format!("{snippet:?}")
}

pub fn parse_pred_src(src: &str) -> Result<Pred, Error> {
    parse_pred_src_with_limits(src, DEFAULT_MAX_QUERY_BYTES, DEFAULT_MAX_AST_DEPTH)
}

pub fn parse_pred_src_with_limits(
    src: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<Pred, Error> {
    if src.len() > max_bytes {
        return Err(Error::new(format!(
            "query length {} exceeds maximum allowed {} bytes",
            src.len(),
            max_bytes
        )));
    }
    let mut p = Parser::new_with_limits(src, max_depth);
    p.skip();
    let pred = p.parse_pred()?;
    p.skip();
    if !p.eof() {
        let preview = token_preview(p.rest());
        return Err(p.err(format!("unexpected trailing input {preview}")));
    }
    Ok(pred)
}

pub fn parse(src: &str) -> Result<Stmt, Error> {
    parse_with_limits(src, DEFAULT_MAX_QUERY_BYTES, DEFAULT_MAX_AST_DEPTH)
}

pub fn parse_with_limits(src: &str, max_bytes: usize, max_depth: usize) -> Result<Stmt, Error> {
    if src.len() > max_bytes {
        return Err(Error::new(format!(
            "query length {} exceeds maximum allowed {} bytes",
            src.len(),
            max_bytes
        )));
    }
    let mut p = Parser::new_with_limits(src, max_depth);
    p.skip();
    let stmt = p.parse_stmt()?;
    p.skip();
    if !p.eof() {
        let preview = token_preview(p.rest());
        return Err(p.err(format!("unexpected trailing input {preview}")));
    }
    Ok(stmt)
}

pub fn parse_program(src: &str) -> Result<Vec<Stmt>, Error> {
    parse_program_with_limits(src, DEFAULT_MAX_QUERY_BYTES, DEFAULT_MAX_AST_DEPTH)
}

/// Fast path for interactive transactions: parse a single-row `insert <col> [{ ... }]`
/// or `insert <col> { ... }` without allocating `Stmt`, `Vec<Record>`, or field `String`s.
pub(crate) fn try_parse_single_insert(
    src: &str,
) -> Option<(&str, smallvec::SmallVec<[(&str, Value); 8]>)> {
    let mut p = Parser::new_with_limits(src, DEFAULT_MAX_AST_DEPTH);
    if !p.eat_kw("insert") {
        return None;
    }
    let collection = p.eat_ident_str()?;
    let bracket = p.eat_char('[');
    if !p.eat_char('{') {
        return None;
    }
    let mut fields = smallvec::SmallVec::new();
    loop {
        p.skip();
        if p.peek() == Some('}') {
            break;
        }
        let name = p.eat_ident_str()?;
        if !p.eat_char(':') {
            return None;
        }
        let value = p.parse_value().ok()?;
        fields.push((name, value));
        let comma = p.eat_char(',');
        p.skip();
        if p.peek() == Some('}') {
            break;
        }
        if !comma {
            return None;
        }
    }
    if !p.eat_char('}') || fields.is_empty() {
        return None;
    }
    if bracket && !p.eat_char(']') {
        return None;
    }
    p.skip();
    if !p.eof() {
        return None;
    }
    Some((collection, fields))
}

pub fn parse_program_with_limits(
    src: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<Vec<Stmt>, Error> {
    if src.len() > max_bytes {
        return Err(Error::new(format!(
            "query length {} exceeds maximum allowed {} bytes",
            src.len(),
            max_bytes
        )));
    }
    let mut p = Parser::new_with_limits(src, max_depth);
    let mut stmts = Vec::new();
    loop {
        p.skip();
        if p.eof() {
            break;
        }
        stmts.push(p.parse_stmt()?);
    }
    if stmts.is_empty() {
        return Err(Error::new("empty program"));
    }
    Ok(stmts)
}

struct Parser<'a> {
    src: &'a str,
    pos: usize,
    depth: usize,
    max_depth: usize,
}

impl<'a> Parser<'a> {
    #[allow(dead_code)]
    fn new(src: &'a str) -> Self {
        Self::new_with_limits(src, DEFAULT_MAX_AST_DEPTH)
    }

    fn new_with_limits(src: &'a str, max_depth: usize) -> Self {
        Self {
            src,
            pos: 0,
            depth: 0,
            max_depth,
        }
    }

    #[inline]
    fn enter_depth(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(self.err(format!(
                "AST recursion depth limit exceeded (max {})",
                self.max_depth
            )));
        }
        Ok(())
    }

    #[inline]
    fn leave_depth(&mut self) {
        if self.depth > 0 {
            self.depth -= 1;
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    #[inline]
    fn peek(&self) -> Option<char> {
        let b = *self.src.as_bytes().get(self.pos)?;
        if b < 0x80 {
            Some(b as char)
        } else {
            self.rest().chars().next()
        }
    }

    #[inline]
    fn bump(&mut self) -> Option<char> {
        let b = *self.src.as_bytes().get(self.pos)?;
        if b < 0x80 {
            self.pos += 1;
            Some(b as char)
        } else {
            let mut cs = self.rest().chars();
            let c = cs.next()?;
            self.pos += c.len_utf8();
            Some(c)
        }
    }

    fn err(&self, msg: impl Into<String>) -> Error {
        let m = msg.into();
        let (line, col) = offset_to_line_col(self.src, self.pos);
        Error::at(self.pos, format!("{m} (line {line}:{col})"))
    }

    fn err_at(&self, pos: usize, msg: impl Into<String>) -> Error {
        let m = msg.into();
        let (line, col) = offset_to_line_col(self.src, pos);
        Error::at(pos, format!("{m} (line {line}:{col})"))
    }

    #[inline]
    fn skip(&mut self) {
        self.pos = self.skip_offset();
    }

    #[inline]
    fn skip_offset(&self) -> usize {
        let mut i = self.pos;
        let bytes = self.src.as_bytes();
        loop {
            while i < bytes.len() {
                let b = bytes[i];
                if b.is_ascii_whitespace() {
                    i += 1;
                } else if b >= 0x80 {
                    if let Some(c) = self.src[i..].chars().next()
                        && c.is_whitespace()
                    {
                        i += c.len_utf8();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'-' {
                i += 2;
                match memchr::memchr(b'\n', &bytes[i..]) {
                    Some(nl) => {
                        i += nl + 1;
                        continue;
                    }
                    None => return bytes.len(),
                }
            }
            break;
        }
        i
    }

    #[inline]
    fn peek_after_skip(&self) -> Option<char> {
        let i = self.skip_offset();
        let b = *self.src.as_bytes().get(i)?;
        if b < 0x80 {
            Some(b as char)
        } else {
            self.src[i..].chars().next()
        }
    }

    #[inline]
    fn starts_ident(&self) -> bool {
        self.src
            .as_bytes()
            .get(self.pos)
            .is_some_and(|&b| is_ident_start_byte(b))
    }

    #[inline]
    fn peek_ident(&self) -> Option<&'a str> {
        let start = self.skip_offset();
        let bytes = self.src.as_bytes();
        let &first = bytes.get(start)?;
        if !is_ident_start_byte(first) {
            return None;
        }
        let mut end = start + 1;
        while end < bytes.len() && is_ident_continue_byte(bytes[end]) {
            end += 1;
        }
        Some(&self.src[start..end])
    }

    #[inline]
    fn eat_ident_str(&mut self) -> Option<&'a str> {
        self.skip();
        let bytes = self.src.as_bytes();
        let start = self.pos;
        let &first = bytes.get(start)?;
        if !is_ident_start_byte(first) {
            return None;
        }
        let mut end = start + 1;
        while end < bytes.len() && is_ident_continue_byte(bytes[end]) {
            end += 1;
        }
        self.pos = end;
        Some(&self.src[start..end])
    }

    fn expect_ident_str(&mut self) -> Result<&'a str, Error> {
        self.eat_ident_str().ok_or_else(|| {
            if self.eof() {
                self.err("expected identifier, found EOF")
            } else {
                let tok = token_preview(self.rest());
                self.err(format!("expected identifier, unexpected token {tok}"))
            }
        })
    }

    fn expect_ident(&mut self) -> Result<String, Error> {
        self.expect_ident_str().map(str::to_owned)
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        let save = self.pos;
        if self.eat_ident_str() == Some(kw) {
            return true;
        }
        self.pos = save;
        false
    }

    fn parse_delimited<T>(
        &mut self,
        open: char,
        close: char,
        sep_err: &'static str,
        empty_err: Option<&'static str>,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        self.expect_char(open)?;
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some(close) {
                break;
            }
            items.push(parse_item(self)?);
            let comma = self.eat_char(',');
            self.skip();
            if self.peek() == Some(close) {
                break;
            }
            if !comma {
                return Err(self.err(sep_err));
            }
        }
        self.expect_char(close)?;
        if items.is_empty()
            && let Some(err) = empty_err
        {
            return Err(self.err(err));
        }
        Ok(items)
    }

    fn expect_kw(&mut self, kw: &str) -> Result<(), Error> {
        if self.eat_kw(kw) {
            Ok(())
        } else {
            Err(self.err(format!("expected `{kw}`")))
        }
    }

    fn eat_char(&mut self, c: char) -> bool {
        self.skip();
        if self.peek() == Some(c) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn expect_char(&mut self, c: char) -> Result<(), Error> {
        if self.eat_char(c) {
            Ok(())
        } else {
            Err(self.err(format!("expected `{c}`")))
        }
    }

    fn eat_op(&mut self, op: &str) -> bool {
        self.skip();
        if self.rest().starts_with(op) {
            let after = self.pos + op.len();
            if op.chars().all(|c| c.is_ascii_punctuation()) {
                self.pos = after;
                return true;
            }
        }
        false
    }

    fn expect_string(&mut self) -> Result<String, Error> {
        self.skip();
        let bytes = self.src.as_bytes();
        if bytes.get(self.pos) != Some(&b'"') {
            return Err(self.err("expected string"));
        }
        let open_pos = self.pos;
        self.pos += 1;
        let start = self.pos;
        match memchr::memchr2(b'"', b'\\', &bytes[start..]) {
            Some(rel) if bytes[start + rel] == b'"' => {
                let end = start + rel;
                let len = end - start;
                if len > MAX_DECODED_STRING_BYTES {
                    return Err(self.err_at(
                        open_pos,
                        format!(
                            "decoded string literal length {len} exceeds maximum allowed {MAX_DECODED_STRING_BYTES} bytes"
                        ),
                    ));
                }
                self.pos = end + 1;
                Ok(self.src[start..end].to_owned())
            }
            Some(rel) => self.expect_string_escaped_tail(open_pos, start, start + rel),
            None => {
                let (open_line, open_col) = offset_to_line_col(self.src, open_pos);
                self.pos = bytes.len();
                Err(self.err(format!(
                    "unterminated string (opened at {open_line}:{open_col})"
                )))
            }
        }
    }

    /// One escape after a backslash. Accepts the escapes string values are printed with
    /// (Rust `{:?}` form: `\n \t \r \0 \\ \" \' \u{..}`), so printed text parses back to
    /// the same value; any other escape is kept as written, backslash included.
    fn string_escape(&mut self, open_pos: usize, out: &mut String) -> Result<(), Error> {
        match self.bump() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('\'') => out.push('\''),
            Some('u') => {
                if self.bump() != Some('{') {
                    return Err(self.err("expected `{` after \\u"));
                }
                let mut val = 0u32;
                let mut digits = 0usize;
                loop {
                    match self.bump() {
                        Some('}') => break,
                        Some(c) if digits < 6 => match c.to_digit(16) {
                            Some(d) => {
                                val = (val << 4) | d;
                                digits += 1;
                            }
                            None => return Err(self.err("bad \\u{..} escape")),
                        },
                        _ => return Err(self.err("bad \\u{..} escape")),
                    }
                }
                if digits == 0 {
                    return Err(self.err("bad \\u{..} escape"));
                }
                let ch = char::from_u32(val).ok_or_else(|| self.err("bad \\u{..} escape"))?;
                if out.len() + ch.len_utf8() > MAX_DECODED_STRING_BYTES {
                    return Err(self.err_at(
                        open_pos,
                        format!(
                            "decoded string literal exceeds maximum allowed {MAX_DECODED_STRING_BYTES} bytes"
                        ),
                    ));
                }
                out.push(ch);
                return Ok(());
            }
            // Any other escape keeps its backslash, so regex patterns such as `"\d+"`
            // mean what they say (this used to drop the backslash and match `d+`).
            Some(c) => {
                out.push('\\');
                out.push(c);
            }
            None => {
                let (open_line, open_col) = offset_to_line_col(self.src, open_pos);
                return Err(self.err(format!(
                    "unterminated string (opened at {open_line}:{open_col})"
                )));
            }
        }
        if out.len() > MAX_DECODED_STRING_BYTES {
            return Err(self.err_at(
                open_pos,
                format!(
                    "decoded string literal exceeds maximum allowed {MAX_DECODED_STRING_BYTES} bytes"
                ),
            ));
        }
        Ok(())
    }

    fn expect_string_arc(&mut self) -> Result<std::sync::Arc<str>, Error> {
        self.skip();
        let bytes = self.src.as_bytes();
        if bytes.get(self.pos) != Some(&b'"') {
            return Err(self.err("expected string"));
        }
        let open_pos = self.pos;
        self.pos += 1;
        let start = self.pos;
        match memchr::memchr2(b'"', b'\\', &bytes[start..]) {
            Some(rel) if bytes[start + rel] == b'"' => {
                let end = start + rel;
                let len = end - start;
                if len > MAX_DECODED_STRING_BYTES {
                    return Err(self.err_at(
                        open_pos,
                        format!(
                            "decoded string literal length {len} exceeds maximum allowed {MAX_DECODED_STRING_BYTES} bytes"
                        ),
                    ));
                }
                self.pos = end + 1;
                Ok(std::sync::Arc::from(&self.src[start..end]))
            }
            Some(rel) => Ok(std::sync::Arc::from(
                self.expect_string_escaped_tail(open_pos, start, start + rel)?,
            )),
            None => {
                let (open_line, open_col) = offset_to_line_col(self.src, open_pos);
                self.pos = bytes.len();
                Err(self.err(format!(
                    "unterminated string (opened at {open_line}:{open_col})"
                )))
            }
        }
    }

    fn expect_string_escaped_tail(
        &mut self,
        open_pos: usize,
        start: usize,
        mut esc_pos: usize,
    ) -> Result<String, Error> {
        let bytes = self.src.as_bytes();
        let prefix_len = esc_pos - start;
        if prefix_len > MAX_DECODED_STRING_BYTES {
            return Err(self.err_at(
                open_pos,
                format!(
                    "decoded string literal exceeds maximum allowed {MAX_DECODED_STRING_BYTES} bytes"
                ),
            ));
        }
        let mut out = String::with_capacity((prefix_len + 16).min(MAX_DECODED_STRING_BYTES));
        out.push_str(&self.src[start..esc_pos]);
        loop {
            self.pos = esc_pos + 1;
            self.string_escape(open_pos, &mut out)?;
            let seg_start = self.pos;
            match memchr::memchr2(b'"', b'\\', &bytes[seg_start..]) {
                Some(rel) => {
                    let hit = seg_start + rel;
                    let seg_len = hit - seg_start;
                    if out.len().saturating_add(seg_len) > MAX_DECODED_STRING_BYTES {
                        return Err(self.err_at(
                            open_pos,
                            format!(
                                "decoded string literal exceeds maximum allowed {MAX_DECODED_STRING_BYTES} bytes"
                            ),
                        ));
                    }
                    out.push_str(&self.src[seg_start..hit]);
                    if bytes[hit] == b'"' {
                        self.pos = hit + 1;
                        return Ok(out);
                    }
                    esc_pos = hit;
                }
                None => {
                    let (open_line, open_col) = offset_to_line_col(self.src, open_pos);
                    self.pos = bytes.len();
                    return Err(self.err(format!(
                        "unterminated string (opened at {open_line}:{open_col})"
                    )));
                }
            }
        }
    }

    fn parse_hyphen_ident(&mut self) -> Result<String, Error> {
        let mut s = self.expect_ident()?;
        while self.peek() == Some('-') {
            self.bump();
            s.push('-');
            s.push_str(&self.expect_ident()?);
        }
        Ok(s)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, Error> {
        self.enter_depth()?;
        let res = self.parse_stmt_inner();
        self.leave_depth();
        res
    }

    fn parse_stmt_inner(&mut self) -> Result<Stmt, Error> {
        self.skip();
        let save = self.pos;
        if let Some(kw) = self.eat_ident_str() {
            match kw {
                "let" => return self.parse_let(),
                "delete" => return self.parse_delete(),
                "append" => return self.parse_append(),
                "insert" => return self.parse_insert(),
                "update" => return self.parse_update(),
                "reembed" => return self.parse_reembed(),
                "index" => return Ok(Stmt::Decl(self.parse_index()?)),
                "owned" => return Ok(Stmt::Decl(self.parse_owned()?)),
                "col" => return Ok(Stmt::Decl(self.parse_col()?)),
                "rel" => return Ok(Stmt::Decl(self.parse_rel()?)),
                "fk" => return Ok(Stmt::Decl(self.parse_fk()?)),
                "guard" => return Ok(Stmt::Decl(self.parse_guard()?)),
                "filter" => return Ok(Stmt::Decl(self.parse_filter()?)),
                "unfilter" => {
                    let collection = self.expect_ident()?;
                    return Ok(Stmt::Decl(Decl::Filter {
                        collection,
                        pred: None,
                        src: String::new(),
                    }));
                }
                "idb" => return self.parse_idb(),
                "pull" => {
                    self.expect_kw("idb")?;
                    self.expect_kw("since")?;
                    let since = self.expect_int()?;
                    let take = if self.eat_kw("take") {
                        Some(self.expect_int()?)
                    } else {
                        None
                    };
                    return Ok(Stmt::IdbPull { since, take });
                }
                "push" => {
                    self.expect_kw("idb")?;
                    return Ok(Stmt::IdbPush);
                }
                "pin" => {
                    let name = self.expect_string()?;
                    return Ok(Stmt::Pin { name });
                }
                "unpin" => {
                    let name = self.expect_string()?;
                    return Ok(Stmt::Unpin { name });
                }
                "snapshot" => {
                    let name = self.expect_string()?;
                    return Ok(Stmt::Snapshot { name });
                }
                "restore" => {
                    let name = self.expect_string()?;
                    return Ok(Stmt::Restore { name });
                }
                _ => self.pos = save,
            }
        }
        Ok(Stmt::Query(self.parse_query()?))
    }

    fn parse_query(&mut self) -> Result<Query, Error> {
        self.enter_depth()?;
        let res = self.parse_query_inner();
        self.leave_depth();
        res
    }

    fn parse_query_inner(&mut self) -> Result<Query, Error> {
        let source = self.parse_source()?;
        let ignore_filter = self.eat_kw("all");
        let mut steps = Vec::new();
        let mut explain = None;
        while self.peek_after_skip() == Some('|') {
            self.expect_char('|')?;
            self.skip();
            if self.looks_like_mutation() {
                return Err(self.err("mutation on pipe"));
            }
            if self.eat_kw("explain") {
                explain = Some(self.parse_explain_kind());
                self.skip();
                if self.peek_after_skip() == Some('|') {
                    return Err(self.err("`explain` must be the last step"));
                }
                break;
            }
            if self.eat_kw("union") {
                let (rhs, paren) = self.parse_union_rhs()?;
                steps.push(Step::Union(rhs));
                if !paren {
                    break;
                }
                continue;
            }
            steps.push(self.parse_step()?);
        }
        Ok(Query {
            source,
            steps,
            explain,
            ignore_filter,
        })
    }

    fn looks_like_mutation(&self) -> bool {
        matches!(
            self.peek_ident(),
            Some(
                "update"
                    | "insert"
                    | "append"
                    | "reembed"
                    | "delete"
                    | "let"
                    | "index"
                    | "filter"
                    | "unfilter"
                    | "owned",
            )
        )
    }

    fn parse_union_rhs(&mut self) -> Result<(Query, bool), Error> {
        self.skip();
        if self.eat_char('(') {
            let q = self.parse_query()?;
            self.expect_char(')')?;
            Ok((q, true))
        } else {
            Ok((self.parse_query()?, false))
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, Error> {
        let name = self.expect_ident()?;
        self.expect_char('=')?;
        let query = self.parse_query()?;
        Ok(Stmt::Let { name, query })
    }

    fn parse_delete(&mut self) -> Result<Stmt, Error> {
        if self.eat_kw("edge") {
            let rel = self.expect_ident()?;
            let from = self.parse_value()?;
            if !self.eat_op("->") {
                return Err(self.err("expected `->`"));
            }
            let to = self.parse_value()?;
            return Ok(Stmt::DeleteEdge { rel, from, to });
        }
        let collection = self.expect_ident()?;
        self.expect_char('[')?;
        let pred = self.parse_pred()?;
        self.expect_char(']')?;
        let (cas, cas_each) = self.parse_cas()?;
        Ok(Stmt::Delete {
            collection,
            pred,
            cas,
            cas_each,
        })
    }

    fn parse_source(&mut self) -> Result<Source, Error> {
        if self.eat_kw("page") {
            return Ok(Source::Page(self.expect_string()?));
        }
        if self.eat_kw("catalog") {
            return Ok(Source::Catalog);
        }
        Ok(Source::Collection(self.expect_ident()?))
    }

    fn parse_explain_kind(&mut self) -> ExplainKind {
        if self.eat_kw("cost") {
            ExplainKind::Cost
        } else if self.eat_kw("run") {
            ExplainKind::Run
        } else if self.eat_kw("backend") {
            ExplainKind::Backend
        } else if self.eat_kw("graph") || self.eat_kw("mermaid") {
            ExplainKind::Graph
        } else if self.eat_kw("dot") {
            ExplainKind::Dot
        } else {
            ExplainKind::Tree
        }
    }

    fn parse_step(&mut self) -> Result<Step, Error> {
        self.skip();
        if self.peek() == Some('{') {
            return Ok(Step::Project(self.parse_project_brace()?));
        }
        let save = self.pos;
        if let Some(kw) = self.eat_ident_str() {
            match kw {
                "where" => return Ok(Step::Filter(self.parse_pred()?)),
                "pick" => {
                    self.skip();
                    if self.peek() == Some('{') {
                        return Ok(Step::Project(self.parse_project_brace()?));
                    }
                    return Ok(Step::Project(self.parse_field_list()?));
                }
                "without" => {
                    self.skip();
                    if self.peek() == Some('{') {
                        let fields = self.parse_delimited(
                            '{',
                            '}',
                            "expected `,` or `}` in without",
                            Some("without requires at least one field"),
                            Self::parse_field,
                        )?;
                        return Ok(Step::Without(fields));
                    }
                    return Ok(Step::Without(self.parse_field_list()?));
                }
                "join" => {
                    let left = self.eat_kw("left");
                    let collection = self.expect_ident()?;
                    self.expect_kw("on")?;
                    let on = self.expect_ident()?;
                    return Ok(Step::Join {
                        left,
                        collection,
                        on,
                    });
                }
                "hop" => {
                    let (rel, depth) = self.parse_rel_depth()?;
                    return Ok(Step::Hop { rel, depth });
                }
                "graph" => {
                    let (rel, depth) = self.parse_rel_depth()?;
                    return Ok(Step::Graph { rel, depth });
                }
                "match" => return self.parse_match_step(),
                "search" => {
                    let mode = if self.eat_kw("lex") {
                        SearchMode::Lex
                    } else if self.eat_kw("vec") {
                        SearchMode::Vec
                    } else {
                        SearchMode::Hybrid
                    };
                    let query = self.expect_string()?;
                    return Ok(Step::Search { mode, query });
                }
                "count" => {
                    if self.eat_char('(') {
                        self.expect_char(')')?;
                    }
                    let by = self.parse_opt_by_keys()?;
                    return match try_into_simple_by_field(by) {
                        Ok(simple) => Ok(Step::Count { by: simple }),
                        Err(by) => Ok(Step::Summarize {
                            specs: vec![AggSpec {
                                alias: "hits".into(),
                                func: AggFunc::Count,
                            }],
                            by,
                        }),
                    };
                }
                "sum" => {
                    return self
                        .parse_single_agg_step("total", AggFunc::Sum, |field, by| Step::Sum {
                            field,
                            by,
                        });
                }
                "min" => {
                    return self.parse_single_agg_step("min", AggFunc::Min, |field, by| {
                        Step::Min { field, by }
                    });
                }
                "max" => {
                    return self.parse_single_agg_step("max", AggFunc::Max, |field, by| {
                        Step::Max { field, by }
                    });
                }
                "avg" => {
                    return self.parse_single_agg_step("avg", AggFunc::Avg, |field, by| {
                        Step::Avg { field, by }
                    });
                }
                "dcount" => {
                    let field = self.parse_window_field_arg()?;
                    let by = self.parse_opt_by_keys()?;
                    return Ok(Step::Summarize {
                        specs: vec![AggSpec {
                            alias: "dcount".into(),
                            func: AggFunc::DCount(field),
                        }],
                        by,
                    });
                }
                "percentile" => {
                    let (field, pct) = self.parse_percentile_args()?;
                    let by = self.parse_opt_by_keys()?;
                    return Ok(Step::Summarize {
                        specs: vec![AggSpec {
                            alias: default_pct_alias(pct),
                            func: AggFunc::Percentile { field, pct },
                        }],
                        by,
                    });
                }
                "arg_max" => return self.parse_arg_extremum_step(true),
                "arg_min" => return self.parse_arg_extremum_step(false),
                "summarize" => return self.parse_summarize_step(),
                "sort" => {
                    let field = self.parse_field()?;
                    let desc = self.eat_kw("desc");
                    if !desc {
                        self.eat_kw("asc");
                    }
                    return Ok(Step::Sort { field, desc });
                }
                "window" => return self.parse_window_step(),
                "skip" | "offset" => {
                    let n = self.expect_int()?;
                    return Ok(Step::Skip { n });
                }
                "take" => {
                    if self.eat_kw("all") {
                        return Ok(Step::Take { n: None });
                    }
                    return Ok(Step::Take {
                        n: Some(self.expect_int()?),
                    });
                }
                "explain" | "union" => {
                    self.pos = save;
                }
                _ => {
                    self.pos = save;
                    return Ok(Step::Filter(self.parse_pred()?));
                }
            }
        } else if matches!(self.peek(), Some('(' | '!')) {
            return Ok(Step::Filter(self.parse_pred()?));
        }
        let tok = token_preview(self.rest());
        Err(self.err(format!(
            "expected a named step or a predicate after `|`, unexpected token {tok}"
        )))
    }

    fn parse_frame_bound(&mut self) -> Result<crate::window::FrameBound, Error> {
        if self.eat_kw("unbounded") {
            if self.eat_kw("preceding") {
                Ok(crate::window::FrameBound::UnboundedPreceding)
            } else if self.eat_kw("following") {
                Ok(crate::window::FrameBound::UnboundedFollowing)
            } else {
                Err(self.err("expected `preceding` or `following` after `unbounded`"))
            }
        } else if self.eat_kw("current") {
            self.expect_kw("row")?;
            Ok(crate::window::FrameBound::CurrentRow)
        } else {
            let n = self.expect_int()?;
            if n < 0 {
                return Err(self.err("frame offset must be non-negative"));
            }
            if self.eat_kw("preceding") {
                Ok(crate::window::FrameBound::Preceding(n as usize))
            } else if self.eat_kw("following") {
                Ok(crate::window::FrameBound::Following(n as usize))
            } else {
                Err(self.err("expected `preceding` or `following` after frame offset number"))
            }
        }
    }

    fn parse_rel_depth(&mut self) -> Result<(String, Option<i64>), Error> {
        let rel = self.expect_ident()?;
        let depth = if self.eat_kw("depth") {
            self.expect_char('=')?;
            Some(self.expect_int()?)
        } else {
            None
        };
        Ok((rel, depth))
    }

    fn parse_single_agg_step(
        &mut self,
        default_alias: &str,
        make_func: impl FnOnce(Field) -> AggFunc,
        make_legacy: impl FnOnce(Field, Option<Field>) -> Step,
    ) -> Result<Step, Error> {
        let field = self.parse_window_field_arg()?;
        let by = self.parse_opt_by_keys()?;
        match try_into_simple_by_field(by) {
            Ok(simple) => Ok(make_legacy(field, simple)),
            Err(by) => Ok(Step::Summarize {
                specs: vec![AggSpec {
                    alias: default_alias.to_string(),
                    func: make_func(field),
                }],
                by,
            }),
        }
    }

    fn parse_opt_by_keys(&mut self) -> Result<Vec<GroupKey>, Error> {
        if !self.eat_kw("by") {
            return Ok(Vec::new());
        }
        self.skip();
        if self.peek() == Some('[') {
            return self.parse_delimited(
                '[',
                ']',
                "expected `,` or `]` in `by` list",
                Some("`by [...]` requires at least one key"),
                Self::parse_group_key,
            );
        }
        let mut keys = vec![self.parse_group_key()?];
        while self.eat_char(',') {
            keys.push(self.parse_group_key()?);
        }
        Ok(keys)
    }

    fn parse_group_key(&mut self) -> Result<GroupKey, Error> {
        let save_alias = self.pos;
        let explicit_alias = if let Some(id) = self.eat_ident_str() {
            if self.eat_char(':') {
                Some(id.to_owned())
            } else {
                self.pos = save_alias;
                None
            }
        } else {
            self.pos = save_alias;
            None
        };

        let save_bin = self.pos;
        if self.eat_kw("bin") && self.eat_char('(') {
            let field = self.parse_field()?;
            self.expect_char(',')?;
            let step = self.parse_bin_step()?;
            self.expect_char(')')?;
            let alias = explicit_alias.unwrap_or_else(|| field.as_str());
            return Ok(GroupKey::bin(alias, field, step));
        }
        self.pos = save_bin;
        let field = self.parse_field()?;
        let alias = explicit_alias.unwrap_or_else(|| field.as_str());
        Ok(GroupKey {
            alias,
            expr: GroupExpr::Field(field),
        })
    }

    fn parse_bin_step(&mut self) -> Result<BinStep, Error> {
        self.skip();
        match self.try_number()? {
            Some(Value::Duration(d)) => Ok(BinStep::Duration(d.as_millis())),
            Some(Value::Int(n)) => Ok(BinStep::Int(n)),
            Some(Value::Float(f)) => Ok(BinStep::Float(f)),
            _ => Err(self.err("expected positive duration (e.g. 1h, 5m) or number for bin step")),
        }
    }

    fn expect_f64(&mut self) -> Result<f64, Error> {
        self.skip();
        match self.try_number()? {
            Some(Value::Int(n)) => Ok(n as f64),
            Some(Value::Float(f)) => Ok(f),
            Some(Value::Duration(_)) => Err(self.err("expected number, got duration")),
            _ => Err(self.err("expected number")),
        }
    }

    fn parse_percentile_args(&mut self) -> Result<(Field, f64), Error> {
        if self.eat_char('(') {
            let field = self.parse_field()?;
            self.expect_char(',')?;
            let pct = self.expect_f64()?;
            self.expect_char(')')?;
            Ok((field, pct))
        } else {
            let field = self.parse_field()?;
            let _ = self.eat_char(',');
            let pct = self.expect_f64()?;
            Ok((field, pct))
        }
    }

    fn parse_arg_extremum_step(&mut self, is_max: bool) -> Result<Step, Error> {
        if self.eat_char('(') {
            let order = self.parse_field()?;
            self.expect_char(',')?;
            if self.eat_char('*') {
                self.expect_char(')')?;
                let by = self.parse_opt_by_keys()?;
                return Ok(Step::ArgExtremum { is_max, order, by });
            }
            let value = self.parse_field()?;
            self.expect_char(')')?;
            let by = self.parse_opt_by_keys()?;
            let alias = value.as_str();
            let func = if is_max {
                AggFunc::ArgMax { order, value }
            } else {
                AggFunc::ArgMin { order, value }
            };
            return Ok(Step::Summarize {
                specs: vec![AggSpec { alias, func }],
                by,
            });
        }
        let order = self.parse_field()?;
        let by = self.parse_opt_by_keys()?;
        Ok(Step::ArgExtremum { is_max, order, by })
    }

    fn parse_summarize_step(&mut self) -> Result<Step, Error> {
        self.skip();
        let specs = if self.peek() == Some('{') {
            self.parse_delimited(
                '{',
                '}',
                "expected `,` or `}` in summarize",
                Some("summarize requires at least one metric"),
                Self::parse_agg_spec,
            )?
        } else {
            let mut list = vec![self.parse_agg_spec()?];
            while self.eat_char(',') {
                list.push(self.parse_agg_spec()?);
            }
            list
        };
        let by = self.parse_opt_by_keys()?;
        Ok(Step::Summarize { specs, by })
    }

    fn parse_agg_spec(&mut self) -> Result<AggSpec, Error> {
        let save_alias = self.pos;
        let explicit_alias = if let Some(id) = self.eat_ident_str() {
            if self.eat_char(':') {
                Some(id.to_owned())
            } else {
                self.pos = save_alias;
                None
            }
        } else {
            self.pos = save_alias;
            None
        };

        let func_name = self.expect_ident_str()?;
        let (default_alias, func): (&str, AggFunc) = match func_name {
            "count" => {
                if self.eat_char('(') && !self.eat_char(')') {
                    return Err(self.err("`count()` in summarize takes no arguments (use `dcount field` for distinct count)"));
                }
                ("hits", AggFunc::Count)
            }
            "sum" => ("total", AggFunc::Sum(self.parse_window_field_arg()?)),
            "avg" => ("avg", AggFunc::Avg(self.parse_window_field_arg()?)),
            "min" => ("min", AggFunc::Min(self.parse_window_field_arg()?)),
            "max" => ("max", AggFunc::Max(self.parse_window_field_arg()?)),
            "dcount" => ("dcount", AggFunc::DCount(self.parse_window_field_arg()?)),
            "percentile" => {
                let (f, pct) = self.parse_percentile_args()?;
                let alias = explicit_alias.unwrap_or_else(|| default_pct_alias(pct));
                return Ok(AggSpec {
                    alias,
                    func: AggFunc::Percentile { field: f, pct },
                });
            }
            "arg_max" | "arg_min" => {
                let is_max = func_name == "arg_max";
                let (order, value) = if self.eat_char('(') {
                    let ord = self.parse_field()?;
                    self.expect_char(',')?;
                    if self.eat_char('*') {
                        return Err(self.err(
                            "use top-level `arg_max field [by ...]` for whole-row `*` selection, or specify a column: `arg_max(order, field)`",
                        ));
                    }
                    let val = self.parse_field()?;
                    self.expect_char(')')?;
                    (ord, val)
                } else {
                    let ord = self.parse_field()?;
                    let _ = self.eat_char(',');
                    let val = self.parse_field()?;
                    (ord, val)
                };
                let alias = explicit_alias.unwrap_or_else(|| value.as_str());
                let func = if is_max {
                    AggFunc::ArgMax { order, value }
                } else {
                    AggFunc::ArgMin { order, value }
                };
                return Ok(AggSpec { alias, func });
            }
            _ => {
                return Err(self.err(format!(
                    "unknown summarize metric `{func_name}`; supported: count, sum, avg, min, max, dcount, percentile, arg_max, arg_min"
                )));
            }
        };

        Ok(AggSpec {
            alias: explicit_alias.unwrap_or_else(|| default_alias.to_owned()),
            func,
        })
    }

    fn parse_window_field_arg(&mut self) -> Result<Field, Error> {
        if self.eat_char('(') {
            let field = self.parse_field()?;
            self.expect_char(')')?;
            Ok(field)
        } else {
            self.parse_field()
        }
    }

    fn parse_window_offset_field(&mut self) -> Result<(usize, Field), Error> {
        let offset = if self.eat_char('(') {
            let off = self.expect_int()? as usize;
            self.expect_char(')')?;
            off
        } else if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.expect_int()? as usize
        } else {
            1
        };
        let f = self.parse_field()?;
        Ok((offset, f))
    }

    fn parse_window_step(&mut self) -> Result<Step, Error> {
        let func_name = self.expect_ident_str()?;
        let (func, field) = if func_name.eq_ignore_ascii_case("row_number") {
            (crate::window::WindowFunction::RowNumber, None)
        } else if func_name.eq_ignore_ascii_case("rank") {
            (crate::window::WindowFunction::Rank, None)
        } else if func_name.eq_ignore_ascii_case("dense_rank") {
            (crate::window::WindowFunction::DenseRank, None)
        } else if func_name.eq_ignore_ascii_case("lead") {
            let (offset, f) = self.parse_window_offset_field()?;
            (crate::window::WindowFunction::Lead { offset }, Some(f))
        } else if func_name.eq_ignore_ascii_case("lag") {
            let (offset, f) = self.parse_window_offset_field()?;
            (crate::window::WindowFunction::Lag { offset }, Some(f))
        } else if func_name.eq_ignore_ascii_case("first_value")
            || func_name.eq_ignore_ascii_case("first")
        {
            (
                crate::window::WindowFunction::FirstValue,
                Some(self.parse_window_field_arg()?),
            )
        } else if func_name.eq_ignore_ascii_case("last_value")
            || func_name.eq_ignore_ascii_case("last")
        {
            (
                crate::window::WindowFunction::LastValue,
                Some(self.parse_window_field_arg()?),
            )
        } else if func_name.eq_ignore_ascii_case("sum") {
            (
                crate::window::WindowFunction::Sum,
                Some(self.parse_window_field_arg()?),
            )
        } else if func_name.eq_ignore_ascii_case("avg") {
            (
                crate::window::WindowFunction::Avg,
                Some(self.parse_window_field_arg()?),
            )
        } else if func_name.eq_ignore_ascii_case("min") {
            (
                crate::window::WindowFunction::Min,
                Some(self.parse_window_field_arg()?),
            )
        } else if func_name.eq_ignore_ascii_case("max") {
            (
                crate::window::WindowFunction::Max,
                Some(self.parse_window_field_arg()?),
            )
        } else if func_name.eq_ignore_ascii_case("count") {
            let f = if self.eat_char('(') {
                let inner = if self.eat_char('*') {
                    None
                } else {
                    Some(self.parse_field()?)
                };
                self.expect_char(')')?;
                inner
            } else if let Some(ident) = self.peek_ident() {
                if ["by", "partition", "sort", "order", "rows", "as"]
                    .iter()
                    .any(|kw| ident.eq_ignore_ascii_case(kw))
                {
                    None
                } else {
                    Some(self.parse_field()?)
                }
            } else {
                None
            };
            (crate::window::WindowFunction::Count, f)
        } else {
            return Err(self.err(format!(
                "unknown window function `{}`; supported: row_number, rank, dense_rank, lead, lag, first_value, last_value, sum, avg, min, max, count",
                func_name.to_ascii_lowercase()
            )));
        };

        let by = if self.eat_kw("by") || self.eat_kw("partition") {
            let _ = self.eat_kw("by");
            let mut fields = Vec::new();
            if self.eat_char('[') {
                while !self.eat_char(']') {
                    fields.push(self.parse_field()?);
                    if !self.eat_char(',') {
                        self.expect_char(']')?;
                        break;
                    }
                }
            } else {
                fields.push(self.parse_field()?);
                while self.eat_char(',') {
                    fields.push(self.parse_field()?);
                }
            }
            fields
        } else {
            Vec::new()
        };

        let order = if self.eat_kw("sort") || self.eat_kw("order") {
            let _ = self.eat_kw("by");
            let mut sorts = Vec::new();
            if self.eat_char('[') {
                while !self.eat_char(']') {
                    let field = self.parse_field()?;
                    let desc = self.eat_kw("desc");
                    if !desc {
                        self.eat_kw("asc");
                    }
                    sorts.push(crate::ast::WindowSort { field, desc });
                    if !self.eat_char(',') {
                        self.expect_char(']')?;
                        break;
                    }
                }
            } else {
                loop {
                    let field = self.parse_field()?;
                    let desc = self.eat_kw("desc");
                    if !desc {
                        self.eat_kw("asc");
                    }
                    sorts.push(crate::ast::WindowSort { field, desc });
                    if !self.eat_char(',') {
                        break;
                    }
                }
            }
            sorts
        } else {
            Vec::new()
        };

        let frame = if self.eat_kw("rows") {
            let _ = self.eat_kw("between");
            let start = self.parse_frame_bound()?;
            if self.eat_kw("and") {
                let end = self.parse_frame_bound()?;
                Some(crate::window::WindowFrame { start, end })
            } else {
                Some(crate::window::WindowFrame {
                    start,
                    end: crate::window::FrameBound::CurrentRow,
                })
            }
        } else {
            None
        };

        self.expect_kw("as")?;
        let as_field = self.expect_ident()?;

        Ok(Step::Window {
            func,
            field,
            by,
            order,
            frame,
            as_field,
        })
    }

    /// `match` path: `-rel-> b`, `<-rel- b`, `-rel*1..3-> b`, `-[e:rel]-> b`.
    fn parse_match_step(&mut self) -> Result<Step, Error> {
        self.skip();
        let start = match self.peek() {
            Some('-') | Some('<') => None,
            Some(c) if is_ident_start(c) => {
                let name = self.expect_ident()?;
                self.skip();
                match self.peek() {
                    Some('-') | Some('<') => Some(name),
                    _ => {
                        return Err(self.err("expected `-rel->` or `<-rel-` after match start"));
                    }
                }
            }
            _ => {
                return Err(self.err("expected match path `-rel-> bind`"));
            }
        };
        let mut hops = Vec::new();
        loop {
            self.skip();
            match self.peek() {
                Some('-') | Some('<') => hops.push(self.parse_match_hop()?),
                _ => break,
            }
        }
        if hops.is_empty() {
            return Err(self.err("match requires at least one hop"));
        }
        Ok(Step::Match { start, hops })
    }

    fn parse_match_hop(&mut self) -> Result<MatchHop, Error> {
        self.skip();
        let reverse = if self.eat_op("<-") {
            true
        } else if self.eat_char('-') {
            false
        } else {
            return Err(self.err("expected `-` or `<-` in match hop"));
        };

        let (edge, rel, min_depth, max_depth) = if self.eat_char('[') {
            let edge = {
                let id = self.expect_ident()?;
                self.expect_char(':')?;
                Some(id)
            };
            let rel = self.expect_ident()?;
            let (min_depth, max_depth) = self.parse_match_star()?;
            self.expect_char(']')?;
            (edge, rel, min_depth, max_depth)
        } else {
            let rel = self.expect_ident()?;
            let (min_depth, max_depth) = self.parse_match_star()?;
            (None, rel, min_depth, max_depth)
        };

        if reverse {
            // `<-rel-` / `<-[e:rel]-` — trailing `-` (already consumed `[...]` / rel)
            if !self.eat_char('-') {
                return Err(self.err("expected trailing `-` after reverse match hop"));
            }
        } else if !self.eat_op("->") {
            return Err(self.err("expected `->` in match hop"));
        }

        let bind = self.expect_ident()?;
        Ok(MatchHop {
            rel,
            bind,
            edge,
            reverse,
            min_depth,
            max_depth,
        })
    }

    /// Optional `*N` or `*N..M` after rel.
    fn parse_match_star(&mut self) -> Result<(i64, i64), Error> {
        self.skip();
        if !self.eat_char('*') {
            return Ok((1, 1));
        }
        let min = self.expect_int()?;
        self.skip();
        if self.eat_op("..") {
            let max = self.expect_int()?;
            Ok((min, max))
        } else {
            Ok((min, min))
        }
    }

    fn parse_project_brace(&mut self) -> Result<Vec<Field>, Error> {
        self.expect_char('{')?;
        let fields = self.parse_field_list()?;
        self.expect_char('}')?;
        if fields.is_empty() {
            return Err(self.err("pick requires at least one field"));
        }
        Ok(fields)
    }

    fn parse_field_list(&mut self) -> Result<Vec<Field>, Error> {
        let mut fields = Vec::new();
        loop {
            self.skip();
            if !self.starts_ident() {
                break;
            }
            fields.push(self.parse_field()?);
            if !self.eat_char(',') {
                break;
            }
        }
        if fields.is_empty() {
            return Err(self.err("expected field list"));
        }
        Ok(fields)
    }

    fn parse_field(&mut self) -> Result<Field, Error> {
        let mut parts = vec![self.expect_ident()?];
        loop {
            self.skip();
            if self.rest().starts_with(".?") {
                self.pos += 2;
                parts.push(self.expect_ident()?);
                continue;
            }
            if self.peek() == Some('.') {
                self.bump();
                parts.push(self.expect_ident()?);
                continue;
            }
            break;
        }
        Ok(Field { parts })
    }

    fn parse_pred(&mut self) -> Result<Pred, Error> {
        let first = self.parse_and()?;
        if !self.eat_kw("or") {
            return Ok(first);
        }
        let second = self.parse_and()?;
        if !self.eat_kw("or") {
            return Ok(Pred::Or(Box::new(first), Box::new(second)));
        }
        let mut terms = Vec::with_capacity(8);
        terms.push(first);
        terms.push(second);
        terms.push(self.parse_and()?);
        while self.eat_kw("or") {
            terms.push(self.parse_and()?);
        }
        Ok(join_chain(terms, Pred::Or))
    }

    fn parse_and(&mut self) -> Result<Pred, Error> {
        let first = self.parse_unary_pred()?;
        if !self.eat_kw("and") {
            return Ok(first);
        }
        let second = self.parse_unary_pred()?;
        if !self.eat_kw("and") {
            return Ok(Pred::And(Box::new(first), Box::new(second)));
        }
        let mut terms = Vec::with_capacity(8);
        terms.push(first);
        terms.push(second);
        terms.push(self.parse_unary_pred()?);
        while self.eat_kw("and") {
            terms.push(self.parse_unary_pred()?);
        }
        Ok(join_chain(terms, Pred::And))
    }

    fn parse_unary_pred(&mut self) -> Result<Pred, Error> {
        self.enter_depth()?;
        let res = (|| {
            if self.eat_kw("not") {
                let p = self.parse_unary_pred()?;
                return Ok(Pred::Not(Box::new(p)));
            }
            if self.eat_char('(') {
                let p = self.parse_pred()?;
                self.expect_char(')')?;
                return Ok(p);
            }
            self.parse_cmp()
        })();
        self.leave_depth();
        res
    }

    fn parse_cmp(&mut self) -> Result<Pred, Error> {
        let field = self.parse_field()?;
        self.skip();
        let bytes = self.src.as_bytes();
        match bytes.get(self.pos).copied() {
            Some(b'=') if bytes.get(self.pos + 1) == Some(&b'=') => {
                self.pos += 2;
                let value = self.parse_value()?;
                return Ok(Pred::Cmp {
                    field,
                    op: CmpOp::Eq,
                    value,
                });
            }
            Some(b'!') if bytes.get(self.pos + 1) == Some(&b'=') => {
                self.pos += 2;
                let value = self.parse_value()?;
                return Ok(Pred::Cmp {
                    field,
                    op: CmpOp::Ne,
                    value,
                });
            }
            Some(b'>') => {
                let op = if bytes.get(self.pos + 1) == Some(&b'=') {
                    self.pos += 2;
                    CmpOp::Ge
                } else {
                    self.pos += 1;
                    CmpOp::Gt
                };
                let value = self.parse_value()?;
                return Ok(Pred::Cmp { field, op, value });
            }
            Some(b'<') => {
                let op = if bytes.get(self.pos + 1) == Some(&b'=') {
                    self.pos += 2;
                    CmpOp::Le
                } else {
                    self.pos += 1;
                    CmpOp::Lt
                };
                let value = self.parse_value()?;
                return Ok(Pred::Cmp { field, op, value });
            }
            Some(b'~') => {
                self.pos += 1;
                self.skip();
                if self.peek() == Some('/') {
                    let (pattern, flags) = self.parse_regex_lit()?;
                    return Ok(Pred::Regex {
                        field,
                        pattern,
                        flags,
                    });
                }
                let needle = self.expect_string()?;
                return Ok(Pred::Contains { field, needle });
            }
            Some(b) if is_ident_start_byte(b) => {
                let save = self.pos;
                if let Some(kw) = self.eat_ident_str() {
                    match kw {
                        "is" => {
                            let negated = self.eat_kw("not");
                            self.expect_kw("null")?;
                            return Ok(Pred::IsNull { field, negated });
                        }
                        "in" => {
                            let values = self.parse_delimited(
                                '[',
                                ']',
                                "expected `,` or `]` in `in` list",
                                Some("empty `in` list"),
                                Self::parse_value,
                            )?;
                            return Ok(Pred::In { field, values });
                        }
                        "has_ci" => {
                            let needle = self.expect_string()?;
                            return Ok(Pred::Has {
                                field,
                                ci: true,
                                needle,
                            });
                        }
                        "has" => {
                            let ci = self.eat_kw("ci");
                            let needle = self.expect_string()?;
                            return Ok(Pred::Has { field, ci, needle });
                        }
                        "regex" => {
                            self.skip();
                            if self.peek() == Some('/') {
                                let (pattern, flags) = self.parse_regex_lit()?;
                                return Ok(Pred::Regex {
                                    field,
                                    pattern,
                                    flags,
                                });
                            }
                            let pattern = self.expect_string()?;
                            crate::check::validate_regex(&pattern, "")?;
                            return Ok(Pred::Regex {
                                field,
                                pattern,
                                flags: String::new(),
                            });
                        }
                        _ => self.pos = save,
                    }
                }
            }
            _ => {}
        }
        let tok = token_preview(self.rest());
        Err(self.err(format!(
            "expected comparison (`==`, `has`, `~`, …), unexpected token {tok}"
        )))
    }

    fn parse_regex_lit(&mut self) -> Result<(String, String), Error> {
        self.skip();
        if self.peek() != Some('/') {
            return Err(self.err("expected regex literal"));
        }
        self.bump();
        let mut pattern = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => return Err(self.err("unterminated regex literal")),
                Some('/') => {
                    self.bump();
                    break;
                }
                Some('\\') => {
                    self.bump();
                    match self.peek() {
                        None => return Err(self.err("unterminated regex literal")),
                        Some(c) => {
                            pattern.push('\\');
                            pattern.push(c);
                            self.bump();
                        }
                    }
                }
                Some(c) => {
                    pattern.push(c);
                    self.bump();
                }
            }
        }
        let mut flags = String::new();
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            flags.push(self.bump().unwrap());
        }
        for f in flags.chars() {
            if f != 'i' {
                return Err(self.err(format!("unknown regex flag `{f}`")));
            }
        }
        crate::check::validate_regex(&pattern, &flags)?;
        Ok((pattern, flags))
    }

    fn parse_value(&mut self) -> Result<Value, Error> {
        self.skip();
        let Some(&b) = self.src.as_bytes().get(self.pos) else {
            return Err(self.err("expected value"));
        };
        if b == b'"' {
            return Ok(Value::String(self.expect_string_arc()?));
        }
        if b.is_ascii_digit()
            || (b == b'-'
                && self
                    .src
                    .as_bytes()
                    .get(self.pos + 1)
                    .is_some_and(u8::is_ascii_digit))
        {
            if let Some(v) = self.try_number_at_pos()? {
                return Ok(v);
            }
        }
        if b == b'$' {
            self.pos += 1;
            if !self.starts_ident() {
                return Err(self.err("expected parameter name after `$`"));
            }
            let start = self.pos;
            let bytes = self.src.as_bytes();
            let mut end = start + 1;
            while end < bytes.len() && is_ident_continue_byte(bytes[end]) {
                end += 1;
            }
            self.pos = end;
            return Ok(Value::Param(std::sync::Arc::from(&self.src[start..end])));
        }
        if is_ident_start_byte(b) {
            let ident = self.expect_ident_str()?;
            match ident {
                "true" => return Ok(Value::Bool(true)),
                "false" => return Ok(Value::Bool(false)),
                "now" => {
                    if self.eat_char('-') {
                        let dur = self.expect_duration()?;
                        return Ok(Value::NowMinus(dur));
                    }
                    return Ok(Value::Now);
                }
                "ago" => return self.parse_ago(),
                "timestamp" => {
                    if self.eat_char('(') {
                        let millis = self.expect_int()?;
                        self.expect_char(')')?;
                        return Ok(Value::Timestamp(millis));
                    }
                    return Ok(Value::Name(ident.to_owned()));
                }
                _ => return Ok(Value::Name(ident.to_owned())),
            }
        }
        let tok = token_preview(self.rest());
        Err(self.err(format!("expected value, unexpected token {tok}")))
    }

    fn parse_ago(&mut self) -> Result<Value, Error> {
        self.skip();
        let paren = self.eat_char('(');
        self.skip();
        if self.peek() == Some('"') {
            return Err(self.err("ago requires a duration with unit, not a string"));
        }
        if let Some(dur) = self.try_duration()? {
            if paren {
                self.expect_char(')')?;
            }
            return Ok(Value::NowMinus(dur));
        }
        Err(self.err("ago requires a duration with unit (7d, 3h, 15m, 30s, 1w)"))
    }

    /// Number literal, optionally signed. A `-` only starts a number when a
    /// digit follows it, so `-rel->` match hops and `foo-bar` names still lex
    /// as before, and durations stay unsigned.
    #[inline]
    fn try_number(&mut self) -> Result<Option<Value>, Error> {
        self.skip();
        self.try_number_at_pos()
    }

    fn try_number_at_pos(&mut self) -> Result<Option<Value>, Error> {
        let bytes = self.src.as_bytes();
        let start = self.pos;
        let Some(&b0) = bytes.get(start) else {
            return Ok(None);
        };
        let neg = b0 == b'-' && bytes.get(start + 1).is_some_and(u8::is_ascii_digit);
        let digit_start = start + usize::from(neg);
        if !bytes.get(digit_start).is_some_and(u8::is_ascii_digit) {
            return Ok(None);
        }
        let mut i = digit_start + 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if !neg
            && let Some(&ub) = bytes.get(i)
            && let Some(unit) = DurUnit::from_char(ub as char)
            && !bytes.get(i + 1).is_some_and(|&nb| is_ident_continue_byte(nb))
        {
            self.pos = i;
            let n: i64 = self.src[digit_start..i]
                .parse()
                .map_err(|_| self.err("invalid duration"))?;
            self.pos = i + 1;
            return Ok(Some(Value::Duration(Duration { n, unit })));
        }
        if bytes.get(i) == Some(&b'.') && bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
            let mut j = i + 2;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            self.pos = j;
            let n: f64 = self.src[start..j]
                .parse()
                .map_err(|_| self.err("invalid float"))?;
            return Ok(Some(Value::Float(n)));
        }
        self.pos = i;
        let n: i64 = self.src[start..i]
            .parse()
            .map_err(|_| self.err("invalid integer"))?;
        Ok(Some(Value::Int(n)))
    }

    fn try_duration(&mut self) -> Result<Option<Duration>, Error> {
        self.skip();
        let bytes = self.src.as_bytes();
        let start = self.pos;
        if !bytes.get(start).is_some_and(u8::is_ascii_digit) {
            return Ok(None);
        }
        let mut i = start + 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        let Some(&ub) = bytes.get(i) else {
            return Ok(None);
        };
        let Some(unit) = DurUnit::from_char(ub as char) else {
            return Ok(None);
        };
        if bytes.get(i + 1).is_some_and(|&b| is_ident_continue_byte(b)) {
            return Ok(None);
        }
        self.pos = i;
        let n: i64 = self.src[start..i]
            .parse()
            .map_err(|_| self.err("invalid duration"))?;
        self.pos = i + 1;
        Ok(Some(Duration { n, unit }))
    }

    fn expect_duration(&mut self) -> Result<Duration, Error> {
        self.try_duration()?
            .ok_or_else(|| self.err("expected duration with unit (7d, 3h, 15m, 30s, 1w)"))
    }

    fn expect_int(&mut self) -> Result<i64, Error> {
        self.skip();
        match self.try_number_at_pos()? {
            Some(Value::Int(n)) => Ok(n),
            Some(Value::Duration(_)) => Err(self.err("expected integer, got duration")),
            _ => Err(self.err("expected integer")),
        }
    }

    fn parse_record(&mut self) -> Result<Record, Error> {
        self.parse_record_with_cap(4)
    }

    fn parse_record_with_cap(&mut self, expected_fields: usize) -> Result<Record, Error> {
        self.expect_char('{')?;
        let mut fields = Vec::with_capacity(expected_fields);
        let bytes = self.src.as_bytes();
        loop {
            self.skip();
            if bytes.get(self.pos) == Some(&b'}') {
                self.pos += 1;
                break;
            }
            let name = self.expect_ident()?;
            self.expect_char(':')?;
            let value = self.parse_value()?;
            fields.push((name, value));
            self.skip();
            let comma = if bytes.get(self.pos) == Some(&b',') {
                self.pos += 1;
                self.skip();
                true
            } else {
                false
            };
            if bytes.get(self.pos) == Some(&b'}') {
                self.pos += 1;
                break;
            }
            if !comma {
                return Err(self.err("expected `,` or `}` in record"));
            }
        }
        if fields.is_empty() {
            return Err(self.err("record is empty"));
        }
        Ok(Record { fields })
    }

    fn parse_record_list(&mut self) -> Result<Vec<Record>, Error> {
        self.skip();
        let bytes = self.src.as_bytes();
        if bytes.get(self.pos) == Some(&b'[') {
            self.pos += 1;
            self.skip();
            if bytes.get(self.pos) == Some(&b']') {
                self.pos += 1;
                return Err(self.err("empty list"));
            }
            let first_start = self.pos;
            let first = self.parse_record_with_cap(8)?;
            let mut expected_fields = first.fields.len().max(1);
            let first_bytes = (self.pos - first_start).max(16);
            self.skip();
            let comma = if bytes.get(self.pos) == Some(&b',') {
                self.pos += 1;
                self.skip();
                true
            } else {
                false
            };
            if bytes.get(self.pos) == Some(&b']') {
                self.pos += 1;
                return Ok(vec![first]);
            }
            if !comma {
                return Err(self.err("expected `,` or `]` in list"));
            }
            let est_more = ((bytes.len() - self.pos) / first_bytes + 1).min(16_384);
            let mut records = Vec::with_capacity(1 + est_more);
            records.push(first);
            loop {
                let rec = self.parse_record_with_cap(expected_fields)?;
                expected_fields = expected_fields.max(rec.fields.len());
                records.push(rec);
                self.skip();
                let comma = if bytes.get(self.pos) == Some(&b',') {
                    self.pos += 1;
                    self.skip();
                    true
                } else {
                    false
                };
                if bytes.get(self.pos) == Some(&b']') {
                    self.pos += 1;
                    break;
                }
                if !comma {
                    return Err(self.err("expected `,` or `]` in list"));
                }
            }
            return Ok(records);
        }
        Ok(vec![self.parse_record()?])
    }

    fn parse_cas(&mut self) -> Result<(Option<String>, bool), Error> {
        if !self.eat_kw("cas") {
            return Ok((None, false));
        }
        if self.eat_kw("each") {
            return Ok((None, true));
        }
        Ok((Some(self.expect_string()?), false))
    }

    fn parse_edge_lit(&mut self) -> Result<EdgeLit, Error> {
        let rel = self.expect_ident()?;
        let from = self.parse_value()?;
        if !self.eat_op("->") {
            return Err(self.err("expected `->`"));
        }
        let to = self.parse_value()?;
        Ok(EdgeLit { rel, from, to })
    }

    fn parse_edge_list(&mut self) -> Result<Vec<EdgeLit>, Error> {
        self.parse_delimited(
            '[',
            ']',
            "expected `,` or `]` in list",
            Some("empty list"),
            Self::parse_edge_lit,
        )
    }

    fn parse_append(&mut self) -> Result<Stmt, Error> {
        if self.eat_kw("facts") {
            return Ok(Stmt::AppendFacts {
                records: self.parse_record_list()?,
            });
        }
        if self.eat_kw("edges") {
            return Ok(Stmt::AppendEdges {
                edges: self.parse_edge_list()?,
            });
        }
        if self.eat_kw("edge") {
            return Ok(Stmt::AppendEdges {
                edges: vec![self.parse_edge_lit()?],
            });
        }
        Err(self.err("expected `facts`, `edge`, or `edges` after `append`"))
    }

    fn parse_insert(&mut self) -> Result<Stmt, Error> {
        let collection = self.expect_ident()?;
        let bulk = self.peek_after_skip() == Some('[');
        let records = self.parse_record_list()?;
        let mut edges = Vec::new();
        if self.eat_kw("with") {
            if bulk || records.len() != 1 {
                return Err(self.err("with not allowed on bulk insert"));
            }
            loop {
                if !self.eat_kw("edge") {
                    if edges.is_empty() {
                        return Err(self.err("expected `edge` after `with`"));
                    }
                    break;
                }
                edges.push(self.parse_insert_edge()?);
            }
        }
        Ok(Stmt::Insert {
            collection,
            records,
            edges,
        })
    }

    fn parse_insert_edge(&mut self) -> Result<InsertEdge, Error> {
        let rel = self.expect_ident()?;
        if !self.eat_op("->") {
            return Err(self.err("expected `->`"));
        }
        let target = if self.eat_kw("page") {
            EdgeTarget::Page(self.expect_string()?)
        } else {
            EdgeTarget::Value(self.parse_value()?)
        };
        Ok(InsertEdge { rel, target })
    }

    fn parse_update(&mut self) -> Result<Stmt, Error> {
        let collection = self.expect_ident()?;
        let pred = if self.eat_char('[') {
            let pred = self.parse_pred()?;
            self.expect_char(']')?;
            Some(pred)
        } else {
            None
        };
        let (cas, cas_each) = self.parse_cas()?;
        let record = self.parse_record()?;
        Ok(Stmt::Update {
            collection,
            pred,
            cas,
            cas_each,
            record,
        })
    }

    fn parse_index(&mut self) -> Result<Decl, Error> {
        let collection = self.expect_ident()?;
        let unique = self.eat_kw("unique");
        let fields = self.parse_delimited(
            '[',
            ']',
            "expected `,` or `]` in index",
            Some("empty index"),
            Self::expect_ident,
        )?;

        // Check for optional WHERE clause (partial index)
        let pred = if self.eat_kw("where") {
            Some(self.parse_pred()?)
        } else {
            None
        };

        Ok(Decl::Index {
            collection,
            unique,
            fields,
            pred,
        })
    }

    fn parse_reembed(&mut self) -> Result<Stmt, Error> {
        let collection = self.expect_ident()?;
        let to = if self.eat_kw("to") {
            let provider = self.parse_hyphen_ident()?;
            self.expect_char('/')?;
            let model = self.parse_hyphen_ident()?;
            self.expect_char('/')?;
            let dim = self.expect_int()?;
            Some(EmbedTarget {
                provider,
                model,
                dim,
            })
        } else {
            None
        };
        Ok(Stmt::Reembed { collection, to })
    }

    fn parse_owned(&mut self) -> Result<Decl, Error> {
        let name = self.expect_ident()?;
        self.expect_char('{')?;
        let mut fields = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some('}') {
                break;
            }
            let fname = self.expect_ident()?;
            self.expect_char(':')?;
            let ty = self.parse_type_expr()?;
            fields.push((fname, ty));
            self.eat_char(',');
        }
        self.expect_char('}')?;
        Ok(Decl::Owned { name, fields })
    }

    fn parse_col(&mut self) -> Result<Decl, Error> {
        let name = self.expect_ident()?;
        let append = self.eat_kw("append");
        self.expect_char('{')?;
        let mut fields = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some('}') {
                break;
            }
            let fname = self.expect_ident()?;
            let ty = if self.eat_char(':') {
                self.parse_type_expr()?
            } else {
                TypeExpr::Named(fname.clone())
            };
            fields.push((fname, ty));
            self.eat_char(',');
        }
        self.expect_char('}')?;
        Ok(Decl::Col {
            name,
            append,
            fields,
        })
    }

    fn parse_type_expr(&mut self) -> Result<TypeExpr, Error> {
        if self.eat_kw("vec") {
            self.expect_char('[')?;
            let dim = self.expect_int()?;
            self.expect_char(']')?;
            self.expect_char('@')?;
            let model = self.parse_hyphen_ident()?;
            return Ok(TypeExpr::Vec { dim, model });
        }
        let name = self.expect_ident()?;
        let _ = self.eat_kw("unique");
        Ok(TypeExpr::Named(name))
    }

    fn parse_rel(&mut self) -> Result<Decl, Error> {
        let name = self.expect_ident()?;
        if self.eat_char('=') {
            self.expect_kw("reverse")?;
            let of = self.expect_ident()?;
            return Ok(Decl::RelReverse { name, of });
        }
        let stub = self.eat_kw("stub");
        Ok(Decl::Rel { name, stub })
    }

    fn parse_fk(&mut self) -> Result<Decl, Error> {
        let from_col = self.expect_ident()?;
        self.expect_char('.')?;
        let from_field = self.expect_ident()?;
        if !self.eat_op("->") {
            return Err(self.err("expected `->`"));
        }
        let to_col = self.expect_ident()?;
        self.expect_char('.')?;
        let to_field = self.expect_ident()?;
        Ok(Decl::Fk {
            from_col,
            from_field,
            to_col,
            to_field,
        })
    }

    fn parse_filter(&mut self) -> Result<Decl, Error> {
        let collection = self.expect_ident()?;
        self.skip();
        let start = self.pos;
        let pred = self.parse_pred()?;
        let src = self.src[start..self.pos].trim().to_string();
        Ok(Decl::Filter {
            collection,
            pred: Some(pred),
            src,
        })
    }

    fn parse_guard(&mut self) -> Result<Decl, Error> {
        let collection = self.expect_ident()?;
        self.expect_char('.')?;
        let field = self.expect_ident()?;
        self.expect_kw("immutable")?;
        self.expect_kw("when")?;
        let pred = self.parse_pred()?;
        Ok(Decl::Guard {
            collection,
            field,
            pred,
        })
    }

    fn parse_idb(&mut self) -> Result<Stmt, Error> {
        if self.eat_kw("slice") {
            return Ok(Stmt::IdbSlice {
                query: self.parse_query()?,
            });
        }
        Err(self.err("expected `slice` after `idb`"))
    }
}

#[inline]
fn is_ident_start_byte(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

#[inline]
fn is_ident_continue_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[inline]
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn try_into_simple_by_field(mut by: Vec<GroupKey>) -> Result<Option<Field>, Vec<GroupKey>> {
    if by.is_empty() {
        Ok(None)
    } else if by.len() == 1
        && matches!(
            &by[0].expr,
            GroupExpr::Field(f) if by[0].alias == f.as_cow_str().as_ref()
        )
    {
        let GroupKey {
            expr: GroupExpr::Field(f),
            ..
        } = by.pop().unwrap()
        else {
            unreachable!()
        };
        Ok(Some(f))
    } else {
        Err(by)
    }
}

fn default_pct_alias(pct: f64) -> String {
    AggFunc::default_percentile_alias(pct)
}

#[cfg(test)]
mod absolute_timestamp_tests {
    use super::*;

    #[test]
    fn timestamp_literals_preserve_signed_milliseconds_and_name_syntax() {
        for millis in [i64::MIN, -1, 0, 1, 1700000000123, i64::MAX] {
            let src = format!("timestamp({millis})");
            let mut parser = Parser::new(&src);
            let value = parser.parse_value().unwrap();
            assert_eq!(value, Value::Timestamp(millis));
            assert!(parser.eof());
            assert_eq!(crate::plan::fmt_value(&value), src);
        }
        let mut parser = Parser::new("timestamp");
        assert_eq!(
            parser.parse_value().unwrap(),
            Value::Name("timestamp".into())
        );
        for src in [
            "timestamp()",
            "timestamp(1.2)",
            "timestamp(1d)",
            "timestamp(9223372036854775808)",
            "timestamp(-9223372036854775809)",
            "timestamp(1",
            "timestamp(now)",
        ] {
            assert!(Parser::new(src).parse_value().is_err(), "{src}");
        }
    }
}

/// Chains of more terms than this are joined as a balanced tree.
const BALANCED_CHAIN_MIN: usize = 32;

/// Join a chain of `and` / `or` terms. Short chains keep the left-nested shape. A long
/// chain is joined pairwise into a balanced tree, so a million terms nest about twenty
/// levels deep instead of a million: checking, planning, evaluating and dropping the
/// predicate recurse along that depth, and a left-nested chain of that length overflowed
/// the stack. Both operators are associative and side-effect free, and pairwise joining
/// keeps the terms in source order, so results do not change.
fn join_chain(mut terms: Vec<Pred>, join: fn(Box<Pred>, Box<Pred>) -> Pred) -> Pred {
    if terms.len() < BALANCED_CHAIN_MIN {
        let mut it = terms.into_iter();
        let first = it.next().expect("a chain has at least one term");
        return it.fold(first, |left, right| join(Box::new(left), Box::new(right)));
    }
    while terms.len() > 1 {
        let mut next = Vec::with_capacity(terms.len().div_ceil(2));
        let mut it = terms.into_iter();
        while let Some(a) = it.next() {
            match it.next() {
                Some(b) => next.push(join(Box::new(a), Box::new(b))),
                None => next.push(a),
            }
        }
        terms = next;
    }
    terms.pop().expect("a chain has at least one term")
}

// ============================================================================
// Parse-Only Inspection, Canonical AST Formatter & Malformed Query Minimizer
// (`D12`: `LIN-0469`..`LIN-0472`, `LIN-0480`)
// ============================================================================

use std::collections::BTreeSet;
use serde::{Deserialize, Serialize};

/// Summary returned by [`parse_inspect`] without requiring a live [`crate::catalog::Catalog`] (`LIN-0469`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseInspection {
    pub stmt_count: usize,
    pub referenced_collections: Vec<String>,
    pub parameter_names: Vec<String>,
    pub is_read_only: bool,
}

/// Parse `src` and extract referenced collections, `$param` bindings, statement count,
/// and read-only classification (`LIN-0469`).
pub fn parse_inspect(src: &str) -> Result<ParseInspection, Error> {
    let stmts = parse_program(src)?;
    let mut cols = BTreeSet::new();
    let mut params = BTreeSet::new();
    let mut let_names = BTreeSet::new();
    let mut is_read_only = true;

    for stmt in &stmts {
        inspect_stmt(stmt, &mut cols, &mut params, &mut let_names, &mut is_read_only);
    }

    Ok(ParseInspection {
        stmt_count: stmts.len(),
        referenced_collections: cols.into_iter().collect(),
        parameter_names: params.into_iter().collect(),
        is_read_only,
    })
}

fn inspect_stmt(
    stmt: &Stmt,
    cols: &mut BTreeSet<String>,
    params: &mut BTreeSet<String>,
    let_names: &mut BTreeSet<String>,
    is_read_only: &mut bool,
) {
    match stmt {
        Stmt::Query(q) | Stmt::IdbSlice { query: q } => {
            inspect_query(q, cols, params, let_names);
        }
        Stmt::Let { name, query } => {
            inspect_query(query, cols, params, let_names);
            let_names.insert(name.clone());
        }
        Stmt::IdbPull { .. } => {}
        Stmt::AppendFacts { records } => {
            *is_read_only = false;
            cols.insert("facts".into());
            for r in records {
                inspect_record(r, params);
            }
        }
        Stmt::AppendEdges { edges } => {
            *is_read_only = false;
            for e in edges {
                inspect_value(&e.from, params);
                inspect_value(&e.to, params);
            }
        }
        Stmt::Insert {
            collection,
            records,
            edges,
        } => {
            *is_read_only = false;
            cols.insert(collection.clone());
            for r in records {
                inspect_record(r, params);
            }
            for e in edges {
                if let EdgeTarget::Value(v) = &e.target {
                    inspect_value(v, params);
                }
            }
        }
        Stmt::InsertRows { collection, .. } => {
            *is_read_only = false;
            cols.insert(collection.clone());
        }
        Stmt::Update {
            collection,
            pred,
            record,
            ..
        } => {
            *is_read_only = false;
            cols.insert(collection.clone());
            if let Some(p) = pred {
                inspect_pred(p, params);
            }
            inspect_record(record, params);
        }
        Stmt::Delete {
            collection, pred, ..
        } => {
            *is_read_only = false;
            cols.insert(collection.clone());
            inspect_pred(pred, params);
        }
        Stmt::DeleteEdge { from, to, .. } => {
            *is_read_only = false;
            inspect_value(from, params);
            inspect_value(to, params);
        }
        Stmt::Reembed { collection, .. } => {
            *is_read_only = false;
            cols.insert(collection.clone());
        }
        Stmt::Decl(d) => {
            *is_read_only = false;
            match d {
                Decl::Col { name, .. } => {
                    cols.insert(name.clone());
                }
                Decl::Fk {
                    from_col, to_col, ..
                } => {
                    cols.insert(from_col.clone());
                    cols.insert(to_col.clone());
                }
                Decl::Guard {
                    collection, pred, ..
                } => {
                    cols.insert(collection.clone());
                    inspect_pred(pred, params);
                }
                Decl::Filter {
                    collection, pred, ..
                } => {
                    cols.insert(collection.clone());
                    if let Some(p) = pred {
                        inspect_pred(p, params);
                    }
                }
                Decl::Index {
                    collection, pred, ..
                } => {
                    cols.insert(collection.clone());
                    if let Some(p) = pred {
                        inspect_pred(p, params);
                    }
                }
                Decl::Rel { .. } | Decl::RelReverse { .. } | Decl::Owned { .. } => {}
            }
        }
        Stmt::IdbPush
        | Stmt::Snapshot { .. }
        | Stmt::Restore { .. }
        | Stmt::Pin { .. }
        | Stmt::Unpin { .. } => {
            *is_read_only = false;
        }
    }
}

fn inspect_query(
    q: &Query,
    cols: &mut BTreeSet<String>,
    params: &mut BTreeSet<String>,
    let_names: &BTreeSet<String>,
) {
    match &q.source {
        Source::Collection(c) => {
            if !let_names.contains(c) {
                cols.insert(c.clone());
            }
        }
        Source::Page(_) => {
            cols.insert("docs".into());
        }
        Source::Catalog => {
            cols.insert("catalog".into());
        }
    }
    for step in &q.steps {
        match step {
            Step::Filter(p) => inspect_pred(p, params),
            Step::Join { collection, .. } => {
                cols.insert(collection.clone());
            }
            Step::Union(rhs) => inspect_query(rhs, cols, params, let_names),
            _ => {}
        }
    }
}

fn inspect_record(rec: &Record, params: &mut BTreeSet<String>) {
    for (_, v) in &rec.fields {
        inspect_value(v, params);
    }
}

fn inspect_pred(pred: &Pred, params: &mut BTreeSet<String>) {
    match pred {
        Pred::And(a, b) | Pred::Or(a, b) => {
            inspect_pred(a, params);
            inspect_pred(b, params);
        }
        Pred::Not(inner) => inspect_pred(inner, params),
        Pred::Cmp { value, .. } => inspect_value(value, params),
        Pred::In { values, .. } => {
            for v in values {
                inspect_value(v, params);
            }
        }
        Pred::IsNull { .. }
        | Pred::Has { .. }
        | Pred::Contains { .. }
        | Pred::Regex { .. } => {}
    }
}

fn inspect_value(val: &Value, params: &mut BTreeSet<String>) {
    if let Value::Param(p) = val {
        params.insert(p.to_string());
    }
}

/// Canonical, deterministic AST formatter for a program of statements (`LIN-0470`).
pub fn format_program_canonical(stmts: &[Stmt]) -> String {
    let mut out = String::new();
    for (i, stmt) in stmts.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format_stmt_canonical(stmt));
    }
    out
}

/// Canonical, deterministic AST formatter that round-trips through [`parse`] to an equal [`Stmt`] (`LIN-0470`, `LIN-0471`).
pub fn format_stmt_canonical(stmt: &Stmt) -> String {
    let mut out = String::new();
    fmt_stmt_canon(&mut out, stmt);
    out
}

/// Canonical formatter for a [`Query`] (`LIN-0470`).
pub fn format_query_canonical(q: &Query) -> String {
    let mut out = String::new();
    fmt_query_canon(&mut out, q);
    out
}

fn fmt_stmt_canon(out: &mut String, stmt: &Stmt) {
    use std::fmt::Write as _;
    match stmt {
        Stmt::Query(q) => fmt_query_canon(out, q),
        Stmt::Let { name, query } => {
            let _ = write!(out, "let {name} = ");
            fmt_query_canon(out, query);
        }
        Stmt::AppendFacts { records } => {
            out.push_str("append facts ");
            fmt_record_list_canon(out, records, true);
        }
        Stmt::AppendEdges { edges } => {
            if edges.len() == 1 {
                out.push_str("append edge ");
                fmt_edge_lit_canon(out, &edges[0]);
            } else {
                out.push_str("append edges [");
                for (i, e) in edges.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    fmt_edge_lit_canon(out, e);
                }
                out.push(']');
            }
        }
        Stmt::Insert {
            collection,
            records,
            edges,
        } => {
            let _ = write!(out, "insert {collection} ");
            if records.len() == 1 {
                fmt_record_canon(out, &records[0]);
                if !edges.is_empty() {
                    out.push_str(" with");
                    for e in edges {
                        out.push_str(" edge ");
                        out.push_str(&e.rel);
                        out.push_str(" -> ");
                        match &e.target {
                            EdgeTarget::Page(p) => {
                                out.push_str("page ");
                                fmt_string_canon(out, p);
                            }
                            EdgeTarget::Value(v) => fmt_value_canon(out, v),
                        }
                    }
                }
            } else {
                fmt_record_list_canon(out, records, true);
            }
        }
        Stmt::InsertRows { collection, rows } => {
            let _ = write!(out, "insert {collection} /* staged rows={rows} */");
        }
        Stmt::Update {
            collection,
            pred,
            cas,
            cas_each,
            record,
        } => {
            out.push_str("update ");
            out.push_str(collection);
            if let Some(p) = pred {
                out.push('[');
                fmt_pred_canon(out, p, 0);
                out.push(']');
            }
            fmt_cas_canon(out, cas.as_deref(), *cas_each);
            out.push(' ');
            fmt_record_canon(out, record);
        }
        Stmt::Delete {
            collection,
            pred,
            cas,
            cas_each,
        } => {
            out.push_str("delete ");
            out.push_str(collection);
            out.push('[');
            fmt_pred_canon(out, pred, 0);
            out.push(']');
            fmt_cas_canon(out, cas.as_deref(), *cas_each);
        }
        Stmt::DeleteEdge { rel, from, to } => {
            out.push_str("delete edge ");
            out.push_str(rel);
            out.push(' ');
            fmt_value_canon(out, from);
            out.push_str(" -> ");
            fmt_value_canon(out, to);
        }
        Stmt::Reembed { collection, to } => {
            out.push_str("reembed ");
            out.push_str(collection);
            if let Some(t) = to {
                let _ = write!(out, " to {}/{}/{}", t.provider, t.model, t.dim);
            }
        }
        Stmt::Decl(d) => fmt_decl_canon(out, d),
        Stmt::IdbSlice { query } => {
            out.push_str("idb slice ");
            fmt_query_canon(out, query);
        }
        Stmt::IdbPull { since, take } => {
            let _ = write!(out, "pull idb since {since}");
            if let Some(n) = take {
                let _ = write!(out, " take {n}");
            }
        }
        Stmt::IdbPush => out.push_str("push idb"),
        Stmt::Snapshot { name } => {
            out.push_str("snapshot ");
            fmt_string_canon(out, name);
        }
        Stmt::Restore { name } => {
            out.push_str("restore ");
            fmt_string_canon(out, name);
        }
        Stmt::Pin { name } => {
            out.push_str("pin ");
            fmt_string_canon(out, name);
        }
        Stmt::Unpin { name } => {
            out.push_str("unpin ");
            fmt_string_canon(out, name);
        }
    }
}

fn fmt_cas_canon(out: &mut String, cas: Option<&str>, cas_each: bool) {
    if cas_each {
        out.push_str(" cas each");
    } else if let Some(h) = cas {
        out.push_str(" cas ");
        fmt_string_canon(out, h);
    }
}

fn fmt_edge_lit_canon(out: &mut String, e: &EdgeLit) {
    out.push_str(&e.rel);
    out.push(' ');
    fmt_value_canon(out, &e.from);
    out.push_str(" -> ");
    fmt_value_canon(out, &e.to);
}

fn fmt_record_list_canon(out: &mut String, records: &[Record], force_brackets: bool) {
    if !force_brackets && records.len() == 1 {
        fmt_record_canon(out, &records[0]);
        return;
    }
    out.push('[');
    for (i, r) in records.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        fmt_record_canon(out, r);
    }
    out.push(']');
}

fn fmt_record_canon(out: &mut String, rec: &Record) {
    out.push_str("{ ");
    for (i, (k, v)) in rec.fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(k);
        out.push_str(": ");
        fmt_value_canon(out, v);
    }
    out.push_str(" }");
}

fn fmt_decl_canon(out: &mut String, d: &Decl) {
    use std::fmt::Write as _;
    match d {
        Decl::Col {
            name,
            append,
            fields,
        } => {
            out.push_str("col ");
            out.push_str(name);
            if *append {
                out.push_str(" append");
            }
            out.push_str(" { ");
            for (i, (fname, ty)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(fname);
                out.push_str(": ");
                fmt_type_expr_canon(out, ty);
            }
            out.push_str(" }");
        }
        Decl::Rel { name, stub } => {
            out.push_str("rel ");
            out.push_str(name);
            if *stub {
                out.push_str(" stub");
            }
        }
        Decl::RelReverse { name, of } => {
            let _ = write!(out, "rel {name} = reverse {of}");
        }
        Decl::Fk {
            from_col,
            from_field,
            to_col,
            to_field,
        } => {
            let _ = write!(out, "fk {from_col}.{from_field} -> {to_col}.{to_field}");
        }
        Decl::Guard {
            collection,
            field,
            pred,
        } => {
            let _ = write!(out, "guard {collection}.{field} immutable when ");
            fmt_pred_canon(out, pred, 0);
        }
        Decl::Filter {
            collection,
            pred,
            src,
        } => match pred {
            None => {
                let _ = write!(out, "unfilter {collection}");
            }
            Some(p) => {
                let _ = write!(out, "filter {collection} ");
                if !src.is_empty() && parse_pred_src(src).ok().as_ref() == Some(p) {
                    out.push_str(src);
                } else {
                    fmt_pred_canon(out, p, 0);
                }
            }
        },
        Decl::Index {
            collection,
            unique,
            fields,
            pred,
        } => {
            out.push_str("index ");
            out.push_str(collection);
            if *unique {
                out.push_str(" unique");
            }
            out.push_str(" [");
            out.push_str(&fields.join(", "));
            out.push(']');
            if let Some(p) = pred {
                out.push_str(" where ");
                fmt_pred_canon(out, p, 0);
            }
        }
        Decl::Owned { name, fields } => {
            let _ = write!(out, "owned {name} {{ ");
            for (i, (fname, ty)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(fname);
                out.push_str(": ");
                fmt_type_expr_canon(out, ty);
            }
            out.push_str(" }");
        }
    }
}

fn fmt_type_expr_canon(out: &mut String, ty: &TypeExpr) {
    use std::fmt::Write as _;
    match ty {
        TypeExpr::Named(n) => out.push_str(n),
        TypeExpr::Vec { dim, model } => {
            let _ = write!(out, "vec[{dim}]@{model}");
        }
    }
}

fn fmt_query_canon(out: &mut String, q: &Query) {
    match &q.source {
        Source::Collection(c) => out.push_str(c),
        Source::Page(uri) => {
            out.push_str("page ");
            fmt_string_canon(out, uri);
        }
        Source::Catalog => out.push_str("catalog"),
    }
    if q.ignore_filter {
        out.push_str(" all");
    }
    for step in &q.steps {
        out.push_str(" | ");
        fmt_step_canon(out, step);
    }
    if let Some(exp) = q.explain {
        out.push_str(" | explain");
        match exp {
            ExplainKind::Tree => {}
            ExplainKind::Cost => out.push_str(" cost"),
            ExplainKind::Run => out.push_str(" run"),
            ExplainKind::Backend => out.push_str(" backend"),
            ExplainKind::Graph => out.push_str(" graph"),
            ExplainKind::Dot => out.push_str(" dot"),
        }
    }
}

fn fmt_step_canon(out: &mut String, step: &Step) {
    use std::fmt::Write as _;
    match step {
        Step::Filter(p) => {
            out.push_str("where ");
            fmt_pred_canon(out, p, 0);
        }
        Step::Project(fields) => {
            out.push_str("pick { ");
            fmt_fields_canon(out, fields);
            out.push_str(" }");
        }
        Step::Without(fields) => {
            out.push_str("without { ");
            fmt_fields_canon(out, fields);
            out.push_str(" }");
        }
        Step::Join {
            left,
            collection,
            on,
        } => {
            out.push_str("join ");
            if *left {
                out.push_str("left ");
            }
            let _ = write!(out, "{collection} on {on}");
        }
        Step::Hop { rel, depth } => {
            out.push_str("hop ");
            out.push_str(rel);
            if let Some(d) = depth {
                let _ = write!(out, " depth={d}");
            }
        }
        Step::Graph { rel, depth } => {
            out.push_str("graph ");
            out.push_str(rel);
            if let Some(d) = depth {
                let _ = write!(out, " depth={d}");
            }
        }
        Step::Match { start, hops } => {
            out.push_str("match ");
            if let Some(s) = start {
                out.push_str(s);
                out.push(' ');
            }
            for (i, h) in hops.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                let _ = write!(out, "{h}");
            }
        }
        Step::Search { mode, query } => {
            out.push_str("search ");
            match mode {
                SearchMode::Hybrid => {}
                SearchMode::Lex => out.push_str("lex "),
                SearchMode::Vec => out.push_str("vec "),
            }
            fmt_string_canon(out, query);
        }
        Step::Count { by } => {
            out.push_str("count");
            if let Some(f) = by {
                out.push_str(" by ");
                out.push_str(&f.as_str());
            }
        }
        Step::Sum { field, by } => fmt_legacy_agg_canon(out, "sum", field, by.as_ref()),
        Step::Min { field, by } => fmt_legacy_agg_canon(out, "min", field, by.as_ref()),
        Step::Max { field, by } => fmt_legacy_agg_canon(out, "max", field, by.as_ref()),
        Step::Avg { field, by } => fmt_legacy_agg_canon(out, "avg", field, by.as_ref()),
        Step::Summarize { specs, by } => {
            out.push_str("summarize { ");
            for (i, s) in specs.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&s.alias);
                out.push_str(": ");
                match &s.func {
                    AggFunc::Count => out.push_str("count"),
                    AggFunc::Sum(f) => {
                        let _ = write!(out, "sum({f})");
                    }
                    AggFunc::Avg(f) => {
                        let _ = write!(out, "avg({f})");
                    }
                    AggFunc::Min(f) => {
                        let _ = write!(out, "min({f})");
                    }
                    AggFunc::Max(f) => {
                        let _ = write!(out, "max({f})");
                    }
                    AggFunc::DCount(f) => {
                        let _ = write!(out, "dcount({f})");
                    }
                    AggFunc::Percentile { field, pct } => {
                        let _ = write!(out, "percentile({field}, ");
                        fmt_float_canon(out, *pct);
                        out.push(')');
                    }
                    AggFunc::ArgMax { order, value } => {
                        let _ = write!(out, "arg_max({order}, {value})");
                    }
                    AggFunc::ArgMin { order, value } => {
                        let _ = write!(out, "arg_min({order}, {value})");
                    }
                }
            }
            out.push_str(" }");
            fmt_by_keys_canon(out, by);
        }
        Step::ArgExtremum { is_max, order, by } => {
            let fn_name = if *is_max { "arg_max" } else { "arg_min" };
            let _ = write!(out, "{fn_name}({order}, *)");
            fmt_by_keys_canon(out, by);
        }
        Step::Sort { field, desc } => {
            let _ = write!(out, "sort {field} {}", if *desc { "desc" } else { "asc" });
        }
        Step::Window {
            func,
            field,
            by,
            order,
            frame,
            as_field,
        } => {
            out.push_str("window ");
            match func {
                crate::window::WindowFunction::RowNumber => out.push_str("row_number"),
                crate::window::WindowFunction::Rank => out.push_str("rank"),
                crate::window::WindowFunction::DenseRank => out.push_str("dense_rank"),
                crate::window::WindowFunction::Lead { offset } => {
                    let _ = write!(out, "lead({offset}) {}", field.as_ref().unwrap());
                }
                crate::window::WindowFunction::Lag { offset } => {
                    let _ = write!(out, "lag({offset}) {}", field.as_ref().unwrap());
                }
                crate::window::WindowFunction::FirstValue => {
                    let _ = write!(out, "first_value({})", field.as_ref().unwrap());
                }
                crate::window::WindowFunction::LastValue => {
                    let _ = write!(out, "last_value({})", field.as_ref().unwrap());
                }
                crate::window::WindowFunction::Sum => {
                    let _ = write!(out, "sum({})", field.as_ref().unwrap());
                }
                crate::window::WindowFunction::Avg => {
                    let _ = write!(out, "avg({})", field.as_ref().unwrap());
                }
                crate::window::WindowFunction::Min => {
                    let _ = write!(out, "min({})", field.as_ref().unwrap());
                }
                crate::window::WindowFunction::Max => {
                    let _ = write!(out, "max({})", field.as_ref().unwrap());
                }
                crate::window::WindowFunction::Count => match field {
                    Some(f) => {
                        let _ = write!(out, "count({f})");
                    }
                    None => out.push_str("count(*)"),
                },
            }
            if !by.is_empty() {
                out.push_str(" by [");
                fmt_fields_canon(out, by);
                out.push(']');
            }
            if !order.is_empty() {
                out.push_str(" sort [");
                for (i, s) in order.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(
                        out,
                        "{} {}",
                        s.field,
                        if s.desc { "desc" } else { "asc" }
                    );
                }
                out.push(']');
            }
            if let Some(fr) = frame {
                let _ = write!(out, " {fr}");
            }
            let _ = write!(out, " as {as_field}");
        }
        Step::Skip { n } => {
            let _ = write!(out, "skip {n}");
        }
        Step::Take { n } => match n {
            Some(v) => {
                let _ = write!(out, "take {v}");
            }
            None => out.push_str("take all"),
        },
        Step::Union(rhs) => {
            out.push_str("union (");
            fmt_query_canon(out, rhs);
            out.push(')');
        }
    }
}

fn fmt_legacy_agg_canon(out: &mut String, kw: &str, field: &Field, by: Option<&Field>) {
    use std::fmt::Write as _;
    let _ = write!(out, "{kw}({field})");
    if let Some(b) = by {
        let _ = write!(out, " by {b}");
    }
}

fn fmt_by_keys_canon(out: &mut String, by: &[GroupKey]) {
    use std::fmt::Write as _;
    if by.is_empty() {
        return;
    }
    out.push_str(" by [");
    for (i, gk) in by.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&gk.alias);
        out.push_str(": ");
        match &gk.expr {
            GroupExpr::Field(f) => {
                let _ = write!(out, "{f}");
            }
            GroupExpr::Bin { field, step } => {
                let _ = write!(out, "bin({field}, ");
                match step {
                    BinStep::Duration(ms) => fmt_millis_as_duration(out, *ms),
                    BinStep::Int(n) => {
                        let _ = write!(out, "{n}");
                    }
                    BinStep::Float(f) => fmt_float_canon(out, *f),
                }
                out.push(')');
            }
        }
    }
    out.push(']');
}

fn fmt_millis_as_duration(out: &mut String, ms: i64) {
    use std::fmt::Write as _;
    let units = [
        (7 * 86_400_000, 'w'),
        (86_400_000, 'd'),
        (3_600_000, 'h'),
        (60_000, 'm'),
        (1_000, 's'),
    ];
    for ( factor, ch) in units {
        if ms >= factor && ms % factor == 0 {
            let _ = write!(out, "{}{ch}", ms / factor);
            return;
        }
    }
    let secs = (ms / 1_000).max(1);
    let _ = write!(out, "{secs}s");
}

fn fmt_fields_canon(out: &mut String, fields: &[Field]) {
    use std::fmt::Write as _;
    for (i, f) in fields.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        let _ = write!(out, "{f}");
    }
}

/// Precedence levels for exact parenthesization:
/// - `0`: top-level (`Or` context)
/// - `1`: `Or` right-hand side (wrap nested `Or` so `(a or (b or c))` stays right-associated!)
/// - `2`: `And` left-hand side (wrap `Or`)
/// - `3`: `And` right-hand side (wrap `Or` and `And` so `(a and (b and c))` stays right-associated!)
/// - `4`: `Not` operand (wrap `And` and `Or`)
fn fmt_pred_canon(out: &mut String, pred: &Pred, ctx_prec: u8) {
    use std::fmt::Write as _;
    match pred {
        Pred::Or(a, b) => {
            let wrap = ctx_prec >= 1;
            if wrap {
                out.push('(');
            }
            fmt_pred_canon(out, a, 0);
            out.push_str(" or ");
            fmt_pred_canon(out, b, 1);
            if wrap {
                out.push(')');
            }
        }
        Pred::And(a, b) => {
            let wrap = ctx_prec >= 3;
            if wrap {
                out.push('(');
            }
            fmt_pred_canon(out, a, 2);
            out.push_str(" and ");
            fmt_pred_canon(out, b, 3);
            if wrap {
                out.push(')');
            }
        }
        Pred::Not(inner) => {
            out.push_str("not ");
            match inner.as_ref() {
                Pred::And(_, _) | Pred::Or(_, _) => {
                    out.push('(');
                    fmt_pred_canon(out, inner, 0);
                    out.push(')');
                }
                _ => fmt_pred_canon(out, inner, 4),
            }
        }
        Pred::Cmp { field, op, value } => {
            let _ = write!(out, "{field} {} ", op.as_str());
            fmt_value_canon(out, value);
        }
        Pred::IsNull { field, negated } => {
            if *negated {
                let _ = write!(out, "{field} is not null");
            } else {
                let _ = write!(out, "{field} is null");
            }
        }
        Pred::In { field, values } => {
            let _ = write!(out, "{field} in [");
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_value_canon(out, v);
            }
            out.push(']');
        }
        Pred::Has { field, ci, needle } => {
            let _ = write!(out, "{field} {}", if *ci { "has ci " } else { "has " });
            fmt_string_canon(out, needle);
        }
        Pred::Contains { field, needle } => {
            let _ = write!(out, "{field} ~ ");
            fmt_string_canon(out, needle);
        }
        Pred::Regex {
            field,
            pattern,
            flags,
        } => {
            let _ = write!(out, "{field} ~ /{pattern}/{flags}");
        }
    }
}

fn fmt_value_canon(out: &mut String, val: &Value) {
    use std::fmt::Write as _;
    match val {
        Value::String(s) => fmt_string_canon(out, s),
        Value::Int(n) => {
            let _ = write!(out, "{n}");
        }
        Value::Float(f) => fmt_float_canon(out, *f),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Timestamp(ms) => {
            let _ = write!(out, "timestamp({ms})");
        }
        Value::Now => out.push_str("now"),
        Value::NowMinus(d) => {
            let _ = write!(out, "now - {d}");
        }
        Value::Duration(d) => {
            let _ = write!(out, "{d}");
        }
        Value::Name(n) => out.push_str(n),
        Value::Param(p) => {
            out.push('$');
            out.push_str(p);
        }
    }
}

fn fmt_float_canon(out: &mut String, f: f64) {
    use std::fmt::Write as _;
    if f == 0.0 && f.is_sign_negative() {
        out.push_str("-0.0");
        return;
    }
    let start = out.len();
    let _ = write!(out, "{f}");
    if f.is_finite() && !out[start..].contains('.') {
        out.push_str(".0");
    }
}

fn fmt_string_canon(out: &mut String, s: &str) {
    use std::fmt::Write as _;
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Delta-debugging (`ddmin`) line/stage/token shrinker for malformed queries (`LIN-0480`).
///
/// Preserves the compile failure class (syntax failure vs checker diagnostic category) while
/// deterministically stripping irrelevant statements, pipe stages, and tokens.
pub fn minimize_malformed_query(src: &str) -> String {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum FailureSig<'a> {
        Empty,
        Parse(&'a str),
        Check(&'a str),
    }

    fn classify_msg<'a>(msg: &str, prefixes: &'a [&'a str], fallback: &'a str) -> &'a str {
        for &p in prefixes {
            if msg.contains(p) {
                return p;
            }
        }
        fallback
    }

    const PARSE_SIGS: &[&str] = &[
        "unterminated string",
        "unterminated regex",
        "decoded string literal",
        "AST recursion depth",
        "query length",
        "mutation on pipe",
        "trailing input",
        "`explain` must be the last step",
        "with not allowed on bulk insert",
        "ago requires a duration",
        "bad \\u{..} escape",
        "empty list",
        "expected",
    ];

    const CHECK_SIGS: &[&str] = &[
        "unknown collection",
        "unknown field",
        "unknown rel",
        "type mismatch",
        "join requires fk",
        "immutable field",
        "update requires cas",
        "delete requires cas",
        "cannot bind over collection",
        "union columns mismatch",
        "union type mismatch",
        "requires numeric field",
        "requires text",
        "search vec requires embedding",
        "search lex requires fts",
    ];

    let cat = crate::catalog::fixture();
    let probe = |candidate: &str| -> Option<FailureSig<'static>> {
        match parse_program(candidate) {
            Err(e) => {
                if e.message.contains("empty program") {
                    Some(FailureSig::Empty)
                } else {
                    Some(FailureSig::Parse(classify_msg(
                        &e.message,
                        PARSE_SIGS,
                        "parse_err",
                    )))
                }
            }
            Ok(stmts) => match crate::check::check_program(&stmts, &cat) {
                Err(e) => Some(FailureSig::Check(classify_msg(
                    &e.message,
                    CHECK_SIGS,
                    "check_err",
                ))),
                Ok(_) => None,
            },
        }
    };

    let Some(target_sig) = probe(src) else {
        return src.to_string();
    };
    if target_sig == FailureSig::Empty {
        return String::new();
    }

    let budget = std::cell::Cell::new(512usize);
    let mut same_failure = |cand: &str| -> bool {
        let rem = budget.get();
        if cand.trim().is_empty() || rem == 0 {
            return false;
        }
        budget.set(rem - 1);
        probe(cand) == Some(target_sig)
    };

    let mut current = src.trim().to_string();

    // Pass 1: Line-level ddmin
    let lines: Vec<String> = current.lines().map(str::to_string).collect();
    if lines.len() > 1 {
        let shrunk_lines = ddmin_units(&lines, "\n", &mut same_failure);
        let joined = shrunk_lines.join("\n");
        if joined.len() < current.len() {
            current = joined;
        }
    }

    // Pass 2: Pipe-stage ddmin (keep source `parts[0]`, shrink pipe steps `parts[1..]`)
    if current.contains('|') && !current.contains('\n') {
        let (head, cur_stages_init) = {
            let parts: Vec<&str> = current.split('|').map(str::trim).collect();
            (
                parts[0].to_string(),
                parts[1..].iter().map(|s| (*s).to_string()).collect::<Vec<_>>(),
            )
        };
        if cur_stages_init.len() >= 2 {
            let mut gran = 2usize;
            let mut cur_stages = cur_stages_init;
            while cur_stages.len() >= 2 && budget.get() > 0 {
                let chunk_size = cur_stages.len().div_ceil(gran);
                let mut reduced = false;
                let mut idx = 0;
                while idx < cur_stages.len() {
                    let end = (idx + chunk_size).min(cur_stages.len());
                    if cur_stages.len() - (end - idx) >= 1 {
                        let mut cand_stages = Vec::with_capacity(cur_stages.len() - (end - idx));
                        cand_stages.extend_from_slice(&cur_stages[..idx]);
                        cand_stages.extend_from_slice(&cur_stages[end..]);
                        let cand = format!("{head} | {}", cand_stages.join(" | "));
                        if same_failure(&cand) {
                            cur_stages = cand_stages;
                            current = cand;
                            gran = (gran - 1).max(2);
                            reduced = true;
                            break;
                        }
                    }
                    idx += chunk_size;
                }
                if !reduced {
                    if gran >= cur_stages.len() {
                        break;
                    }
                    gran = (gran * 2).min(cur_stages.len());
                }
            }
        }
    }

    // Pass 3: Whitespace-separated token ddmin
    let tokens: Vec<String> = split_shrink_tokens(&current);
    if tokens.len() > 1 {
        let shrunk_toks = ddmin_units(&tokens, " ", &mut same_failure);
        let candidate = shrunk_toks.join(" ");
        if candidate.len() < current.len() && (probe(&candidate) == Some(target_sig)) {
            current = candidate;
        }
    }

    current
}

fn ddmin_units(
    initial: &[String],
    sep: &str,
    test: &mut impl FnMut(&str) -> bool,
) -> Vec<String> {
    let mut units = initial.to_vec();
    let mut n = 2usize;
    while units.len() >= 2 {
        let chunk_len = units.len().div_ceil(n);
        let mut changed = false;

        // Try reducing directly to a single subset first when n == 2..8
        if n <= 8 {
            let mut i = 0;
            while i < units.len() {
                let end = (i + chunk_len).min(units.len());
                let subset = &units[i..end];
                if subset.len() < units.len() {
                    let cand = subset.join(sep);
                    if test(&cand) {
                        units = subset.to_vec();
                        n = 2;
                        changed = true;
                        break;
                    }
                }
                i += chunk_len;
            }
            if changed {
                continue;
            }
        }

        // Try removing each complement chunk
        let mut i = 0;
        while i < units.len() {
            let end = (i + chunk_len).min(units.len());
            if end - i < units.len() {
                let mut comp = Vec::with_capacity(units.len() - (end - i));
                comp.extend_from_slice(&units[..i]);
                comp.extend_from_slice(&units[end..]);
                let cand = comp.join(sep);
                if test(&cand) {
                    units = comp;
                    n = (n - 1).max(2);
                    changed = true;
                    break;
                }
            }
            i += chunk_len;
        }
        if !changed {
            if n >= units.len() {
                break;
            }
            n = (n * 2).min(units.len());
        }
    }
    units
}

fn split_shrink_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i] == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                    continue;
                }
                if bytes[i] == b'"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            out.push(s[start..i].to_string());
            continue;
        }
        let start = i;
        if matches!(bytes[i], b'|' | b'{' | b'}' | b'[' | b']' | b'(' | b')' | b',') {
            i += 1;
        } else {
            while i < bytes.len()
                && !bytes[i].is_ascii_whitespace()
                && !matches!(bytes[i], b'|' | b'{' | b'}' | b'[' | b']' | b'(' | b')' | b',')
            {
                let ch = s[i..].chars().next().unwrap();
                i += ch.len_utf8();
            }
        }
        out.push(s[start..i].to_string());
    }
    out
}
