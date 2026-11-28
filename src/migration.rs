//! Catalog introspection, schema diffing, compatibility risk classification, dry-run validation,
//! and atomic transactional migrations (`D46`: `LIN-1801`..`LIN-1840`).

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Field, Pred};
use crate::catalog::{Catalog, Collection, FieldInfo, IndexDef, Type};
use crate::error::Error;
use crate::exec::Db;
use crate::persist::{ColSnap, IndexSnap};
use crate::store::{Cell, Row};

// ---------------------------------------------------------------------------
// Catalog Introspection (`LIN-1801`, `LIN-1802`, `LIN-1817`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionIntrospection {
    pub name: String,
    pub append_only: bool,
    pub fields: BTreeMap<String, Type>,
    pub unique_fields: BTreeSet<String>,
    pub fts_fields: Vec<String>,
    pub indexes: Vec<String>,
    pub filter_src: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexIntrospection {
    pub label: String,
    pub collection: String,
    pub unique: bool,
    pub fields: Vec<String>,
    pub partial_pred: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationIntrospection {
    pub name: String,
    pub stub: bool,
    pub reverse_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKeyIntrospection {
    pub from_col: String,
    pub from_field: String,
    pub to_col: String,
    pub to_field: String,
    pub session: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogIntrospection {
    pub schema_hash_u64: u64,
    pub schema_hash: String,
    pub embedder_id: String,
    pub collections: BTreeMap<String, CollectionIntrospection>,
    pub scalar_indexes: BTreeMap<String, IndexIntrospection>,
    pub fts_fields: BTreeMap<String, Vec<String>>,
    pub relations: BTreeMap<String, RelationIntrospection>,
    pub foreign_keys: Vec<ForeignKeyIntrospection>,
}

/// Inspect a [`Catalog`] snapshot into deterministic metadata maps (`LIN-1801`, `LIN-1802`, `LIN-1817`).
pub fn inspect_catalog(cat: &Catalog) -> CatalogIntrospection {
    let mut collections = BTreeMap::new();
    let mut fts_fields_map = BTreeMap::new();

    for (cname, col) in &cat.collections {
        let mut fields = BTreeMap::new();
        let mut unique_fields = BTreeSet::new();
        let mut fts = Vec::new();
        for (fname, finfo) in &col.fields {
            fields.insert(fname.clone(), finfo.ty);
            if finfo.unique {
                unique_fields.insert(fname.clone());
            }
            if finfo.fts {
                fts.push(fname.clone());
            }
        }
        if !fts.is_empty() {
            fts_fields_map.insert(cname.clone(), fts.clone());
        }
        let indexes: Vec<String> = cat
            .indexes_on(cname)
            .into_iter()
            .map(IndexDef::label)
            .collect();
        collections.insert(
            cname.clone(),
            CollectionIntrospection {
                name: cname.clone(),
                append_only: col.append_only,
                fields,
                unique_fields,
                fts_fields: fts,
                indexes,
                filter_src: col.filter_src.clone(),
            },
        );
    }

    let mut scalar_indexes = BTreeMap::new();
    for (label, idx) in &cat.indexes {
        scalar_indexes.insert(
            label.clone(),
            IndexIntrospection {
                label: label.clone(),
                collection: idx.collection.clone(),
                unique: idx.unique,
                fields: idx.fields.clone(),
                partial_pred: idx.pred.as_ref().map(ToString::to_string),
            },
        );
    }

    let mut relations = BTreeMap::new();
    for (rname, rel) in &cat.rels {
        relations.insert(
            rname.clone(),
            RelationIntrospection {
                name: rname.clone(),
                stub: rel.stub,
                reverse_of: rel.reverse_of.clone(),
            },
        );
    }

    let foreign_keys = cat
        .fks
        .iter()
        .map(|fk| ForeignKeyIntrospection {
            from_col: fk.from_col.clone(),
            from_field: fk.from_field.clone(),
            to_col: fk.to_col.clone(),
            to_field: fk.to_field.clone(),
            session: fk.session,
        })
        .collect();

    CatalogIntrospection {
        schema_hash_u64: cat.hash_u64(),
        schema_hash: crate::catalog::catalog_hash(cat),
        embedder_id: cat.embed_id.clone(),
        collections,
        scalar_indexes,
        fts_fields: fts_fields_map,
        relations,
        foreign_keys,
    }
}

/// Inspect the active [`Catalog`] of a live [`Db`] (`LIN-1801`).
pub fn inspect_db_catalog(db: &Db) -> CatalogIntrospection {
    inspect_catalog(&db.catalog)
}

// ---------------------------------------------------------------------------
// Schema Diffing & Compatibility Analysis (`LIN-1810`, `LIN-1818`, `LIN-1819`, `LIN-1838`..`LIN-1840`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SchemaChangeKind {
    AddCollection,
    RemoveCollection,
    AddField,
    RemoveField,
    ChangeFieldType,
    AddIndex,
    RemoveIndex,
    ChangeFts,
    AddRelation,
    RemoveRelation,
    ChangeEmbedder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MigrationRiskLabel {
    BackwardCompatible,
    RequiresRebuild,
    IrreversibleDataLoss,
    IncompatibleTypeChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaDiffReport {
    pub changes: Vec<(SchemaChangeKind, String, MigrationRiskLabel)>,
    pub backward_compatible: bool,
    pub forward_compatible: bool,
    pub irreversible: bool,
    pub downgrade_feasible: bool,
    pub affected_indexes: Vec<String>,
    pub affected_fts_collections: Vec<String>,
    pub affected_relations: Vec<String>,
}

/// Compare two [`Catalog`] instances and classify every change with risk, compatibility,
/// and downgrade feasibility metadata (`LIN-1810`, `LIN-1818`, `LIN-1819`, `LIN-1838`..`LIN-1840`).
pub fn diff_catalogs(from: &Catalog, to: &Catalog) -> SchemaDiffReport {
    let mut changes: Vec<(SchemaChangeKind, String, MigrationRiskLabel)> = Vec::new();
    let mut affected_indexes = BTreeSet::new();
    let mut affected_fts = BTreeSet::new();
    let mut affected_relations = BTreeSet::new();

    // 1. Embedder check
    if from.embed_id != to.embed_id {
        changes.push((
            SchemaChangeKind::ChangeEmbedder,
            format!("{} -> {}", from.embed_id, to.embed_id),
            MigrationRiskLabel::RequiresRebuild,
        ));
    }

    // 2. Collections & fields
    for (cname, from_col) in &from.collections {
        let Some(to_col) = to.collections.get(cname) else {
            changes.push((
                SchemaChangeKind::RemoveCollection,
                cname.clone(),
                MigrationRiskLabel::IrreversibleDataLoss,
            ));
            if from_col.has_fts() {
                affected_fts.insert(cname.clone());
            }
            for idx in from.indexes_on(cname) {
                affected_indexes.insert(idx.label());
            }
            continue;
        };

        for (fname, from_info) in &from_col.fields {
            match to_col.fields.get(fname) {
                None => {
                    changes.push((
                        SchemaChangeKind::RemoveField,
                        format!("{cname}.{fname}"),
                        MigrationRiskLabel::IrreversibleDataLoss,
                    ));
                    if from_info.fts {
                        affected_fts.insert(cname.clone());
                    }
                    for idx in from.indexes_on(cname) {
                        if idx.fields.iter().any(|f| f == fname) {
                            affected_indexes.insert(idx.label());
                        }
                    }
                }
                Some(to_info) => {
                    if from_info.ty != to_info.ty {
                        changes.push((
                            SchemaChangeKind::ChangeFieldType,
                            format!(
                                "{cname}.{fname}: {} -> {}",
                                from_info.ty.name(),
                                to_info.ty.name()
                            ),
                            MigrationRiskLabel::IncompatibleTypeChange,
                        ));
                        for idx in from.indexes_on(cname) {
                            if idx.fields.iter().any(|f| f == fname) {
                                affected_indexes.insert(idx.label());
                            }
                        }
                        for idx in to.indexes_on(cname) {
                            if idx.fields.iter().any(|f| f == fname) {
                                affected_indexes.insert(idx.label());
                            }
                        }
                    }
                    if from_info.fts != to_info.fts {
                        changes.push((
                            SchemaChangeKind::ChangeFts,
                            format!("{cname}.{fname}: fts {} -> {}", from_info.fts, to_info.fts),
                            MigrationRiskLabel::RequiresRebuild,
                        ));
                        affected_fts.insert(cname.clone());
                    }
                }
            }
        }

        for (fname, to_info) in &to_col.fields {
            if !from_col.fields.contains_key(fname) {
                changes.push((
                    SchemaChangeKind::AddField,
                    format!("{cname}.{fname}: {}", to_info.ty.name()),
                    MigrationRiskLabel::BackwardCompatible,
                ));
                if to_info.fts {
                    affected_fts.insert(cname.clone());
                }
            }
        }
    }

    for (cname, to_col) in &to.collections {
        if !from.collections.contains_key(cname) {
            changes.push((
                SchemaChangeKind::AddCollection,
                cname.clone(),
                MigrationRiskLabel::BackwardCompatible,
            ));
            if to_col.has_fts() {
                affected_fts.insert(cname.clone());
            }
        }
    }

    // 3. Indexes
    for (label, from_idx) in &from.indexes {
        match to.indexes.get(label) {
            None => {
                changes.push((
                    SchemaChangeKind::RemoveIndex,
                    label.clone(),
                    MigrationRiskLabel::BackwardCompatible,
                ));
                affected_indexes.insert(label.clone());
            }
            Some(to_idx) if from_idx != to_idx => {
                changes.push((
                    SchemaChangeKind::RemoveIndex,
                    label.clone(),
                    MigrationRiskLabel::BackwardCompatible,
                ));
                changes.push((
                    SchemaChangeKind::AddIndex,
                    label.clone(),
                    MigrationRiskLabel::RequiresRebuild,
                ));
                affected_indexes.insert(label.clone());
            }
            _ => {}
        }
    }
    for label in to.indexes.keys() {
        if !from.indexes.contains_key(label) {
            changes.push((
                SchemaChangeKind::AddIndex,
                label.clone(),
                MigrationRiskLabel::RequiresRebuild,
            ));
            affected_indexes.insert(label.clone());
        }
    }

    // 4. Relations
    for (rname, from_rel) in &from.rels {
        match to.rels.get(rname) {
            None => {
                changes.push((
                    SchemaChangeKind::RemoveRelation,
                    rname.clone(),
                    MigrationRiskLabel::IrreversibleDataLoss,
                ));
                affected_relations.insert(rname.clone());
            }
            Some(to_rel)
                if from_rel.stub != to_rel.stub || from_rel.reverse_of != to_rel.reverse_of =>
            {
                changes.push((
                    SchemaChangeKind::RemoveRelation,
                    rname.clone(),
                    MigrationRiskLabel::IrreversibleDataLoss,
                ));
                changes.push((
                    SchemaChangeKind::AddRelation,
                    rname.clone(),
                    MigrationRiskLabel::BackwardCompatible,
                ));
                affected_relations.insert(rname.clone());
            }
            _ => {}
        }
    }
    for rname in to.rels.keys() {
        if !from.rels.contains_key(rname) {
            changes.push((
                SchemaChangeKind::AddRelation,
                rname.clone(),
                MigrationRiskLabel::BackwardCompatible,
            ));
            affected_relations.insert(rname.clone());
        }
    }

    let irreversible = changes
        .iter()
        .any(|(_, _, risk)| *risk == MigrationRiskLabel::IrreversibleDataLoss);
    let has_incompatible_type = changes
        .iter()
        .any(|(_, _, risk)| *risk == MigrationRiskLabel::IncompatibleTypeChange);

    let backward_compatible = !irreversible
        && !has_incompatible_type
        && !changes
            .iter()
            .any(|(kind, _, _)| *kind == SchemaChangeKind::ChangeEmbedder);

    let forward_compatible = changes
        .iter()
        .all(|(kind, _, _)| matches!(kind, SchemaChangeKind::AddIndex | SchemaChangeKind::RemoveIndex));

    let downgrade_feasible = !irreversible && !has_incompatible_type;

    SchemaDiffReport {
        changes,
        backward_compatible,
        forward_compatible,
        irreversible,
        downgrade_feasible,
        affected_indexes: affected_indexes.into_iter().collect(),
        affected_fts_collections: affected_fts.into_iter().collect(),
        affected_relations: affected_relations.into_iter().collect(),
    }
}

// ---------------------------------------------------------------------------
// Migration Plans, Dry-Run Validation, & Transactional Execution (`LIN-1803`..`LIN-1816`, `LIN-1820`..`LIN-1824`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum MigrationPlan {
    AddFieldWithDefault {
        collection: String,
        field: String,
        ty: Type,
        default: Cell,
    },
    DropFieldChecked {
        collection: String,
        field: String,
    },
    RenameField {
        collection: String,
        from: String,
        to: String,
    },
    RenameCollection {
        from: String,
        to: String,
    },
    ConvertFieldType {
        collection: String,
        field: String,
        to_type: Type,
        required: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationDryRunReport {
    pub valid: bool,
    pub steps_checked: usize,
    pub affected_rows: usize,
    pub affected_collections: Vec<String>,
    pub rebuilt_indexes: Vec<String>,
    pub rebuilt_fts_collections: Vec<String>,
    pub updated_relations: Vec<String>,
    pub nullable_to_required_violations: Vec<String>,
    pub conversion_errors: Vec<String>,
    pub dependency_conflicts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReceipt {
    pub steps_applied: usize,
    pub rows_migrated: usize,
    pub previous_catalog_hash: String,
    pub new_catalog_hash: String,
    pub new_generation: u64,
    pub rebuilt_indexes: Vec<String>,
    pub rebuilt_fts_collections: Vec<String>,
}

fn pred_mentions_field(pred: &Pred, target: &str) -> bool {
    match pred {
        Pred::Cmp { field, .. }
        | Pred::Has { field, .. }
        | Pred::Contains { field, .. }
        | Pred::Regex { field, .. }
        | Pred::In { field, .. }
        | Pred::IsNull { field, .. } => field.as_str() == target || field.leaf() == Some(target),
        Pred::And(a, b) | Pred::Or(a, b) => {
            pred_mentions_field(a, target) || pred_mentions_field(b, target)
        }
        Pred::Not(inner) => pred_mentions_field(inner, target),
    }
}

fn rename_field_in_pred(pred: &mut Pred, from: &str, to: &str) {
    let rewrite = |field: &mut Field| {
        if field.as_str() == from {
            *field = Field::path(to);
        }
    };
    match pred {
        Pred::Cmp { field, .. }
        | Pred::Has { field, .. }
        | Pred::Contains { field, .. }
        | Pred::Regex { field, .. }
        | Pred::In { field, .. }
        | Pred::IsNull { field, .. } => rewrite(field),
        Pred::And(a, b) | Pred::Or(a, b) => {
            rename_field_in_pred(a, from, to);
            rename_field_in_pred(b, from, to);
        }
        Pred::Not(inner) => rename_field_in_pred(inner, from, to),
    }
}

fn cell_matches_type(cell: &Cell, ty: Type) -> bool {
    match (cell, ty) {
        (Cell::Null, _) => true,
        (Cell::Text(_) | Cell::SmallText(_), Type::Text | Type::Var | Type::Id | Type::Uri | Type::Rel) => true,
        (Cell::Int(_), Type::I64) => true,
        (Cell::Float(f), Type::F64) => f.is_finite(),
        (Cell::Int(_), Type::F64) => true,
        (Cell::Bool(_), Type::Bool) => true,
        (Cell::Time(_) | Cell::Int(_), Type::Time | Type::Dur) => true,
        (Cell::Vec(_), Type::Vec) => true,
        _ => false,
    }
}

fn convert_cell(cell: Option<&Cell>, to_type: Type, required: bool) -> Result<Cell, String> {
    let Some(c) = cell.filter(|c| !matches!(c, Cell::Null)) else {
        return if required {
            Err("null/missing value violates required constraint".to_string())
        } else {
            Ok(Cell::Null)
        };
    };

    match to_type {
        Type::Text | Type::Var | Type::Id | Type::Uri | Type::Rel => match c {
            Cell::Text(_) | Cell::SmallText(_) => Ok(c.clone()),
            Cell::Int(n) | Cell::Time(n) => Ok(Cell::from_text(&n.to_string())),
            Cell::Float(f) => Ok(Cell::from_text(&f.to_string())),
            Cell::Bool(b) => Ok(Cell::from_text(if *b { "true" } else { "false" })),
            Cell::Vec(v) => {
                let s = v
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(",");
                Ok(Cell::from_text(&format!("[{s}]")))
            }
            Cell::Null => unreachable!(),
        },
        Type::I64 => match c {
            Cell::Int(n) | Cell::Time(n) => Ok(Cell::Int(*n)),
            Cell::Float(f) if f.is_finite() && f.fract() == 0.0 && *f >= i64::MIN as f64 && *f < i64::MAX as f64 => {
                Ok(Cell::Int(*f as i64))
            }
            Cell::Bool(b) => Ok(Cell::Int(i64::from(*b))),
            _ => {
                if let Some(s) = c.text() {
                    s.trim()
                        .parse::<i64>()
                        .map(Cell::Int)
                        .map_err(|_| format!("cannot convert text `{s}` to i64"))
                } else {
                    Err(format!("cannot convert {c:?} to i64"))
                }
            }
        },
        Type::F64 => match c {
            Cell::Float(f) if f.is_finite() => Ok(Cell::Float(*f)),
            Cell::Int(n) => Ok(Cell::Float(*n as f64)),
            _ => {
                if let Some(s) = c.text() {
                    s.trim()
                        .parse::<f64>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .map(Cell::Float)
                        .ok_or_else(|| format!("cannot convert text `{s}` to finite f64"))
                } else {
                    Err(format!("cannot convert {c:?} to f64"))
                }
            }
        },
        Type::Bool => match c {
            Cell::Bool(b) => Ok(Cell::Bool(*b)),
            Cell::Int(0) => Ok(Cell::Bool(false)),
            Cell::Int(1) => Ok(Cell::Bool(true)),
            _ => {
                if let Some(s) = c.text() {
                    match s.trim().to_ascii_lowercase().as_str() {
                        "true" | "1" | "yes" => Ok(Cell::Bool(true)),
                        "false" | "0" | "no" => Ok(Cell::Bool(false)),
                        _ => Err(format!("cannot convert text `{s}` to bool")),
                    }
                } else {
                    Err(format!("cannot convert {c:?} to bool"))
                }
            }
        },
        Type::Time | Type::Dur => match c {
            Cell::Time(t) | Cell::Int(t) => Ok(Cell::Time(*t)),
            _ => {
                if let Some(s) = c.text() {
                    s.trim()
                        .parse::<i64>()
                        .map(Cell::Time)
                        .map_err(|_| format!("cannot convert text `{s}` to time"))
                } else {
                    Err(format!("cannot convert {c:?} to time"))
                }
            }
        },
        Type::Vec => match c {
            Cell::Vec(_) => Ok(c.clone()),
            _ => Err(format!("cannot convert {c:?} to vec")),
        },
    }
}

/// Validate a sequence of [`MigrationPlan`] steps against `db` without mutating any state,
/// checking index/FTS/FK/guard dependencies, row counts, conversion validity, and nullability (`LIN-1812`..`LIN-1816`, `LIN-1820`).
pub fn dry_run_migration(db: &Db, plan: &[MigrationPlan]) -> Result<MigrationDryRunReport, Error> {
    let mut staged_cat = db.catalog.clone();
    let mut staged_rows: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for cname in staged_cat.collections.keys() {
        staged_rows.insert(cname.clone(), db.store.collection(cname).to_vec());
    }

    let mut affected_rows = 0usize;
    let mut affected_collections = BTreeSet::new();
    let mut rebuilt_indexes = BTreeSet::new();
    let mut rebuilt_fts = BTreeSet::new();
    let mut updated_relations = BTreeSet::new();
    let mut nullable_to_required_violations = Vec::new();
    let mut conversion_errors = Vec::new();
    let mut dependency_conflicts = Vec::new();

    for (step_idx, step) in plan.iter().enumerate() {
        match step {
            MigrationPlan::AddFieldWithDefault {
                collection,
                field,
                ty,
                default,
            } => {
                let col = staged_cat.collections.get_mut(collection).ok_or_else(|| {
                    Error::missing_schema(format!(
                        "migration step {}: unknown collection `{collection}`",
                        step_idx + 1
                    ))
                })?;
                if col.fields.contains_key(field) {
                    dependency_conflicts.push(format!(
                        "step {}: field `{collection}.{field}` already exists",
                        step_idx + 1
                    ));
                    continue;
                }
                if !cell_matches_type(default, *ty) {
                    conversion_errors.push(format!(
                        "step {}: default value {default:?} does not match type {}",
                        step_idx + 1,
                        ty.name()
                    ));
                    continue;
                }
                let fts = *ty == Type::Text;
                col.fields.insert(
                    field.clone(),
                    FieldInfo {
                        ty: *ty,
                        fts,
                        unique: false,
                    },
                );
                if fts {
                    rebuilt_fts.insert(collection.clone());
                }
                if let Some(rows) = staged_rows.get_mut(collection) {
                    if !matches!(default, Cell::Null) {
                        for r in rows.iter_mut() {
                            r.insert(field.as_str(), default.clone());
                            affected_rows += 1;
                        }
                    }
                    affected_collections.insert(collection.clone());
                }
            }
            MigrationPlan::DropFieldChecked { collection, field } => {
                let col_exists = staged_cat.collections.contains_key(collection);
                if !col_exists {
                    return Err(Error::missing_schema(format!(
                        "migration step {}: unknown collection `{collection}`",
                        step_idx + 1
                    )));
                }
                if field == "id" {
                    dependency_conflicts.push(format!(
                        "step {}: cannot drop primary key `{collection}.id`",
                        step_idx + 1
                    ));
                    continue;
                }
                if !staged_cat.collections[collection].fields.contains_key(field) {
                    dependency_conflicts.push(format!(
                        "step {}: unknown field `{collection}.{field}`",
                        step_idx + 1
                    ));
                    continue;
                }
                // Check scalar index dependencies
                let mut blocked = false;
                for idx in staged_cat.indexes.values() {
                    if &idx.collection == collection
                        && (idx.fields.iter().any(|f| f == field)
                            || idx.pred.as_ref().is_some_and(|p| pred_mentions_field(p, field)))
                    {
                        dependency_conflicts.push(format!(
                            "step {}: field `{collection}.{field}` is referenced by index `{}`",
                            step_idx + 1,
                            idx.label()
                        ));
                        blocked = true;
                    }
                }
                // Check FK dependencies
                for fk in &staged_cat.fks {
                    if (&fk.from_col == collection && &fk.from_field == field)
                        || (&fk.to_col == collection && &fk.to_field == field)
                    {
                        dependency_conflicts.push(format!(
                            "step {}: field `{collection}.{field}` is referenced by foreign key `{}.{} -> {}.{}`",
                            step_idx + 1,
                            fk.from_col,
                            fk.from_field,
                            fk.to_col,
                            fk.to_field
                        ));
                        blocked = true;
                    }
                }
                // Check guard & catalog filter dependencies
                for g in &staged_cat.guards {
                    if &g.collection == collection
                        && (&g.field == field || pred_mentions_field(&g.pred, field))
                    {
                        dependency_conflicts.push(format!(
                            "step {}: field `{collection}.{field}` is referenced by guard on `{}`",
                            step_idx + 1,
                            g.field
                        ));
                        blocked = true;
                    }
                }
                if let Some(ref fpred) = staged_cat.collections[collection].filter
                    && pred_mentions_field(fpred, field)
                {
                    dependency_conflicts.push(format!(
                        "step {}: field `{collection}.{field}` is referenced by collection filter",
                        step_idx + 1
                    ));
                    blocked = true;
                }
                if blocked {
                    continue;
                }
                let removed = staged_cat
                    .collections
                    .get_mut(collection)
                    .unwrap()
                    .fields
                    .remove(field)
                    .unwrap();
                if removed.fts {
                    rebuilt_fts.insert(collection.clone());
                }
                if let Some(rows) = staged_rows.get_mut(collection) {
                    for r in rows.iter_mut() {
                        if r.remove(field).is_some() {
                            affected_rows += 1;
                        }
                    }
                    affected_collections.insert(collection.clone());
                }
            }
            MigrationPlan::RenameField {
                collection,
                from,
                to,
            } => {
                if !staged_cat.collections.contains_key(collection) {
                    return Err(Error::missing_schema(format!(
                        "migration step {}: unknown collection `{collection}`",
                        step_idx + 1
                    )));
                }
                if from == "id" || to == "id" {
                    dependency_conflicts.push(format!(
                        "step {}: cannot rename primary key `id` on `{collection}`",
                        step_idx + 1
                    ));
                    continue;
                }
                if !staged_cat.collections[collection].fields.contains_key(from) {
                    dependency_conflicts.push(format!(
                        "step {}: unknown source field `{collection}.{from}`",
                        step_idx + 1
                    ));
                    continue;
                }
                if staged_cat.collections[collection].fields.contains_key(to) {
                    dependency_conflicts.push(format!(
                        "step {}: target field `{collection}.{to}` already exists",
                        step_idx + 1
                    ));
                    continue;
                }
                let col_mut = staged_cat.collections.get_mut(collection).unwrap();
                let info = col_mut.fields.remove(from).unwrap();
                col_mut.fields.insert(to.clone(), info);
                if let Some(ref mut fpred) = col_mut.filter {
                    rename_field_in_pred(fpred, from, to);
                    col_mut.filter_src = Some(fpred.to_string());
                }
                if info.fts {
                    rebuilt_fts.insert(collection.clone());
                }
                // Rewrite affected indexes
                let old_indexes: Vec<(String, IndexDef)> = staged_cat
                    .indexes
                    .iter()
                    .filter(|(_, idx)| &idx.collection == collection)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (old_label, mut idx) in old_indexes {
                    let mut changed = false;
                    for f in &mut idx.fields {
                        if f == from {
                            *f = to.clone();
                            changed = true;
                        }
                    }
                    if let Some(ref mut p) = idx.pred
                        && pred_mentions_field(p, from)
                    {
                        rename_field_in_pred(p, from, to);
                        changed = true;
                    }
                    if changed {
                        staged_cat.indexes.remove(&old_label);
                        let new_label = idx.label();
                        rebuilt_indexes.insert(new_label.clone());
                        staged_cat.indexes.insert(new_label, idx);
                    }
                }
                for fk in &mut staged_cat.fks {
                    if &fk.from_col == collection && &fk.from_field == from {
                        fk.from_field = to.clone();
                        updated_relations.insert(format!(
                            "{}.{} -> {}.{}",
                            fk.from_col, fk.from_field, fk.to_col, fk.to_field
                        ));
                    }
                    if &fk.to_col == collection && &fk.to_field == from {
                        fk.to_field = to.clone();
                        updated_relations.insert(format!(
                            "{}.{} -> {}.{}",
                            fk.from_col, fk.from_field, fk.to_col, fk.to_field
                        ));
                    }
                }
                for g in &mut staged_cat.guards {
                    if &g.collection == collection {
                        if &g.field == from {
                            g.field = to.clone();
                        }
                        rename_field_in_pred(&mut g.pred, from, to);
                    }
                }
                if let Some(rows) = staged_rows.get_mut(collection) {
                    for r in rows.iter_mut() {
                        if let Some(val) = r.remove(from) {
                            r.insert(to.as_str(), val);
                            affected_rows += 1;
                        }
                    }
                    affected_collections.insert(collection.clone());
                }
            }
            MigrationPlan::RenameCollection { from, to } => {
                if !staged_cat.collections.contains_key(from) {
                    return Err(Error::missing_schema(format!(
                        "migration step {}: unknown collection `{from}`",
                        step_idx + 1
                    )));
                }
                if matches!(from.as_str(), "docs" | "facts" | "catalog")
                    || matches!(to.as_str(), "docs" | "facts" | "catalog")
                {
                    dependency_conflicts.push(format!(
                        "step {}: cannot rename built-in system collection (`{from}` -> `{to}`)",
                        step_idx + 1
                    ));
                    continue;
                }
                if staged_cat.collections.contains_key(to) {
                    dependency_conflicts.push(format!(
                        "step {}: target collection `{to}` already exists",
                        step_idx + 1
                    ));
                    continue;
                }
                let mut col_def: Collection = staged_cat.collections.remove(from).unwrap();
                col_def.name = to.clone();
                if col_def.has_fts() {
                    rebuilt_fts.insert(to.clone());
                }
                staged_cat.collections.insert(to.clone(), col_def);

                let old_indexes: Vec<(String, IndexDef)> = staged_cat
                    .indexes
                    .iter()
                    .filter(|(_, idx)| &idx.collection == from)
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                for (old_label, mut idx) in old_indexes {
                    staged_cat.indexes.remove(&old_label);
                    idx.collection = to.clone();
                    let new_label = idx.label();
                    rebuilt_indexes.insert(new_label.clone());
                    staged_cat.indexes.insert(new_label, idx);
                }
                for fk in &mut staged_cat.fks {
                    let mut touched = false;
                    if &fk.from_col == from {
                        fk.from_col = to.clone();
                        touched = true;
                    }
                    if &fk.to_col == from {
                        fk.to_col = to.clone();
                        touched = true;
                    }
                    if touched {
                        updated_relations.insert(format!(
                            "{}.{} -> {}.{}",
                            fk.from_col, fk.from_field, fk.to_col, fk.to_field
                        ));
                    }
                }
                for g in &mut staged_cat.guards {
                    if &g.collection == from {
                        g.collection = to.clone();
                    }
                }
                if let Some(rows) = staged_rows.remove(from) {
                    affected_rows += rows.len();
                    staged_rows.insert(to.clone(), rows);
                }
                affected_collections.insert(to.clone());
            }
            MigrationPlan::ConvertFieldType {
                collection,
                field,
                to_type,
                required,
            } => {
                let col_mut = staged_cat.collections.get_mut(collection).ok_or_else(|| {
                    Error::missing_schema(format!(
                        "migration step {}: unknown collection `{collection}`",
                        step_idx + 1
                    ))
                })?;
                let Some(finfo) = col_mut.fields.get_mut(field) else {
                    dependency_conflicts.push(format!(
                        "step {}: unknown field `{collection}.{field}`",
                        step_idx + 1
                    ));
                    continue;
                };
                let old_fts = finfo.fts;
                finfo.ty = *to_type;
                finfo.fts = *to_type == Type::Text;
                if old_fts != finfo.fts {
                    rebuilt_fts.insert(collection.clone());
                }
                for idx in staged_cat.indexes_on(collection) {
                    if idx.fields.iter().any(|f| f == field) {
                        rebuilt_indexes.insert(idx.label());
                    }
                }

                if let Some(rows) = staged_rows.get_mut(collection) {
                    for (row_idx, r) in rows.iter_mut().enumerate() {
                        let existing = r.get(field.as_str());
                        match convert_cell(existing, *to_type, *required) {
                            Ok(Cell::Null) => {
                                if r.remove(field.as_str()).is_some() {
                                    affected_rows += 1;
                                }
                            }
                            Ok(converted) => {
                                r.insert(field.as_str(), converted);
                                affected_rows += 1;
                            }
                            Err(msg) => {
                                let row_id = r
                                    .get("id")
                                    .and_then(Cell::text)
                                    .map(str::to_string)
                                    .unwrap_or_else(|| format!("#{row_idx}"));
                                if msg.contains("required constraint") {
                                    nullable_to_required_violations.push(format!(
                                        "step {}: `{collection}.{field}` on row `{row_id}`: {msg}",
                                        step_idx + 1
                                    ));
                                } else {
                                    conversion_errors.push(format!(
                                        "step {}: `{collection}.{field}` on row `{row_id}`: {msg}",
                                        step_idx + 1
                                    ));
                                }
                            }
                        }
                    }
                    affected_collections.insert(collection.clone());
                }
            }
        }
    }

    let valid = dependency_conflicts.is_empty()
        && nullable_to_required_violations.is_empty()
        && conversion_errors.is_empty();

    Ok(MigrationDryRunReport {
        valid,
        steps_checked: plan.len(),
        affected_rows,
        affected_collections: affected_collections.into_iter().collect(),
        rebuilt_indexes: rebuilt_indexes.into_iter().collect(),
        rebuilt_fts_collections: rebuilt_fts.into_iter().collect(),
        updated_relations: updated_relations.into_iter().collect(),
        nullable_to_required_violations,
        conversion_errors,
        dependency_conflicts,
    })
}

fn sync_store_extras_from_catalog(db: &mut Db) {
    let base = crate::catalog::fixture();
    db.store.extra_collections.clear();
    for (cname, col) in &db.catalog.collections {
        let needs_extra = match base.collections.get(cname) {
            None => true,
            Some(base_col) => {
                base_col.append_only != col.append_only
                    || base_col.fields.len() != col.fields.len()
                    || col
                        .fields
                        .iter()
                        .any(|(f, info)| base_col.fields.get(f).map(|b| b.ty) != Some(info.ty))
            }
        };
        if needs_extra {
            let fields = col
                .fields
                .iter()
                .map(|(fname, finfo)| {
                    let tstr = if finfo.ty == Type::Vec {
                        format!("vec@{}", db.catalog.embed_id)
                    } else {
                        finfo.ty.name().to_string()
                    };
                    (fname.clone(), tstr)
                })
                .collect();
            db.store.extra_collections.insert(
                cname.clone(),
                ColSnap {
                    name: cname.clone(),
                    append_only: col.append_only,
                    fields,
                },
            );
        }
    }

    db.store.extra_indexes.clear();
    for (label, idx) in &db.catalog.indexes {
        db.store.extra_indexes.insert(
            label.clone(),
            IndexSnap {
                collection: idx.collection.clone(),
                unique: idx.unique,
                fields: idx.fields.clone(),
                pred_src: idx.pred.as_ref().map(ToString::to_string),
            },
        );
    }

    db.store.extra_filters.clear();
    for (cname, col) in &db.catalog.collections {
        if let Some(ref fsrc) = col.filter_src {
            db.store.extra_filters.insert(cname.clone(), fsrc.clone());
        }
    }
}

/// Execute `plan` transactionally against `db`, invoking `on_progress(completed_steps, total_steps)`.
/// On any dependency, conversion, or constraint error, rolls back all catalog and row mutations atomically (`LIN-1803`..`LIN-1816`, `LIN-1820`..`LIN-1824`).
pub fn apply_migration(
    db: &mut Db,
    plan: &[MigrationPlan],
    on_progress: &mut dyn FnMut(usize, usize),
) -> Result<MigrationReceipt, Error> {
    if db.is_follower() || db.is_read_only() {
        return Err(Error::permission_denied(
            "migrations cannot run on a read-only or follower database",
        ));
    }

    let previous_catalog_hash = crate::catalog::catalog_hash(&db.catalog);
    if plan.is_empty() {
        on_progress(0, 0);
        return Ok(MigrationReceipt {
            steps_applied: 0,
            rows_migrated: 0,
            previous_catalog_hash: previous_catalog_hash.clone(),
            new_catalog_hash: previous_catalog_hash,
            new_generation: db.r#gen(),
            rebuilt_indexes: Vec::new(),
            rebuilt_fts_collections: Vec::new(),
        });
    }

    let saved_catalog = db.catalog.clone();
    let saved_store = db.store.mem_backup_pin();

    let total_steps = plan.len();
    let mut rows_migrated = 0usize;
    let mut rebuilt_indexes = BTreeSet::new();
    let mut rebuilt_fts = BTreeSet::new();

    let exec_result: Result<(), Error> = (|| {
        for (idx, step) in plan.iter().enumerate() {
            match step {
                MigrationPlan::AddFieldWithDefault {
                    collection,
                    field,
                    ty,
                    default,
                } => {
                    let col = db.catalog.collections.get_mut(collection).ok_or_else(|| {
                        Error::missing_schema(format!("unknown collection `{collection}`"))
                    })?;
                    if col.fields.contains_key(field) {
                        return Err(Error::check(format!(
                            "field `{collection}.{field}` already exists"
                        )));
                    }
                    if !cell_matches_type(default, *ty) {
                        return Err(Error::runtime(format!(
                            "default value {default:?} is incompatible with {}",
                            ty.name()
                        )));
                    }
                    let fts = *ty == Type::Text;
                    col.fields.insert(
                        field.clone(),
                        FieldInfo {
                            ty: *ty,
                            fts,
                            unique: false,
                        },
                    );
                    if fts {
                        rebuilt_fts.insert(collection.clone());
                    }
                    if !matches!(default, Cell::Null) {
                        let rows = db.store.collection_mut(collection);
                        for r in rows.iter_mut() {
                            r.insert(field.as_str(), default.clone());
                            rows_migrated += 1;
                        }
                    }
                }
                MigrationPlan::DropFieldChecked { collection, field } => {
                    if !db.catalog.collections.contains_key(collection) {
                        return Err(Error::missing_schema(format!(
                            "unknown collection `{collection}`"
                        )));
                    }
                    if field == "id" {
                        return Err(Error::check(format!(
                            "cannot drop primary key `{collection}.id`"
                        )));
                    }
                    if !db.catalog.collections[collection].fields.contains_key(field) {
                        return Err(Error::check(format!(
                            "unknown field `{collection}.{field}`"
                        )));
                    }
                    for idx_def in db.catalog.indexes.values() {
                        if &idx_def.collection == collection
                            && (idx_def.fields.iter().any(|f| f == field)
                                || idx_def
                                    .pred
                                    .as_ref()
                                    .is_some_and(|p| pred_mentions_field(p, field)))
                        {
                            return Err(Error::check(format!(
                                "cannot drop `{collection}.{field}`: referenced by index `{}`",
                                idx_def.label()
                            )));
                        }
                    }
                    for fk in &db.catalog.fks {
                        if (&fk.from_col == collection && &fk.from_field == field)
                            || (&fk.to_col == collection && &fk.to_field == field)
                        {
                            return Err(Error::check(format!(
                                "cannot drop `{collection}.{field}`: referenced by foreign key `{}.{} -> {}.{}`",
                                fk.from_col, fk.from_field, fk.to_col, fk.to_field
                            )));
                        }
                    }
                    for g in &db.catalog.guards {
                        if &g.collection == collection
                            && (&g.field == field || pred_mentions_field(&g.pred, field))
                        {
                            return Err(Error::check(format!(
                                "cannot drop `{collection}.{field}`: referenced by guard on `{}`",
                                g.field
                            )));
                        }
                    }
                    if let Some(ref fpred) = db.catalog.collections[collection].filter
                        && pred_mentions_field(fpred, field)
                    {
                        return Err(Error::check(format!(
                            "cannot drop `{collection}.{field}`: referenced by collection filter"
                        )));
                    }
                    let removed = db
                        .catalog
                        .collections
                        .get_mut(collection)
                        .unwrap()
                        .fields
                        .remove(field)
                        .unwrap();
                    if removed.fts {
                        rebuilt_fts.insert(collection.clone());
                    }
                    let rows = db.store.collection_mut(collection);
                    for r in rows.iter_mut() {
                        if r.remove(field).is_some() {
                            rows_migrated += 1;
                        }
                    }
                }
                MigrationPlan::RenameField {
                    collection,
                    from,
                    to,
                } => {
                    if !db.catalog.collections.contains_key(collection) {
                        return Err(Error::missing_schema(format!(
                            "unknown collection `{collection}`"
                        )));
                    }
                    if from == "id" || to == "id" {
                        return Err(Error::check(format!(
                            "cannot rename primary key `id` on `{collection}`"
                        )));
                    }
                    if !db.catalog.collections[collection].fields.contains_key(from) {
                        return Err(Error::check(format!(
                            "unknown field `{collection}.{from}`"
                        )));
                    }
                    if db.catalog.collections[collection].fields.contains_key(to) {
                        return Err(Error::check(format!(
                            "field `{collection}.{to}` already exists"
                        )));
                    }
                    let col_mut = db.catalog.collections.get_mut(collection).unwrap();
                    let info = col_mut.fields.remove(from).unwrap();
                    col_mut.fields.insert(to.clone(), info);
                    if let Some(ref mut fpred) = col_mut.filter {
                        rename_field_in_pred(fpred, from, to);
                        col_mut.filter_src = Some(fpred.to_string());
                    }
                    if info.fts {
                        rebuilt_fts.insert(collection.clone());
                    }

                    let old_indexes: Vec<(String, IndexDef)> = db
                        .catalog
                        .indexes
                        .iter()
                        .filter(|(_, ix)| &ix.collection == collection)
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    for (old_label, mut ix) in old_indexes {
                        let mut changed = false;
                        for f in &mut ix.fields {
                            if f == from {
                                *f = to.clone();
                                changed = true;
                            }
                        }
                        if let Some(ref mut p) = ix.pred
                            && pred_mentions_field(p, from)
                        {
                            rename_field_in_pred(p, from, to);
                            changed = true;
                        }
                        if changed {
                            db.catalog.indexes.remove(&old_label);
                            let new_label = ix.label();
                            rebuilt_indexes.insert(new_label.clone());
                            db.catalog.indexes.insert(new_label, ix);
                        }
                    }
                    for fk in &mut db.catalog.fks {
                        if &fk.from_col == collection && &fk.from_field == from {
                            fk.from_field = to.clone();
                        }
                        if &fk.to_col == collection && &fk.to_field == from {
                            fk.to_field = to.clone();
                        }
                    }
                    for g in &mut db.catalog.guards {
                        if &g.collection == collection {
                            if &g.field == from {
                                g.field = to.clone();
                            }
                            rename_field_in_pred(&mut g.pred, from, to);
                        }
                    }
                    let rows = db.store.collection_mut(collection);
                    for r in rows.iter_mut() {
                        if let Some(val) = r.remove(from) {
                            r.insert(to.as_str(), val);
                            rows_migrated += 1;
                        }
                    }
                }
                MigrationPlan::RenameCollection { from, to } => {
                    if !db.catalog.collections.contains_key(from) {
                        return Err(Error::missing_schema(format!(
                            "unknown collection `{from}`"
                        )));
                    }
                    if matches!(from.as_str(), "docs" | "facts" | "catalog")
                        || matches!(to.as_str(), "docs" | "facts" | "catalog")
                    {
                        return Err(Error::check(format!(
                            "cannot rename built-in system collection (`{from}` -> `{to}`)"
                        )));
                    }
                    if db.catalog.collections.contains_key(to) {
                        return Err(Error::check(format!("collection `{to}` already exists")));
                    }
                    let mut col_def = db.catalog.collections.remove(from).unwrap();
                    col_def.name = to.clone();
                    if col_def.has_fts() {
                        rebuilt_fts.insert(to.clone());
                    }
                    db.catalog.collections.insert(to.clone(), col_def);

                    let old_indexes: Vec<(String, IndexDef)> = db
                        .catalog
                        .indexes
                        .iter()
                        .filter(|(_, ix)| &ix.collection == from)
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    for (old_label, mut ix) in old_indexes {
                        db.catalog.indexes.remove(&old_label);
                        ix.collection = to.clone();
                        let new_label = ix.label();
                        rebuilt_indexes.insert(new_label.clone());
                        db.catalog.indexes.insert(new_label, ix);
                    }
                    for fk in &mut db.catalog.fks {
                        if &fk.from_col == from {
                            fk.from_col = to.clone();
                        }
                        if &fk.to_col == from {
                            fk.to_col = to.clone();
                        }
                    }
                    for g in &mut db.catalog.guards {
                        if &g.collection == from {
                            g.collection = to.clone();
                        }
                    }
                    let moved_rows = db.store.collections.remove(from).unwrap_or_default();
                    rows_migrated += moved_rows.len();
                    db.store.collections.insert(to.clone(), moved_rows);
                    db.store.fts.remove(from);
                    db.store.hnsw.remove(from);
                    db.store.by_id.remove(from);
                }
                MigrationPlan::ConvertFieldType {
                    collection,
                    field,
                    to_type,
                    required,
                } => {
                    let col_mut = db.catalog.collections.get_mut(collection).ok_or_else(|| {
                        Error::missing_schema(format!("unknown collection `{collection}`"))
                    })?;
                    let finfo = col_mut.fields.get_mut(field).ok_or_else(|| {
                        Error::check(format!("unknown field `{collection}.{field}`"))
                    })?;
                    let old_fts = finfo.fts;
                    finfo.ty = *to_type;
                    finfo.fts = *to_type == Type::Text;
                    if old_fts != finfo.fts {
                        rebuilt_fts.insert(collection.clone());
                    }
                    for ix in db.catalog.indexes_on(collection) {
                        if ix.fields.iter().any(|f| f == field) {
                            rebuilt_indexes.insert(ix.label());
                        }
                    }
                    let rows = db.store.collection_mut(collection);
                    for (row_idx, r) in rows.iter_mut().enumerate() {
                        let converted = convert_cell(r.get(field.as_str()), *to_type, *required)
                            .map_err(|msg| {
                                let row_id = r
                                    .get("id")
                                    .and_then(Cell::text)
                                    .map(str::to_string)
                                    .unwrap_or_else(|| format!("#{row_idx}"));
                                Error::runtime(format!(
                                    "migration conversion failed on `{collection}.{field}` (row `{row_id}`): {msg}"
                                ))
                            })?;
                        if matches!(converted, Cell::Null) {
                            if r.remove(field.as_str()).is_some() {
                                rows_migrated += 1;
                            }
                        } else {
                            r.insert(field.as_str(), converted);
                            rows_migrated += 1;
                        }
                    }
                }
            }
            on_progress(idx + 1, total_steps);
        }

        // Verify unique index constraints across all migrated collections before committing!
        db.catalog.refresh_hash();
        sync_store_extras_from_catalog(db);
        db.store.indexes.clear();
        for (label, idx_def) in &db.catalog.indexes {
            let mut live = crate::index::LiveIndex::new(idx_def.clone());
            let rows = db.store.collection(&idx_def.collection);
            for (row_i, row) in rows.iter().enumerate() {
                live.insert_at_new(row_i, row).map_err(|msg| {
                    Error::runtime(msg).with_collection(&idx_def.collection)
                })?;
            }
            db.store.indexes.insert(label.clone(), live);
        }

        db.store.rebuild_row_maps();
        db.store.rebuild_fts(&db.catalog);
        db.store.refresh_col_counts();
        db.store.r#gen += 1;
        db.clear_plan_cache();
        db.reader_cache = None;

        if let Some(p) = db.persist.as_mut() {
            p.catalog_hash = db.catalog.hash_str().to_string();
            db.store.checkpoint(p)?;
        }
        Ok(())
    })();

    if let Err(err) = exec_result {
        db.catalog = saved_catalog;
        db.store.mem_restore(saved_store);
        db.clear_plan_cache();
        db.reader_cache = None;
        return Err(err);
    }

    Ok(MigrationReceipt {
        steps_applied: total_steps,
        rows_migrated,
        previous_catalog_hash,
        new_catalog_hash: crate::catalog::catalog_hash(&db.catalog),
        new_generation: db.r#gen(),
        rebuilt_indexes: rebuilt_indexes.into_iter().collect(),
        rebuilt_fts_collections: rebuilt_fts.into_iter().collect(),
    })
}
