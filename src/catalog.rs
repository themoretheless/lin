use std::collections::BTreeMap;

use crate::ast::{Decl, Pred, Query, Source, Step, TypeExpr};
use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Text,
    I64,
    F64,
    Bool,
    Time,
    Dur,
    Id,
    Uri,
    Rel,
    Var,
    Vec,
}

impl Type {
    pub fn name(self) -> &'static str {
        match self {
            Type::Text => "text",
            Type::I64 => "i64",
            Type::F64 => "f64",
            Type::Bool => "bool",
            Type::Time => "time",
            Type::Dur => "dur",
            Type::Id => "id",
            Type::Uri => "uri",
            Type::Rel => "rel",
            Type::Var => "var",
            Type::Vec => "vec",
        }
    }

    pub fn is_textish(self) -> bool {
        matches!(self, Type::Text | Type::Var)
    }

    pub fn is_numeric(self) -> bool {
        matches!(self, Type::I64 | Type::F64)
    }

    pub fn from_name(n: &str) -> Option<Self> {
        match n {
            "text" => Some(Type::Text),
            "i64" => Some(Type::I64),
            "f64" => Some(Type::F64),
            "bool" => Some(Type::Bool),
            "time" => Some(Type::Time),
            "dur" => Some(Type::Dur),
            "id" => Some(Type::Id),
            "uri" => Some(Type::Uri),
            "rel" => Some(Type::Rel),
            "var" => Some(Type::Var),
            "vec" => Some(Type::Vec),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub ty: Type,
    pub fts: bool,
    pub unique: bool,
}

#[derive(Debug, Clone)]
pub struct Collection {
    pub name: String,
    pub append_only: bool,
    pub fields: BTreeMap<String, FieldInfo>,
    /// Applied as the first `Filter` on reads from this collection.
    pub filter: Option<Pred>,
    pub filter_src: Option<String>,
}

impl Collection {
    pub fn has_fts(&self) -> bool {
        self.fields.values().any(|f| f.fts)
    }

    pub fn has_vec(&self) -> bool {
        self.fields.values().any(|f| f.ty == Type::Vec)
    }

    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.get(name)
    }
}

#[derive(Debug, Clone)]
pub struct Rel {
    pub name: String,
    pub stub: bool,
    pub reverse_of: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Fk {
    pub from_col: String,
    pub from_field: String,
    pub to_col: String,
    pub to_field: String,
}

#[derive(Debug, Clone)]
pub struct Guard {
    pub collection: String,
    pub field: String,
    pub pred: Pred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    pub collection: String,
    pub unique: bool,
    pub fields: Vec<String>,
}

impl IndexDef {
    pub fn label(&self) -> String {
        format!("{}[{}]", self.collection, self.fields.join(","))
    }
}

#[derive(Debug, Clone)]
pub struct Catalog {
    pub collections: BTreeMap<String, Collection>,
    pub rels: BTreeMap<String, Rel>,
    pub fks: Vec<Fk>,
    pub guards: Vec<Guard>,
    pub indexes: BTreeMap<String, IndexDef>,
    pub owned: BTreeMap<String, Vec<(String, FieldInfo)>>,
    pub embed_id: String,
}

impl Catalog {
    pub fn collection(&self, name: &str) -> Option<&Collection> {
        self.collections.get(name)
    }

    pub fn rel(&self, name: &str) -> Option<&Rel> {
        self.rels.get(name)
    }

    pub fn indexes_on(&self, collection: &str) -> Vec<&IndexDef> {
        self.indexes
            .values()
            .filter(|i| i.collection == collection)
            .collect()
    }

    pub fn find_fk(&self, from_col: &str, on_field: &str, to_col: &str) -> Option<&Fk> {
        self.fks
            .iter()
            .find(|fk| fk.from_col == from_col && fk.from_field == on_field && fk.to_col == to_col)
    }

    pub fn is_immutable_field(&self, collection: &str, field: &str) -> bool {
        self.guards
            .iter()
            .any(|g| g.collection == collection && g.field == field)
    }

