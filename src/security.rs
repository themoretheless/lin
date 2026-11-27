//! Capability-based access control, import path sandboxing, UTF-8 safety scanning, and resource-limit receipts (`D45`: `LIN-1777`..`LIN-1796`).

use std::path::{Component, Path, PathBuf};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::error::{Error, ErrorCode, redact_sensitive_path};

// ---------------------------------------------------------------------------
// Collection & Session Capabilities (`LIN-1777`..`LIN-1780`, `LIN-1786`..`LIN-1788`)
// ---------------------------------------------------------------------------

/// Fine-grained per-collection operation capability flags (`LIN-1778`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct CollectionCapability {
    pub read: bool,
    pub insert: bool,
    pub update: bool,
    pub delete: bool,
    pub ddl: bool,
    pub export: bool,
}

impl CollectionCapability {
    pub const BIT_READ: u8 = 1 << 0;
    pub const BIT_INSERT: u8 = 1 << 1;
    pub const BIT_UPDATE: u8 = 1 << 2;
    pub const BIT_DELETE: u8 = 1 << 3;
    pub const BIT_DDL: u8 = 1 << 4;
    pub const BIT_EXPORT: u8 = 1 << 5;

    #[inline]
    pub const fn none() -> Self {
        Self {
            read: false,
            insert: false,
            update: false,
            delete: false,
            ddl: false,
            export: false,
        }
    }

    #[inline]
    pub const fn read_only() -> Self {
        Self {
            read: true,
            insert: false,
            update: false,
            delete: false,
            ddl: false,
            export: false,
        }
    }

    #[inline]
    pub const fn read_write() -> Self {
        Self {
            read: true,
            insert: true,
            update: true,
            delete: true,
            ddl: false,
            export: false,
        }
    }

    #[inline]
    pub const fn all() -> Self {
        Self {
            read: true,
            insert: true,
            update: true,
            delete: true,
            ddl: true,
            export: true,
        }
    }

    #[inline]
    pub const fn to_bitmask(self) -> u8 {
        let mut bits = 0u8;
        if self.read {
            bits |= Self::BIT_READ;
        }
        if self.insert {
            bits |= Self::BIT_INSERT;
        }
        if self.update {
            bits |= Self::BIT_UPDATE;
        }
        if self.delete {
            bits |= Self::BIT_DELETE;
        }
        if self.ddl {
            bits |= Self::BIT_DDL;
        }
        if self.export {
            bits |= Self::BIT_EXPORT;
        }
        bits
    }

    #[inline]
    pub const fn from_bitmask(bits: u8) -> Self {
        Self {
            read: (bits & Self::BIT_READ) != 0,
            insert: (bits & Self::BIT_INSERT) != 0,
            update: (bits & Self::BIT_UPDATE) != 0,
            delete: (bits & Self::BIT_DELETE) != 0,
            ddl: (bits & Self::BIT_DDL) != 0,
            export: (bits & Self::BIT_EXPORT) != 0,
        }
    }

    #[inline]
    pub const fn union(self, other: Self) -> Self {
        Self::from_bitmask(self.to_bitmask() | other.to_bitmask())
    }

    #[inline]
    pub const fn intersection(self, other: Self) -> Self {
        Self::from_bitmask(self.to_bitmask() & other.to_bitmask())
    }
}

/// Extension trust tier (`LIN-1779`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ExtensionTrustBoundary {
    Untrusted,
    Sandboxed,
    BuiltinVerified,
}

/// Session/handle security policy with deny-by-default unsafe operations and offline-by-default model access.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityPolicy {
    pub read_only: bool,
    pub deny_unsafe_ops: bool,
    pub allow_network_models: bool,
    pub allowed_import_root: Option<PathBuf>,
    pub per_collection: FxHashMap<String, CollectionCapability>,
}

