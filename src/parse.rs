use crate::ast::*;
use crate::error::Error;

pub fn parse(src: &str) -> Result<Stmt, Error> {
    let mut p = Parser::new(src);
    p.skip();
    let stmt = p.parse_stmt()?;
    p.skip();
    if !p.eof() {
        return Err(p.err("trailing input"));
    }
    Ok(stmt)
}

pub fn parse_program(src: &str) -> Result<Vec<Stmt>, Error> {
    let mut p = Parser::new(src);
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
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let mut cs = self.rest().chars();
        let c = cs.next()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn err(&self, msg: impl Into<String>) -> Error {
        Error::at(self.pos, msg)
    }

    fn skip(&mut self) {
        loop {
            self.skip_ws();
            if self.rest().starts_with("--") {
                while let Some(c) = self.peek() {
                    self.bump();
                    if c == '\n' {
                        break;
                    }
                }
                continue;
            }
            break;
        }
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_whitespace()) {
            self.bump();
        }
    }

    fn peek_after_skip(&self) -> Option<char> {
        let mut i = self.pos;
        let bytes = self.src.as_bytes();
        loop {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i + 1 < bytes.len() && bytes[i] == b'-' && bytes[i + 1] == b'-' {
                i += 2;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            break;
        }
        self.src[i..].chars().next()
    }

    fn starts_ident(&self) -> bool {
        matches!(self.peek(), Some(c) if is_ident_start(c))
    }

    fn peek_ident(&self) -> Option<String> {
        let mut i = self.pos;
        while i < self.src.len() && self.src.as_bytes()[i].is_ascii_whitespace() {
            i += 1;
        }
        let rest = &self.src[i..];
        let mut chars = rest.chars();
        let first = chars.next()?;
        if !is_ident_start(first) {
            return None;
        }
        let mut n = first.len_utf8();
        for c in chars {
            if !is_ident_continue(c) {
                break;
            }
            n += c.len_utf8();
        }
        Some(rest[..n].to_string())
    }

    fn eat_ident(&mut self) -> Option<String> {
        self.skip();
        if !self.starts_ident() {
            return None;
        }
        let start = self.pos;
        self.bump();
        while matches!(self.peek(), Some(c) if is_ident_continue(c)) {
            self.bump();
        }
        Some(self.src[start..self.pos].to_string())
    }

    fn expect_ident(&mut self) -> Result<String, Error> {
        self.eat_ident()
            .ok_or_else(|| self.err("expected identifier"))
    }

    fn eat_kw(&mut self, kw: &str) -> bool {
        let save = self.pos;
        self.skip();
        if let Some(id) = self.eat_ident()
            && id == kw
        {
            return true;
        }
        self.pos = save;
        false
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
        if self.peek() != Some('"') {
            return Err(self.err("expected string"));
        }
        self.bump();
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err(self.err("unterminated string")),
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    Some(c) => out.push(c),
                    None => return Err(self.err("unterminated string")),
                },
                Some(c) => out.push(c),
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
        self.skip();
        if self.eat_kw("let") {
            return self.parse_let();
        }
        if self.eat_kw("delete") {
            return self.parse_delete();
        }
        if self.eat_kw("append") {
            return self.parse_append();
        }
        if self.eat_kw("insert") {
            return self.parse_insert();
        }
        if self.eat_kw("update") {
            return self.parse_update();
        }
        if self.eat_kw("reembed") {
            return self.parse_reembed();
        }
        if self.eat_kw("index") {
            return Ok(Stmt::Decl(self.parse_index()?));
        }
        if self.eat_kw("col") {
            return Ok(Stmt::Decl(self.parse_col()?));
        }
        if self.eat_kw("rel") {
            return Ok(Stmt::Decl(self.parse_rel()?));
        }
        if self.eat_kw("fk") {
            return Ok(Stmt::Decl(self.parse_fk()?));
        }
        if self.eat_kw("guard") {
            return Ok(Stmt::Decl(self.parse_guard()?));
        }
        if self.eat_kw("idb") {
            return self.parse_idb();
        }
        if self.eat_kw("pull") {
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
        if self.eat_kw("push") {
            self.expect_kw("idb")?;
            return Ok(Stmt::IdbPush);
        }
        if self.eat_kw("snapshot") {
            let name = self.expect_string()?;
            return Ok(Stmt::Snapshot { name });
        }
        if self.eat_kw("restore") {
            let name = self.expect_string()?;
            return Ok(Stmt::Restore { name });
        }
        Ok(Stmt::Query(self.parse_query()?))
    }

    fn parse_query(&mut self) -> Result<Query, Error> {
        let source = self.parse_source()?;
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
        })
    }

    fn looks_like_mutation(&self) -> bool {
        matches!(
            self.peek_ident().as_deref(),
            Some("update" | "insert" | "append" | "reembed" | "delete" | "let" | "index")
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
        if self.eat_kw("where") {
            return Ok(Step::Filter(self.parse_pred()?));
        }
        if self.eat_kw("pick") {
            return Ok(Step::Project(self.parse_field_list()?));
        }
        if self.eat_kw("join") {
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
        if self.eat_kw("hop") {
            let rel = self.expect_ident()?;
            let depth = if self.eat_kw("depth") {
                self.expect_char('=')?;
                Some(self.expect_int()?)
            } else {
                None
            };
            return Ok(Step::Hop { rel, depth });
        }
        if self.eat_kw("graph") {
            let rel = self.expect_ident()?;
            let depth = if self.eat_kw("depth") {
                self.expect_char('=')?;
                Some(self.expect_int()?)
            } else {
                None
            };
            return Ok(Step::Graph { rel, depth });
        }
        if self.eat_kw("match") {
            return self.parse_match_step();
        }
        if self.eat_kw("search") {
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
        if self.eat_kw("count") {
            self.expect_kw("by")?;
            return Ok(Step::Count {
                by: self.parse_field()?,
            });
        }
        if self.eat_kw("sum") {
            let field = self.parse_field()?;
            self.expect_kw("by")?;
            return Ok(Step::Sum {
                field,
                by: self.parse_field()?,
            });
        }
        if self.eat_kw("sort") {
            let field = self.parse_field()?;
            let desc = self.eat_kw("desc");
            if !desc {
                self.eat_kw("asc");
            }
            return Ok(Step::Sort { field, desc });
        }
        if self.eat_kw("take") {
            if self.eat_kw("all") {
                return Ok(Step::Take { n: None });
            }
            return Ok(Step::Take {
                n: Some(self.expect_int()?),
            });
        }
        if self.starts_pred() {
            return Ok(Step::Filter(self.parse_pred()?));
        }
        Err(self.err("expected a named step or a predicate after `|`"))
    }

    /// `match [-rel-> bind]+` or `match start -rel-> bind …`
    fn parse_match_step(&mut self) -> Result<Step, Error> {
        self.skip();
        let start = if self.peek() != Some('-') {
            let name = self.expect_ident()?;
            self.skip();
            if self.peek() != Some('-') {
                return Err(self.err("expected `-rel->` after match start"));
            }
            Some(name)
        } else {
            None
        };
        let mut hops = Vec::new();
        loop {
            self.skip();
            if self.peek() != Some('-') {
                break;
            }
            self.bump();
            let rel = self.expect_ident()?;
            if !self.eat_op("->") {
                return Err(self.err("expected `->` in match hop"));
            }
            let bind = self.expect_ident()?;
            hops.push(MatchHop { rel, bind });
        }
        if hops.is_empty() {
            return Err(self.err("match requires at least one `-rel-> bind`"));
        }
        Ok(Step::Match { start, hops })
    }

    fn starts_pred(&self) -> bool {
        match self.peek_after_skip() {
            Some('(') => true,
            Some(_) => matches!(
                self.peek_ident().as_deref(),
                Some(id) if !is_step_keyword(id)
            ),
            None => false,
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
        let mut left = self.parse_and()?;
        while self.eat_kw("or") {
            let right = self.parse_and()?;
            left = Pred::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Pred, Error> {
        let mut left = self.parse_unary_pred()?;
        while self.eat_kw("and") {
            let right = self.parse_unary_pred()?;
            left = Pred::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary_pred(&mut self) -> Result<Pred, Error> {
        if self.eat_char('(') {
            let p = self.parse_pred()?;
            self.expect_char(')')?;
            return Ok(p);
        }
        self.parse_cmp()
    }

    fn parse_cmp(&mut self) -> Result<Pred, Error> {
        let field = self.parse_field()?;
        self.skip();
        if self.eat_kw("has") {
            let ci = self.eat_kw("ci");
            let needle = self.expect_string()?;
            return Ok(Pred::Has { field, ci, needle });
        }
        if self.eat_kw("regex") {
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
        if self.eat_char('~') {
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
        let op = self.parse_cmp_op()?;
        let value = self.parse_value()?;
        Ok(Pred::Cmp { field, op, value })
    }

    fn parse_cmp_op(&mut self) -> Result<CmpOp, Error> {
        self.skip();
        if self.eat_op("==") {
            return Ok(CmpOp::Eq);
        }
        if self.eat_op("!=") {
            return Ok(CmpOp::Ne);
        }
        if self.eat_op(">=") {
            return Ok(CmpOp::Ge);
        }
        if self.eat_op("<=") {
            return Ok(CmpOp::Le);
        }
        if self.eat_char('>') {
            return Ok(CmpOp::Gt);
        }
        if self.eat_char('<') {
            return Ok(CmpOp::Lt);
        }
        Err(self.err("expected comparison (`==`, `has`, `~`, …)"))
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
        if self.eat_kw("now") {
            if self.eat_char('-') {
                let dur = self.expect_duration()?;
                return Ok(Value::NowMinus(dur));
            }
            return Ok(Value::Now);
        }
        if self.eat_kw("ago") {
            return self.parse_ago();
        }
        if self.eat_kw("true") {
            return Ok(Value::Bool(true));
        }
        if self.eat_kw("false") {
            return Ok(Value::Bool(false));
        }
        if self.peek() == Some('"') {
            return Ok(Value::String(self.expect_string()?));
        }
        if let Some(v) = self.try_number()? {
            return Ok(v);
        }
        if self.starts_ident() {
            return Ok(Value::Name(self.expect_ident()?));
        }
        Err(self.err("expected value"))
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
        if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(self.err("ago requires a duration with unit (7d, 3h, 15m, 30s, 1w)"));
        }
        Err(self.err("ago requires a duration with unit (7d, 3h, 15m, 30s, 1w)"))
    }

    fn try_number(&mut self) -> Result<Option<Value>, Error> {
        self.skip();
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Ok(None);
        }
        if let Some(dur) = self.try_duration()? {
            return Ok(Some(Value::Duration(dur)));
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        let int_part = &self.src[start..self.pos];
        if self.peek() == Some('.')
            && self
                .rest()
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_digit())
        {
            self.bump();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
            }
            let s = &self.src[start..self.pos];
            let n: f64 = s.parse().map_err(|_| self.err("invalid float"))?;
            return Ok(Some(Value::Float(n)));
        }
        let n: i64 = int_part.parse().map_err(|_| self.err("invalid integer"))?;
        Ok(Some(Value::Int(n)))
    }

    fn try_duration(&mut self) -> Result<Option<Duration>, Error> {
        self.skip();
        let save = self.pos;
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Ok(None);
        }
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        let Some(unit_c) = self.peek() else {
            self.pos = save;
            return Ok(None);
        };
        let Some(unit) = DurUnit::from_char(unit_c) else {
            self.pos = save;
            return Ok(None);
        };
        if self.rest().chars().nth(1).is_some_and(is_ident_continue) {
            self.pos = save;
            return Ok(None);
        }
        let n: i64 = self.src[start..self.pos]
            .parse()
            .map_err(|_| self.err("invalid duration"))?;
        self.bump();
        Ok(Some(Duration { n, unit }))
    }

    fn expect_duration(&mut self) -> Result<Duration, Error> {
        self.try_duration()?
            .ok_or_else(|| self.err("expected duration with unit (7d, 3h, 15m, 30s, 1w)"))
    }

    fn expect_int(&mut self) -> Result<i64, Error> {
        self.skip();
        match self.try_number()? {
            Some(Value::Int(n)) => Ok(n),
            Some(Value::Duration(_)) => Err(self.err("expected integer, got duration")),
            _ => Err(self.err("expected integer")),
        }
    }

    fn parse_record(&mut self) -> Result<Record, Error> {
        self.expect_char('{')?;
        let mut fields = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some('}') {
                break;
            }
            let name = self.expect_ident()?;
            self.expect_char(':')?;
            let value = self.parse_value()?;
            fields.push((name, value));
            let comma = self.eat_char(',');
            self.skip();
            if self.peek() == Some('}') {
                break;
            }
            if !comma {
                return Err(self.err("expected `,` or `}` in record"));
            }
        }
        self.expect_char('}')?;
        if fields.is_empty() {
            return Err(self.err("record is empty"));
        }
        Ok(Record { fields })
    }

    fn parse_record_list(&mut self) -> Result<Vec<Record>, Error> {
        self.skip();
        if self.peek() == Some('[') {
            self.expect_char('[')?;
            let mut recs = Vec::new();
            loop {
                self.skip();
                if self.peek() == Some(']') {
                    break;
                }
                recs.push(self.parse_record()?);
                let comma = self.eat_char(',');
                self.skip();
                if self.peek() == Some(']') {
                    break;
                }
                if !comma {
                    return Err(self.err("expected `,` or `]` in list"));
                }
            }
            self.expect_char(']')?;
            if recs.is_empty() {
                return Err(self.err("empty list"));
            }
            return Ok(recs);
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
        self.expect_char('[')?;
        let mut edges = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some(']') {
                break;
            }
            edges.push(self.parse_edge_lit()?);
            let comma = self.eat_char(',');
            self.skip();
            if self.peek() == Some(']') {
                break;
            }
            if !comma {
                return Err(self.err("expected `,` or `]` in list"));
            }
        }
        self.expect_char(']')?;
        if edges.is_empty() {
            return Err(self.err("empty list"));
        }
        Ok(edges)
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
        self.expect_char('[')?;
        let mut fields = Vec::new();
        loop {
            self.skip();
            if self.peek() == Some(']') {
                break;
            }
            fields.push(self.expect_ident()?);
            let comma = self.eat_char(',');
            self.skip();
            if self.peek() == Some(']') {
                break;
            }
            if !comma {
                return Err(self.err("expected `,` or `]` in index"));
            }
        }
        self.expect_char(']')?;
        if fields.is_empty() {
            return Err(self.err("empty index"));
        }
        Ok(Decl::Index {
            collection,
            unique,
            fields,
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
            self.expect_char(':')?;
            let ty = self.parse_type_expr()?;
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

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_step_keyword(s: &str) -> bool {
    matches!(
        s,
        "where"
            | "pick"
            | "join"
            | "hop"
            | "graph"
            | "match"
            | "search"
            | "count"
            | "sum"
            | "sort"
            | "take"
            | "explain"
            | "union"
    )
}
