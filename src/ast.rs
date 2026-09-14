#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Query(Query),
    AppendFacts {
        records: Vec<Record>,
    },
    AppendEdges {
        edges: Vec<EdgeLit>,
    },
    Insert {
        collection: String,
        records: Vec<Record>,
        edges: Vec<InsertEdge>,
    },
    Update {
        collection: String,
        pred: Option<Pred>,
        cas: Option<String>,
        cas_each: bool,
        record: Record,
    },
    Delete {
        collection: String,
        pred: Pred,
        cas: Option<String>,
        cas_each: bool,
    },
    DeleteEdge {
        rel: String,
        from: Value,
        to: Value,
    },
    Let {
        name: String,
        query: Query,
    },
    Reembed {
        collection: String,
        to: Option<EmbedTarget>,
    },
    Decl(Decl),
    IdbSlice {
        query: Query,
    },
    IdbPull {
        since: i64,
        take: Option<i64>,
    },
    IdbPush,
    Snapshot {
        name: String,
    },
    Restore {
        name: String,
    },
    /// Preferred alias of [`Stmt::Snapshot`] — in-memory pin, not durable.
    Pin {
        name: String,
    },
    /// Preferred alias of [`Stmt::Restore`].
    Unpin {
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub source: Source,
    pub steps: Vec<Step>,
    pub explain: Option<ExplainKind>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Collection(String),
    Page(String),
    Catalog,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchHop {
    pub rel: String,
    /// Bound end node of this hop.
    pub bind: String,
    /// Optional edge bind: `-[e:rel]->`.
    pub edge: Option<String>,
    /// `<-rel-` / `<-[e:rel]-` instead of forward.
    pub reverse: bool,
    /// Inclusive range; default 1..=1. Cap 3.
    pub min_depth: i64,
    pub max_depth: i64,
}

impl MatchHop {
    pub fn label(&self) -> String {
        let star = if self.min_depth == 1 && self.max_depth == 1 {
            String::new()
        } else if self.min_depth == self.max_depth {
            format!("*{}", self.min_depth)
        } else {
            format!("*{}..{}", self.min_depth, self.max_depth)
        };
        let mid = match &self.edge {
            Some(e) => format!("[{e}:{}{star}]", self.rel),
            None => format!("{}{star}", self.rel),
        };
        if self.reverse {
            format!("<-{mid}- {}", self.bind)
        } else {
            format!("-{mid}-> {}", self.bind)
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Filter(Pred),
    Project(Vec<Field>),
    Join {
        left: bool,
        collection: String,
        on: String,
    },
    Hop {
        rel: String,
        depth: Option<i64>,
    },
    /// Walk from current rows; emit edge rows `{ rel, from, to }` (not nodes — use `hop`).
    Graph {
        rel: String,
        depth: Option<i64>,
    },
    /// Path pattern: `match [-rel-> bind]+` with optional start alias.
    /// Example: `match -wikilink-> b -wikilink-> c` or `match a -wikilink-> b`.
    Match {
        start: Option<String>,
        hops: Vec<MatchHop>,
    },
    Search {
        mode: SearchMode,
        query: String,
    },
    /// `count` (total hits) or `count by field` (group).
    Count {
        by: Option<Field>,
    },
    Sum {
        field: Field,
        by: Field,
    },
    Sort {
        field: Field,
        desc: bool,
    },
    /// Drop the first `n` rows (alias `offset` in the surface language).
    Skip {
        n: i64,
    },
    Take {
        n: Option<i64>,
    },
    Union(Query),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Hybrid,
    Lex,
    Vec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplainKind {
    Tree,
    Cost,
    Run,
    Backend,
    Graph,
    Dot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pred {
    And(Box<Pred>, Box<Pred>),
    Or(Box<Pred>, Box<Pred>),
    Cmp {
        field: Field,
        op: CmpOp,
        value: Value,
    },
    Has {
        field: Field,
        ci: bool,
        needle: String,
    },
    Contains {
        field: Field,
        needle: String,
    },
    Regex {
        field: Field,
        pattern: String,
        flags: String,
    },
}

impl Pred {
    pub fn and(self, other: Pred) -> Pred {
        Pred::And(Box::new(self), Box::new(other))
    }

    pub fn walk_fields<'a>(&'a self, out: &mut Vec<&'a Field>) {
        match self {
            Pred::And(a, b) | Pred::Or(a, b) => {
                a.walk_fields(out);
                b.walk_fields(out);
            }
            Pred::Cmp { field, .. }
            | Pred::Has { field, .. }
            | Pred::Contains { field, .. }
            | Pred::Regex { field, .. } => out.push(field),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

impl CmpOp {
    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Gt => ">",
            CmpOp::Lt => "<",
            CmpOp::Ge => ">=",
            CmpOp::Le => "<=",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Field {
    pub parts: Vec<String>,
}

impl Field {
    pub fn name(name: impl Into<String>) -> Self {
        Self {
            parts: vec![name.into()],
        }
    }

    /// Dotted path: `"users.email"` → `["users", "email"]`.
    pub fn path(path: impl AsRef<str>) -> Self {
        let parts: Vec<String> = path
            .as_ref()
            .split('.')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect();
        Self { parts }
    }

    pub fn as_str(&self) -> String {
        self.parts.join(".")
    }

    /// Single-segment field name without allocating (hot path for filters/projects).
    pub fn leaf(&self) -> Option<&str> {
        match self.parts.as_slice() {
            [p] => Some(p.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    NowMinus(Duration),
    Now,
    Duration(Duration),
    Name(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Duration {
    pub n: i64,
    pub unit: DurUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurUnit {
    Week,
    Day,
    Hour,
    Minute,
    Second,
}

impl DurUnit {
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            'w' => Some(DurUnit::Week),
            'd' => Some(DurUnit::Day),
            'h' => Some(DurUnit::Hour),
            'm' => Some(DurUnit::Minute),
            's' => Some(DurUnit::Second),
            _ => None,
        }
    }

    pub fn as_char(self) -> char {
        match self {
            DurUnit::Week => 'w',
            DurUnit::Day => 'd',
            DurUnit::Hour => 'h',
            DurUnit::Minute => 'm',
            DurUnit::Second => 's',
        }
    }
}

impl Duration {
    pub fn display(self) -> String {
        format!("{}{}", self.n, self.unit.as_char())
    }

    pub fn as_millis(self) -> i64 {
        let n = self.n;
        match self.unit {
            DurUnit::Week => n.saturating_mul(7 * 86_400_000),
            DurUnit::Day => n.saturating_mul(86_400_000),
            DurUnit::Hour => n.saturating_mul(3_600_000),
            DurUnit::Minute => n.saturating_mul(60_000),
            DurUnit::Second => n.saturating_mul(1_000),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub fields: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EdgeLit {
    pub rel: String,
    pub from: Value,
    pub to: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InsertEdge {
    pub rel: String,
    pub target: EdgeTarget,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EdgeTarget {
    Page(String),
    Value(Value),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbedTarget {
    pub provider: String,
    pub model: String,
    pub dim: i64,
}

impl EmbedTarget {
    pub fn display(&self) -> String {
        format!("{}/{}/{}", self.provider, self.model, self.dim)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    Col {
        name: String,
        append: bool,
        fields: Vec<(String, TypeExpr)>,
    },
    Rel {
        name: String,
        stub: bool,
    },
    RelReverse {
        name: String,
        of: String,
    },
    Fk {
        from_col: String,
        from_field: String,
        to_col: String,
        to_field: String,
    },
    Guard {
        collection: String,
        field: String,
        pred: Pred,
    },
    Index {
        collection: String,
        unique: bool,
        fields: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeExpr {
    Named(String),
    Vec { dim: i64, model: String },
}