    pub fn apply_decl(&mut self, d: &Decl) -> Result<(), Error> {
        match d {
            Decl::Col {
                name,
                append,
                fields,
            } => {
                if self.collections.contains_key(name) {
                    return Err(Error::new(format!("collection exists: {name}")));
                }
                let mut map = BTreeMap::new();
                for (fname, ty) in fields {
                    match ty {
                        TypeExpr::Named(n) if self.owned.contains_key(n) => {
                            let owned = self.owned[n].clone();
                            for (of, info) in owned {
                                if map.contains_key(&of) {
                                    return Err(Error::new(format!(
                                        "owned field collision: {name}.{of}"
                                    )));
                                }
                                map.insert(of, info);
                            }
                        }
                        TypeExpr::Named(n) => {
                            let ty = Type::from_name(n)
                                .ok_or_else(|| Error::new(format!("unknown type: {n}")))?;
                            map.insert(
                                fname.clone(),
                                FieldInfo {
                                    ty,
                                    fts: ty == Type::Text,
                                    unique: fname == "id" || fname == "uri",
                                },
                            );
                        }
                        TypeExpr::Vec { .. } => {
                            map.insert(
                                fname.clone(),
                                FieldInfo {
                                    ty: Type::Vec,
                                    fts: false,
                                    unique: false,
                                },
                            );
                        }
                    }
                }
                if !map.contains_key("id") && !*append {
                    map.insert("id".into(), field(Type::Id, false, true));
                }
                self.collections.insert(
                    name.clone(),
                    Collection {
                        name: name.clone(),
                        append_only: *append,
                        fields: map,
                        filter: None,
                        filter_src: None,
                    },
                );
            }
            Decl::Rel { name, stub } => {
                if self.rels.contains_key(name) {
                    return Err(Error::new(format!("rel exists: {name}")));
                }
                self.rels.insert(
                    name.clone(),
                    Rel {
                        name: name.clone(),
                        stub: *stub,
                        reverse_of: None,
                    },
                );
            }
            Decl::RelReverse { name, of } => {
                if self.rels.contains_key(name) {
                    return Err(Error::new(format!("rel exists: {name}")));
                }
                if self.rel(of).is_none() {
                    return Err(Error::new(format!("unknown rel: {of}")));
                }
                self.rels.insert(
                    name.clone(),
                    Rel {
                        name: name.clone(),
                        stub: false,
                        reverse_of: Some(of.clone()),
                    },
                );
            }
            Decl::Fk {
                from_col,
                from_field,
                to_col,
                to_field,
            } => {
                self.fks.push(Fk {
                    from_col: from_col.clone(),
                    from_field: from_field.clone(),
                    to_col: to_col.clone(),
                    to_field: to_field.clone(),
                });
            }
            Decl::Guard {
                collection,
                field,
                pred,
            } => {
                self.guards.push(Guard {
                    collection: collection.clone(),
                    field: field.clone(),
                    pred: pred.clone(),
                });
            }
            Decl::Filter {
                collection,
                pred,
                src,
            } => {
                let col = self
                    .collections
                    .get_mut(collection)
                    .ok_or_else(|| Error::new(format!("unknown collection: {collection}")))?;
                if pred.is_none() {
                    col.filter = None;
                    col.filter_src = None;
                } else {
                    col.filter = pred.clone();
                    col.filter_src = Some(src.clone());
                }
            }
            Decl::Owned { name, fields } => {
                if self.owned.contains_key(name) || self.collections.contains_key(name) {
                    return Err(Error::new(format!("owned exists: {name}")));
                }
                let mut owned_fields = Vec::new();
                for (fname, ty) in fields {
                    let (ty, fts) = match ty {
                        TypeExpr::Named(n) => {
                            let ty = Type::from_name(n)
                                .ok_or_else(|| Error::new(format!("unknown type: {n}")))?;
                            (ty, ty == Type::Text)
                        }
                        TypeExpr::Vec { .. } => (Type::Vec, false),
                    };
                    owned_fields.push((
                        fname.clone(),
                        FieldInfo {
                            ty,
                            fts,
                            unique: false,
                        },
                    ));
                }
                if owned_fields.is_empty() {
                    return Err(Error::new("empty owned"));
                }
                self.owned.insert(name.clone(), owned_fields);
            }
            Decl::Index {
                collection,
                unique,
                fields,
            } => {
                let def = IndexDef {
                    collection: collection.clone(),
                    unique: *unique,
                    fields: fields.clone(),
                };
                let label = def.label();
                if self.indexes.contains_key(&label) {
                    return Err(Error::new(format!("index exists: {label}")));
                }
                self.indexes.insert(label, def);
            }
        }
        Ok(())
    }
}

fn field(ty: Type, fts: bool, unique: bool) -> FieldInfo {
    FieldInfo { ty, fts, unique }
}

fn col(name: &str, append_only: bool, fields: &[(&str, Type, bool, bool)]) -> Collection {
    let mut map = BTreeMap::new();
    for (n, ty, fts, unique) in fields {
        map.insert((*n).to_string(), field(*ty, *fts, *unique));
    }
    Collection {
        name: name.to_string(),
        append_only,
        fields: map,
        filter: None,
        filter_src: None,
    }
}

/// Prepend catalog `filter` so check / plan / exec / cursor share one rewrite.
pub fn with_catalog_filter(q: &Query, cat: &Catalog) -> Query {
    let mut q = q.clone();
    apply_catalog_filter(&mut q, cat);
    q
}

fn apply_catalog_filter(q: &mut Query, cat: &Catalog) {
    for step in &mut q.steps {
        if let Step::Union(inner) = step {
            apply_catalog_filter(inner, cat);
        }
    }
    if q.ignore_filter {
        return;
    }
    let Source::Collection(name) = &q.source else {
        return;
    };
    let Some(pred) = cat.collection(name).and_then(|c| c.filter.clone()) else {
        return;
    };
    q.steps.insert(0, Step::Filter(pred));
}

pub fn fixture() -> Catalog {
    let mut collections = BTreeMap::new();
    collections.insert(
        "docs".into(),
        col(
            "docs",
            false,
            &[
                ("id", Type::Id, false, true),
                ("uri", Type::Uri, false, true),
                ("title", Type::Text, true, false),
                ("wing", Type::Text, false, false),
                ("room", Type::Text, false, false),
                ("layer", Type::Text, false, false),
                ("body", Type::Text, true, false),
                ("hash", Type::Text, false, false),
                ("snippet", Type::Text, false, false),
                ("ts", Type::Time, false, false),
                ("embedding", Type::Vec, false, false),
            ],
        ),
    );
    collections.insert(
        "users".into(),
        col(
            "users",
            false,
            &[
                ("id", Type::Id, false, true),
                ("email", Type::Text, false, true),
            ],
        ),
    );
    collections.insert(
        "orders".into(),
        col(
            "orders",
            false,
            &[
                ("id", Type::Id, false, true),
                ("user_id", Type::Id, false, false),
                ("total", Type::F64, false, false),
                ("ts", Type::Time, false, false),
            ],
        ),
    );
    collections.insert(
        "facts".into(),
        col(
            "facts",
            true,
            &[
                ("s", Type::Text, false, false),
                ("p", Type::Rel, false, false),
                ("o", Type::Text, false, false),
            ],
        ),
    );
    collections.insert(
        "catalog".into(),
        col(
            "catalog",
            false,
            &[
                ("kind", Type::Text, false, false),
                ("name", Type::Text, false, false),
            ],
        ),
    );

    let mut rels = BTreeMap::new();
    rels.insert(
        "wikilink".into(),
        Rel {
            name: "wikilink".into(),
            stub: true,
            reverse_of: None,
        },
    );
    rels.insert(
        "tagged".into(),
        Rel {
            name: "tagged".into(),
            stub: false,
            reverse_of: None,
        },
    );
    rels.insert(
        "tunnel".into(),
        Rel {
            name: "tunnel".into(),
            stub: false,
            reverse_of: None,
        },
    );
    rels.insert(
        "backlink".into(),
        Rel {
            name: "backlink".into(),
            stub: false,
            reverse_of: Some("wikilink".into()),
        },
    );

    Catalog {
        collections,
        rels,
        fks: vec![Fk {
            from_col: "orders".into(),
            from_field: "user_id".into(),
            to_col: "users".into(),
            to_field: "id".into(),
        }],
        guards: vec![Guard {
            collection: "docs".into(),
            field: "body".into(),
            pred: Pred::Cmp {
                field: crate::ast::Field::name("layer"),
                op: crate::ast::CmpOp::Eq,
                value: crate::ast::Value::String("raw".into()),
            },
        }],
        indexes: BTreeMap::new(),
        owned: BTreeMap::new(),
        embed_id: "nomic-embed-text/768".into(),
    }
}