impl Default for CapabilityPolicy {
    fn default() -> Self {
        Self {
            read_only: false,
            deny_unsafe_ops: true,
            allow_network_models: false,
            allowed_import_root: None,
            per_collection: FxHashMap::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClassifiedOp {
    Read,
    Insert,
    Update,
    Delete,
    Ddl,
    Export,
    Unsafe,
    MetadataOnly,
}

fn classify_stmt_kind(stmt_kind: &str) -> Option<ClassifiedOp> {
    match stmt_kind.trim().to_ascii_lowercase().as_str() {
        "read" | "query" | "select" | "scan" | "get" | "search" | "match" | "hop" => {
            Some(ClassifiedOp::Read)
        }
        "explain" | "ping" | "stats" | "health" => Some(ClassifiedOp::MetadataOnly),
        "insert" | "bulk_insert" | "append" | "import" => Some(ClassifiedOp::Insert),
        "update" | "upsert" | "reembed" | "merge" => Some(ClassifiedOp::Update),
        "delete" | "evict" => Some(ClassifiedOp::Delete),
        "ddl" | "create" | "drop" | "alter" | "truncate" | "create_index" | "drop_index"
        | "create_view" | "drop_view" => Some(ClassifiedOp::Ddl),
        "export" | "backup" | "export_backup" | "export_wal" | "dump" => Some(ClassifiedOp::Export),
        "unsafe"
        | "ffi"
        | "load_extension"
        | "plugin"
        | "raw_mmap"
        | "raw_wal_truncate"
        | "disable_checksum"
        | "shell"
        | "exec_host" => Some(ClassifiedOp::Unsafe),
        _ => None,
    }
}

impl CapabilityPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn strict_read_only() -> Self {
        Self {
            read_only: true,
            ..Self::default()
        }
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn with_deny_unsafe_ops(mut self, deny_unsafe_ops: bool) -> Self {
        self.deny_unsafe_ops = deny_unsafe_ops;
        self
    }

    pub fn with_allow_network_models(mut self, allow_network_models: bool) -> Self {
        self.allow_network_models = allow_network_models;
        self
    }

    pub fn with_allowed_import_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.allowed_import_root = Some(root.into());
        self
    }

    pub fn with_collection_capability(
        mut self,
        collection: impl Into<String>,
        cap: CollectionCapability,
    ) -> Self {
        self.per_collection.insert(collection.into(), cap);
        self
    }

    /// Checks whether `stmt_kind` on optional `collection` is permitted by this policy (`LIN-1777`, `LIN-1778`, `LIN-1786`).
    pub fn check_stmt_allowed(
        &self,
        stmt_kind: &str,
        collection: Option<&str>,
    ) -> Result<(), Error> {
        let Some(op) = classify_stmt_kind(stmt_kind) else {
            return Err(Error::permission_denied(format!(
                "permission denied: unrecognized or disallowed statement kind `{stmt_kind}`"
            ))
            .with_operation(stmt_kind));
        };

        if op == ClassifiedOp::Unsafe {
            if self.deny_unsafe_ops {
                return Err(Error::permission_denied(format!(
                    "permission denied: unsafe operation `{stmt_kind}` is disabled by default"
                ))
                .with_operation(stmt_kind)
                .with_suggestion("explicitly set `deny_unsafe_ops = false` in a trusted host context"));
            }
            if self.read_only {
                return Err(Error::permission_denied(format!(
                    "permission denied: unsafe operation `{stmt_kind}` forbidden in read-only policy"
                ))
                .with_operation(stmt_kind));
            }
            return Ok(());
        }

        if self.read_only
            && matches!(
                op,
                ClassifiedOp::Insert
                    | ClassifiedOp::Update
                    | ClassifiedOp::Delete
                    | ClassifiedOp::Ddl
            )
        {
            let mut err = Error::permission_denied(format!(
                "permission denied: `{stmt_kind}` is forbidden under read-only capability policy"
            ))
            .with_operation(stmt_kind);
            if let Some(col) = collection {
                err = err.with_collection(col);
            }
            return Err(err);
        }

        if op == ClassifiedOp::MetadataOnly {
            return Ok(());
        }

        if !self.per_collection.is_empty() {
            let Some(col) = collection else {
                return Err(Error::permission_denied(format!(
                    "permission denied: `{stmt_kind}` requires explicit target collection under scoped policy"
                ))
                .with_operation(stmt_kind));
            };
            let Some(cap) = self.per_collection.get(col) else {
                return Err(Error::permission_denied(format!(
                    "permission denied: no capability granted for collection `{col}`"
                ))
                .with_operation(stmt_kind)
                .with_collection(col));
            };
            let allowed = match op {
                ClassifiedOp::Read => cap.read,
                ClassifiedOp::Insert => cap.insert,
                ClassifiedOp::Update => cap.update,
                ClassifiedOp::Delete => cap.delete,
                ClassifiedOp::Ddl => cap.ddl,
                ClassifiedOp::Export => cap.export,
                ClassifiedOp::Unsafe | ClassifiedOp::MetadataOnly => unreachable!(),
            };
            if !allowed {
                return Err(Error::permission_denied(format!(
                    "permission denied: operation `{stmt_kind}` not granted on collection `{col}`"
                ))
                .with_operation(stmt_kind)
                .with_collection(col));
            }
        }

        Ok(())
    }

    /// Enforces explicit network model permissions; offline local hashing embedders always succeed (`LIN-1780`, `LIN-1787`, `LIN-1796`).
    pub fn check_network_model_allowed(&self, provider: &str) -> Result<(), Error> {
        let normalized = provider.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return Err(Error::permission_denied(
                "permission denied: empty model provider identifier",
            ));
        }
        let is_offline_local = matches!(
            normalized.as_str(),
            "hashing" | "local" | "builtin" | "offline" | "memory" | "onnx-local"
        );
        if is_offline_local || self.allow_network_models {
            return Ok(());
        }
        Err(Error::permission_denied(format!(
            "permission denied: network model provider `{provider}` blocked in offline-by-default policy"
        ))
        .with_operation("load_network_model")
        .with_suggestion(
            "use offline `HashingEmbedder` / pre-staged local model or set `allow_network_models = true`",
        ))
    }

    /// Enforces extension trust boundary isolation (`LIN-1779`).
    pub fn check_extension_trust(&self, boundary: ExtensionTrustBoundary) -> Result<(), Error> {
        match boundary {
            ExtensionTrustBoundary::BuiltinVerified => Ok(()),
            ExtensionTrustBoundary::Sandboxed if !self.deny_unsafe_ops || !self.read_only => Ok(()),
            ExtensionTrustBoundary::Sandboxed | ExtensionTrustBoundary::Untrusted => {
                if self.deny_unsafe_ops {
                    Err(Error::permission_denied(
                        "permission denied: untrusted extension blocked by `deny_unsafe_ops` boundary",
                    )
                    .with_operation("load_extension"))
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Validates an import/export path against NUL bytes, `..` traversal, backslash traversal,
    /// symlink escapes, and `allowed_import_root` containment (`LIN-1788`).
    pub fn validate_sandboxed_path(&self, path: &Path) -> Result<PathBuf, Error> {
        let raw_bytes = path.as_os_str().as_encoded_bytes();
        if raw_bytes.is_empty() {
            return Err(
                Error::permission_denied("sandboxed path validation failed: empty path")
                    .with_operation("validate_sandboxed_path"),
            );
        }
        if raw_bytes.contains(&0) {
            return Err(Error::permission_denied(
                "sandboxed path validation failed: embedded NUL byte rejected",
            )
            .with_operation("validate_sandboxed_path"));
        }

        // Reject `..` components as well as Windows-style `..\` segments passed on POSIX hosts.
        for comp in path.components() {
            match comp {
                Component::ParentDir => {
                    return Err(Error::permission_denied(format!(
                        "sandboxed path validation failed: parent traversal `..` rejected in `{}`",
                        redact_sensitive_path(path)
                    ))
                    .with_operation("validate_sandboxed_path"));
                }
                Component::Normal(seg) => {
                    let s = seg.to_string_lossy();
                    if s.split('\\').any(|part| part == "..") {
                        return Err(Error::permission_denied(
                            "sandboxed path validation failed: backslash `..` traversal rejected",
                        )
                        .with_operation("validate_sandboxed_path"));
                    }
                }
                Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
            }
        }

        let Some(ref root) = self.allowed_import_root else {
            return Ok(lexical_normalize_no_parent(path));
        };

        if root.as_os_str().as_encoded_bytes().contains(&0) {
            return Err(Error::permission_denied(
                "sandboxed path validation failed: `allowed_import_root` contains NUL byte",
            ));
        }

        let canonical_root = resolve_with_existing_ancestor(root)?;
        let candidate = if path.is_relative() {
            canonical_root.join(path)
        } else {
            path.to_path_buf()
        };
        let resolved = resolve_with_existing_ancestor(&candidate)?;

        if !resolved.starts_with(&canonical_root) {
            return Err(Error::permission_denied(format!(
                "sandboxed path validation failed: `{}` escapes allowed_import_root",
                redact_sensitive_path(path)
            ))
            .with_operation("validate_sandboxed_path"));
        }

        Ok(resolved)
    }
}

fn lexical_normalize_no_parent(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Canonicalizes the deepest existing filesystem prefix of `target` and appends any not-yet-created
/// normal child segments so both non-existent output files and symlink escape attacks are resolved accurately.
fn resolve_with_existing_ancestor(target: &Path) -> Result<PathBuf, Error> {
    let normalized = lexical_normalize_no_parent(target);
    let mut cur: &Path = &normalized;
    let mut tail_rev: Vec<&std::ffi::OsStr> = Vec::new();

    while !cur.as_os_str().is_empty() {
        if cur.exists() {
            let mut base = std::fs::canonicalize(cur).map_err(|e| {
                Error::io(format!(
                    "failed to canonicalize `{}`: {e}",
                    redact_sensitive_path(cur)
                ))
            })?;
            for seg in tail_rev.into_iter().rev() {
                base.push(seg);
            }
            return Ok(base);
        }
        // If `cur` is a dangling symlink (`symlink_metadata` succeeds while `exists()` is false),
        // inspect its link target so a broken symlink pointing outside root cannot bypass containment.
        if let Ok(meta) = std::fs::symlink_metadata(cur)
            && meta.file_type().is_symlink()
        {
            let link_target = std::fs::read_link(cur).map_err(|e| {
                Error::io(format!(
                    "failed to read symlink `{}`: {e}",
                    redact_sensitive_path(cur)
                ))
            })?;
            if link_target.components().any(|c| c == Component::ParentDir) {
                return Err(Error::permission_denied(
                    "sandboxed path validation failed: symlink with `..` traversal rejected",
                ));
            }
            let rebased = if link_target.is_absolute() {
                link_target
            } else {
                cur.parent().unwrap_or_else(|| Path::new("")).join(link_target)
            };
            let mut base = resolve_with_existing_ancestor(&rebased)?;
            for seg in tail_rev.into_iter().rev() {
                base.push(seg);
            }
            return Ok(base);
        }
        if let Some(name) = cur.file_name() {
            tail_rev.push(name);
            cur = cur.parent().unwrap_or_else(|| Path::new(""));
        } else {
            break;
        }
    }

    if normalized.is_relative()
        && let Ok(cwd) = std::env::current_dir()
    {
        let canon_cwd = std::fs::canonicalize(&cwd).unwrap_or(cwd);
        return Ok(canon_cwd.join(normalized));
    }
    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Resource Limit Receipts (`LIN-1785`)
// ---------------------------------------------------------------------------

/// Structured machine-readable receipt emitted when an operation hits a configured quota/limit (`LIN-1785`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceLimitReceipt {
    pub limit_kind: String,
    pub configured_limit: u64,
    pub observed_value: u64,
    pub collection: Option<String>,
    pub remediation: String,
}

impl ResourceLimitReceipt {
    pub fn new(
        limit_kind: impl Into<String>,
        configured_limit: u64,
        observed_value: u64,
        collection: Option<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            limit_kind: limit_kind.into(),
            configured_limit,
            observed_value,
            collection,
            remediation: remediation.into(),
        }
    }

    #[inline]
    pub fn exceeded_by(&self) -> u64 {
        self.observed_value.saturating_sub(self.configured_limit)
    }

    pub fn to_error(&self) -> Error {
        let mut err = Error::resource_limit(format!(
            "resource limit `{}` exceeded: observed {} > configured {}",
            self.limit_kind, self.observed_value, self.configured_limit
        ))
        .with_code(ErrorCode::ResourceLimit)
        .with_suggestion(self.remediation.clone());
        if let Some(ref col) = self.collection {
            err = err.with_collection(col.clone());
        }
        err
    }
}

// ---------------------------------------------------------------------------
// Single-pass Bounded Pathological UTF-8 / Control-Char Scanner (`LIN-1783`)
// ---------------------------------------------------------------------------

/// Summary returned by [`scan_utf8_safety`] in a single `O(N)` pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Utf8ScanSummary {
    pub byte_len: usize,
    pub char_count: usize,
    pub ascii_bytes: usize,
    pub multibyte_chars: usize,
    pub control_char_count: usize,
    pub bidi_override_count: usize,
    pub zero_width_count: usize,
    pub replacement_char_count: usize,
    pub max_combining_run: usize,
}

impl Utf8ScanSummary {
    /// True if bidi Trojan-Source overrides, C0/C1 control characters, replacement characters,
    /// or excessive Zalgo combining runs (`> 32`) were observed.
    #[inline]
    pub fn has_suspicious_patterns(&self) -> bool {
        self.bidi_override_count > 0
            || self.control_char_count > 0
            || self.replacement_char_count > 0
            || self.max_combining_run > 32
    }
}

#[inline]
fn is_bidi_override(ch: char) -> bool {
    matches!(
        ch,
        '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{061C}' | '\u{200E}' | '\u{200F}'
    )
}

#[inline]
fn is_zero_width(ch: char) -> bool {
    matches!(ch, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}')
}

#[inline]
fn is_combining_mark(ch: char) -> bool {
    matches!(
        ch as u32,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x064B..=0x065F
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
    )
}

/// Scans `input` in a single `O(N)` pass bounded by `max_bytes`, rejecting inputs exceeding `max_bytes`
/// in `O(1)` and rejecting embedded NUL bytes (`LIN-1783`).
pub fn scan_utf8_safety(input: &str, max_bytes: usize) -> Result<Utf8ScanSummary, Error> {
    let byte_len = input.len();
    if byte_len > max_bytes {
        return Err(Error::resource_limit(format!(
            "UTF-8 input length ({byte_len} bytes) exceeds max_bytes limit ({max_bytes})"
        ))
        .with_operation("scan_utf8_safety"));
    }

    // Fast SIMD-friendly NUL check via `memchr` before character classification.
    if let Some(nul_pos) = memchr::memchr(0, input.as_bytes()) {
        return Err(Error::corrupt(format!(
            "embedded NUL byte at offset {nul_pos} in UTF-8 input"
        ))
        .with_span(nul_pos, nul_pos + 1)
        .with_operation("scan_utf8_safety"));
    }

    let mut summary = Utf8ScanSummary {
        byte_len,
        ..Utf8ScanSummary::default()
    };

    // Fast path for pure printable ASCII slices.
    if input.is_ascii() {
        summary.char_count = byte_len;
        summary.ascii_bytes = byte_len;
        for &b in input.as_bytes() {
            if (b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r')) || b == 0x7F {
                summary.control_char_count += 1;
            }
        }
        return Ok(summary);
    }

    let mut current_combining_run = 0usize;
    for ch in input.chars() {
        summary.char_count += 1;
        if ch.is_ascii() {
            summary.ascii_bytes += 1;
            let b = ch as u8;
            if (b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r')) || b == 0x7F {
                summary.control_char_count += 1;
            }
            current_combining_run = 0;
            continue;
        }

        summary.multibyte_chars += 1;
        if is_combining_mark(ch) {
            current_combining_run += 1;
            if current_combining_run > summary.max_combining_run {
                summary.max_combining_run = current_combining_run;
            }
        } else {
            current_combining_run = 0;
        }

        if is_bidi_override(ch) {
            summary.bidi_override_count += 1;
        } else if is_zero_width(ch) {
            summary.zero_width_count += 1;
        } else if ch == '\u{FFFD}' {
            summary.replacement_char_count += 1;
        } else if ch.is_control() && !matches!(ch, '\t' | '\n' | '\r') {
            summary.control_char_count += 1;
        }
    }

    Ok(summary)
}

// ---------------------------------------------------------------------------
// FFI Buffer Validation Design, Provenance Inventory & Disclosure (`LIN-1789`, `LIN-1794`, `LIN-1795`)
// ---------------------------------------------------------------------------

/// Validates raw C ABI pointer address, alignment, and byte length without dereferencing (`LIN-1789`).
pub fn validate_c_abi_buffer_descriptor(
    ptr_addr: usize,
    elem_align: usize,
    elem_size: usize,
    len: usize,
) -> Result<usize, Error> {
    if elem_align == 0 || !elem_align.is_power_of_two() {
        return Err(Error::permission_denied(
            "C ABI validation failed: alignment must be a non-zero power of two",
        ));
    }
    if ptr_addr == 0 {
        return Err(Error::permission_denied(
            "C ABI validation failed: null pointer rejected",
        ));
    }
    if !ptr_addr.is_multiple_of(elem_align) {
        return Err(Error::permission_denied(format!(
            "C ABI validation failed: unaligned pointer 0x{ptr_addr:x} for alignment {elem_align}"
        )));
    }
    let Some(byte_len) = elem_size.checked_mul(len) else {
        return Err(Error::resource_limit(
            "C ABI validation failed: buffer byte length overflow",
        ));
    };
    if byte_len > isize::MAX as usize {
        return Err(Error::resource_limit(
            "C ABI validation failed: buffer exceeds `isize::MAX`",
        ));
    }
    if ptr_addr.checked_add(byte_len).is_none() {
        return Err(Error::permission_denied(
            "C ABI validation failed: pointer range wraps address space",
        ));
    }
    Ok(byte_len)
}

/// Audited core dependency provenance entry (`LIN-1795`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DependencyProvenanceEntry {
    pub crate_name: &'static str,
    pub license_spdx: &'static str,
    pub role: &'static str,
    pub optional_feature: Option<&'static str>,
}

pub const DEPENDENCY_PROVENANCE_INVENTORY: &[DependencyProvenanceEntry] = &[
    DependencyProvenanceEntry {
        crate_name: "crc32fast",
        license_spdx: "MIT OR Apache-2.0",
        role: "WAL/snapshot/wire CRC32 framing",
        optional_feature: None,
    },
    DependencyProvenanceEntry {
        crate_name: "memchr",
        license_spdx: "Unlicense OR MIT",
        role: "SIMD byte/substring scanning",
        optional_feature: None,
    },
    DependencyProvenanceEntry {
        crate_name: "memmap2",
        license_spdx: "MIT OR Apache-2.0",
        role: "Memory-mapped cold segments",
        optional_feature: None,
    },
    DependencyProvenanceEntry {
        crate_name: "rustls",
        license_spdx: "Apache-2.0 OR ISC OR MIT",
        role: "TLS replication & S3 transport",
        optional_feature: None,
    },
    DependencyProvenanceEntry {
        crate_name: "serde",
        license_spdx: "MIT OR Apache-2.0",
        role: "Catalog, snapshot, and receipt serialization",
        optional_feature: None,
    },
    DependencyProvenanceEntry {
        crate_name: "tokio",
        license_spdx: "MIT",
        role: "Async Db wrapper runtime",
        optional_feature: Some("async"),
    },
    DependencyProvenanceEntry {
        crate_name: "wgpu",
        license_spdx: "MIT OR Apache-2.0",
        role: "Optional GPU compute kernels",
        optional_feature: Some("gpu"),
    },
];

/// Responsible vulnerability disclosure policy metadata (`LIN-1794`).
pub const RESPONSIBLE_DISCLOSURE_GUIDANCE: &str = "\
Report security vulnerabilities privately via GitHub Security Advisories (GHSA) on the repository \
rather than public issues. Include minimal reproduction steps, affected version (`lin::VERSION`), \
and capability configuration. Acknowledged within 72 hours; coordinated patch target within 14 days.";
