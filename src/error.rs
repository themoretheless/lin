use std::fmt;
use std::path::{Component, Path};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Phase {
    Compile,
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorKind {
    Parse,
    Check,
    Runtime,
}

/// Stable machine-readable diagnostic error codes (`D40`: `LIN-1561`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ErrorCode {
    ParseSyntax,
    ParseUnterminatedStr,
    ParseTrailing,
    CheckSchema,
    CheckType,
    CorruptData,
    UnsupportedFormat,
    NotFound,
    MissingSchema,
    Canceled,
    DeadlineExceeded,
    ResourceLimit,
    AllocationFailure,
    UniqueViolation,
    FkViolation,
    CasMismatch,
    IncompatibleModel,
    StalePrepared,
    MissingFeature,
    PermissionDenied,
    Io,
    Gpu,
    Runtime,
}

impl ErrorCode {
    pub const ALL: &'static [ErrorCode] = &[
        ErrorCode::ParseSyntax,
        ErrorCode::ParseUnterminatedStr,
        ErrorCode::ParseTrailing,
        ErrorCode::CheckSchema,
        ErrorCode::CheckType,
        ErrorCode::CorruptData,
        ErrorCode::UnsupportedFormat,
        ErrorCode::NotFound,
        ErrorCode::MissingSchema,
        ErrorCode::Canceled,
        ErrorCode::DeadlineExceeded,
        ErrorCode::ResourceLimit,
        ErrorCode::AllocationFailure,
        ErrorCode::UniqueViolation,
        ErrorCode::FkViolation,
        ErrorCode::CasMismatch,
        ErrorCode::IncompatibleModel,
        ErrorCode::StalePrepared,
        ErrorCode::MissingFeature,
        ErrorCode::PermissionDenied,
        ErrorCode::Io,
        ErrorCode::Gpu,
        ErrorCode::Runtime,
    ];

    #[inline]
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCode::ParseSyntax => "E_PARSE_SYNTAX",
            ErrorCode::ParseUnterminatedStr => "E_PARSE_UNTERMINATED_STR",
            ErrorCode::ParseTrailing => "E_PARSE_TRAILING",
            ErrorCode::CheckSchema => "E_CHECK_SCHEMA",
            ErrorCode::CheckType => "E_CHECK_TYPE",
            ErrorCode::CorruptData => "E_CORRUPT_DATA",
            ErrorCode::UnsupportedFormat => "E_UNSUPPORTED_FORMAT",
            ErrorCode::NotFound => "E_NOT_FOUND",
            ErrorCode::MissingSchema => "E_MISSING_SCHEMA",
            ErrorCode::Canceled => "E_CANCELED",
            ErrorCode::DeadlineExceeded => "E_DEADLINE_EXCEEDED",
            ErrorCode::ResourceLimit => "E_RESOURCE_LIMIT",
            ErrorCode::AllocationFailure => "E_ALLOCATION_FAILURE",
            ErrorCode::UniqueViolation => "E_UNIQUE_VIOLATION",
            ErrorCode::FkViolation => "E_FK_VIOLATION",
            ErrorCode::CasMismatch => "E_CAS_MISMATCH",
            ErrorCode::IncompatibleModel => "E_INCOMPATIBLE_MODEL",
            ErrorCode::StalePrepared => "E_STALE_PREPARED",
            ErrorCode::MissingFeature => "E_MISSING_FEATURE",
            ErrorCode::PermissionDenied => "E_PERMISSION_DENIED",
            ErrorCode::Io => "E_IO",
            ErrorCode::Gpu => "E_GPU",
            ErrorCode::Runtime => "E_RUNTIME",
        }
    }

    pub fn from_code_str(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|c| c.as_str() == code)
    }

    #[inline]
    pub const fn default_suggestion(self) -> Option<&'static str> {
        match self {
            ErrorCode::ParseSyntax => {
                Some("check query syntax near the reported byte offset or run `lin explain`")
            }
            ErrorCode::ParseUnterminatedStr => {
                Some("close the string or regex literal before the end of input")
            }
            ErrorCode::ParseTrailing => {
                Some("remove unexpected trailing tokens or separate statements cleanly")
            }
            ErrorCode::CheckSchema => {
                Some("verify field, relation, and pipeline stage compatibility with the catalog")
            }
            ErrorCode::CheckType => {
                Some("align predicate/projection expression types with the declared column schema")
            }
            ErrorCode::CorruptData => {
                Some("restore from the latest verified backup/snapshot or inspect the failing byte offset")
            }
            ErrorCode::UnsupportedFormat => {
                Some("upgrade the `lin` binary or re-export the artifact with a supported format version")
            }
            ErrorCode::NotFound => {
                Some("verify that the target row or resource identifier exists before retrying")
            }
            ErrorCode::MissingSchema => {
                Some("declare the collection or field schema via `create` before querying or mutating")
            }
            ErrorCode::Canceled => {
                Some("operation was canceled by caller; reset cancellation state before retrying")
            }
            ErrorCode::DeadlineExceeded => {
                Some("increase the execution deadline or add selective index predicates")
            }
            ErrorCode::ResourceLimit => {
                Some("increase configured `Quotas` limits or bound query cardinality with `take`")
            }
            ErrorCode::AllocationFailure => {
                Some("reduce batch/working-set size, enable spill-to-disk, or free host memory")
            }
            ErrorCode::UniqueViolation => {
                Some("use a distinct key or perform a CAS `update` on the existing record")
            }
            ErrorCode::FkViolation => {
                Some("ensure the referenced parent row exists and no dependent child rows remain")
            }
            ErrorCode::CasMismatch => {
                Some("re-read the latest row version/etag and retry the conditional write")
            }
            ErrorCode::IncompatibleModel => Some(
                "run `reembed <collection> to <model>/<dim>` or configure a matching `Embedder`",
            ),
            ErrorCode::StalePrepared => Some(
                "re-prepare the statement via `db.prepare(...)` after catalog/schema evolution",
            ),
            ErrorCode::MissingFeature => {
                Some("rebuild `lin` with the required Cargo `--features` flag enabled")
            }
            ErrorCode::PermissionDenied => {
                Some("grant the required capability in `CapabilityPolicy` or switch to a writable handle")
            }
            ErrorCode::Io => {
                Some("check filesystem/network availability, disk space, and descriptor permissions")
            }
            ErrorCode::Gpu => {
                Some("retry on CPU execution path or inspect `GpuAdapterReport` device status")
            }
            ErrorCode::Runtime => None,
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Byte-offset span `[start, end)` in query or wire source (`LIN-1563`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// Shared, `UnwindSafe + RefUnwindSafe` error cause wrapper (`LIN-1562`, `LIN-1569`).
#[derive(Clone)]
pub struct SharedErrorSource(Arc<dyn std::error::Error + Send + Sync + 'static>);

impl std::panic::UnwindSafe for SharedErrorSource {}
impl std::panic::RefUnwindSafe for SharedErrorSource {}

impl fmt::Debug for SharedErrorSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl std::ops::Deref for SharedErrorSource {
    type Target = dyn std::error::Error + 'static;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// Boxed cold-path diagnostic attachments (`LIN-1593`, `LIN-1594`).
///
/// Kept behind `Option<Box<ErrorDetails>>` so unenriched `Error` values pay zero extra
/// heap allocation and add only a single pointer word on the stack.
#[derive(Debug, Clone, Default)]
pub struct ErrorDetails {
    pub code: Option<ErrorCode>,
    pub span: Option<SourceSpan>,
    pub operation: Option<String>,
    pub collection: Option<String>,
    pub field: Option<String>,
    pub constraint: Option<String>,
    pub wal_offset: Option<u64>,
    pub vec_offset: Option<usize>,
    pub suggestion: Option<String>,
    pub source: Option<SharedErrorSource>,
}

impl PartialEq for ErrorDetails {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
            && self.span == other.span
            && self.operation == other.operation
            && self.collection == other.collection
            && self.field == other.field
            && self.constraint == other.constraint
            && self.wal_offset == other.wal_offset
            && self.vec_offset == other.vec_offset
            && self.suggestion == other.suggestion
            && self.source.as_deref().map(ToString::to_string)
                == other.source.as_deref().map(ToString::to_string)
    }
}

impl Eq for ErrorDetails {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub message: String,
    pub pos: Option<usize>,
    pub phase: Phase,
    pub details: Option<Box<ErrorDetails>>,
}

/// Longest message an error keeps. Messages quote user input (names, literals); a
/// statement carrying a megabyte identifier must not produce a megabyte error.
const MAX_MESSAGE_BYTES: usize = 4096;

#[cold]
#[inline(never)]
fn truncate_message(m: &mut String) {
    use fmt::Write as _;
    let mut cut = MAX_MESSAGE_BYTES;
    while !m.is_char_boundary(cut) {
        cut -= 1;
    }
    let dropped = m.len() - cut;
    m.truncate(cut);
    let _ = write!(m, "… [{dropped} bytes truncated]");
}

#[cold]
#[inline(never)]
fn bounded(message: impl Into<String>) -> String {
    let mut m = message.into();
    if m.len() > MAX_MESSAGE_BYTES {
        truncate_message(&mut m);
    }
    m
}

#[inline]
fn contains_ascii_icase(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    if h.len() < n.len() {
        return false;
    }
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

#[inline]
fn contains_any_icase(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| contains_ascii_icase(haystack, n))
}

fn infer_error_code(message: &str, pos: Option<usize>, phase: Phase) -> ErrorCode {
    match phase {
        Phase::Compile => {
            if contains_any_icase(
                message,
                &["unterminated string", "unterminated regex", "unterminated literal"],
            ) {
                return ErrorCode::ParseUnterminatedStr;
            }
            if contains_any_icase(
                message,
                &["trailing input", "trailing token", "unexpected trailing"],
            ) {
                return ErrorCode::ParseTrailing;
            }
            if contains_any_icase(
                message,
                &[
                    "recursion depth limit",
                    "query length",
                    "exceeds maximum",
                    "limit exceeded",
                ],
            ) {
                return ErrorCode::ResourceLimit;
            }
            if pos.is_some() {
                return ErrorCode::ParseSyntax;
            }
            if contains_any_icase(
                message,
                &[
                    "unknown collection",
                    "missing schema",
                    "no such collection",
                    "collection does not exist",
                ],
            ) {
                return ErrorCode::MissingSchema;
            }
            if contains_any_icase(
                message,
                &[
                    "type mismatch",
                    "cannot compare",
                    "invalid type",
                    "expected numeric",
                    "expected boolean",
                    "expected text",
                    "must be numeric",
                    "incompatible type",
                ],
            ) {
                return ErrorCode::CheckType;
            }
            if contains_any_icase(
                message,
                &[
                    "expected `",
                    "expected identifier",
                    "expected value",
                    "syntax error",
                    "unexpected token",
                ],
            ) {
                return ErrorCode::ParseSyntax;
            }
            ErrorCode::CheckSchema
        }
        Phase::Runtime => {
            if contains_any_icase(
                message,
                &[
                    "corrupt",
                    "checksum",
                    "crc",
                    "bad magic",
                    "bad wire magic",
                    "missing wire lcrc",
                    "truncated",
                    "malformed",
                    "invalid utf8",
                    "invalid utf-8",
                ],
            ) {
                return ErrorCode::CorruptData;
            }
            if contains_any_icase(
                message,
                &[
                    "unsupported",
                    "unknown version",
                    "format version",
                    "unknown column tag",
                    "unknown cell tag",
                ],
            ) {
                return ErrorCode::UnsupportedFormat;
            }
            if contains_any_icase(
                message,
                &[
                    "stale prepared",
                    "schema generation",
                    "catalog generation",
                    "prepared plan stale",
                ],
            ) {
                return ErrorCode::StalePrepared;
            }
            if contains_any_icase(
                message,
                &[
                    "incompatible model",
                    "embed_id mismatch",
                    "embedder mismatch",
                    "embedding dimension mismatch",
                    "model dimension",
                ],
            ) {
                return ErrorCode::IncompatibleModel;
            }
            if contains_any_icase(
                message,
                &[
                    "missing feature",
                    "feature not enabled",
                    "compiled without",
                    "requires cargo feature",
                ],
            ) {
                return ErrorCode::MissingFeature;
            }
            if contains_any_icase(
                message,
                &[
                    "permission denied",
                    "not allowed",
                    "read-only",
                    "unauthorized",
                    "capability",
                    "sandbox",
                    "unsafe operation",
                    "path traversal",
                    "network model",
                ],
            ) {
                return ErrorCode::PermissionDenied;
            }
            if contains_any_icase(message, &["deadline", "timed out", "timeout"]) {
                return ErrorCode::DeadlineExceeded;
            }
            if contains_any_icase(message, &["canceled", "cancelled", "operation aborted"]) {
                return ErrorCode::Canceled;
            }
            if contains_any_icase(
                message,
                &["allocation failed", "allocation failure", "out of memory", "oom"],
            ) {
                return ErrorCode::AllocationFailure;
            }
            if contains_any_icase(
                message,
                &[
                    "quota",
                    "resource limit",
                    "limit exceeded",
                    "budget exceeded",
                    "max rows",
                    "max bytes",
                ],
            ) {
                return ErrorCode::ResourceLimit;
            }
            if contains_any_icase(
                message,
                &[
                    "duplicate id",
                    "unique constraint",
                    "unique violation",
                    "duplicate key",
                ],
            ) {
                return ErrorCode::UniqueViolation;
            }
            if contains_any_icase(
                message,
                &["foreign key", "fk violation", "referential integrity", "referenced by"],
            ) {
                return ErrorCode::FkViolation;
            }
            if contains_any_icase(
                message,
                &["cas mismatch", "cas:", "cas failed", "version mismatch", "etag mismatch"],
            ) {
                return ErrorCode::CasMismatch;
            }
            if contains_any_icase(message, &["missing schema", "unknown collection"]) {
                return ErrorCode::MissingSchema;
            }
            if contains_any_icase(
                message,
                &["not found", "no matching row", "does not exist", "missing key"],
            ) {
                return ErrorCode::NotFound;
            }
            if contains_any_icase(message, &["gpu", "wgpu", "shader", "compute pipeline"]) {
                return ErrorCode::Gpu;
            }
            if contains_any_icase(
                message,
                &[
                    "io error",
                    "i/o",
                    "connection refused",
                    "connection reset",
                    "broken pipe",
                    "unexpected eof",
                    "sync failed",
                ],
            ) {
                return ErrorCode::Io;
            }
            ErrorCode::Runtime
        }
    }
}

impl Error {
    #[cold]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: bounded(message),
            pos: None,
            phase: Phase::Compile,
            details: None,
        }
    }

    #[cold]
    pub fn at(pos: usize, message: impl Into<String>) -> Self {
        Self {
            message: bounded(message),
            pos: Some(pos),
            phase: Phase::Compile,
            details: None,
        }
    }

    /// Compile-phase parser error constructor (zero extra heap allocation).
    #[cold]
    pub fn parse(message: impl Into<String>) -> Self {
        Self {
            message: bounded(message),
            pos: Some(0),
            phase: Phase::Compile,
            details: None,
        }
    }

    /// Compile-phase parser error constructor at an explicit byte position.
    #[cold]
    pub fn parse_at(pos: usize, message: impl Into<String>) -> Self {
        Self::at(pos, message)
    }

    /// Compile-phase semantic/schema checker error constructor (zero extra heap allocation).
    #[cold]
    pub fn check(message: impl Into<String>) -> Self {
        Self::new(message)
    }

    #[cold]
    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            message: bounded(message),
            pos: None,
            phase: Phase::Runtime,
            details: None,
        }
    }

    #[cold]
    fn runtime_coded(code: ErrorCode, message: impl Into<String>, suggestion: &str) -> Self {
        Self::runtime(message)
            .with_code(code)
            .with_suggestion(suggestion)
    }

    /// Storage/wire corruption error (`LIN-1565`).
    #[cold]
    pub fn corrupt(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::CorruptData,
            message,
            "restore from latest verified snapshot/backup or inspect failing WAL/segment offset",
        )
    }

    /// Unsupported wire/storage format version (`LIN-1565`).
    #[cold]
    pub fn unsupported_format(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::UnsupportedFormat,
            message,
            "upgrade `lin` binary or re-export data using a supported format version",
        )
    }

    /// Missing record/entity/artifact (`LIN-1566`).
    #[cold]
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::NotFound,
            message,
            "verify that the target row, key, or file exists before retrying",
        )
    }

    /// Missing collection/field schema definition (`LIN-1566`).
    #[cold]
    pub fn missing_schema(message: impl Into<String>) -> Self {
        Self::new(message)
            .with_code(ErrorCode::MissingSchema)
            .with_suggestion("declare the collection or field schema via `create` before use")
    }

    /// Explicit caller-initiated cancellation (`LIN-1567`).
    #[cold]
    pub fn canceled(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::Canceled,
            message,
            "operation was explicitly canceled; clear cancellation token before retrying",
        )
    }

    /// Execution timeout / deadline budget exhausted (`LIN-1567`).
    #[cold]
    pub fn deadline_exceeded(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::DeadlineExceeded,
            message,
            "increase query deadline/timeout or narrow scan cardinality with an index filter",
        )
    }

    /// Policy/quota resource ceiling reached (`LIN-1568`).
    #[cold]
    pub fn resource_limit(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::ResourceLimit,
            message,
            "raise configured `Quotas` limit or reduce query working-set size",
        )
    }

    /// Physical allocator / memory pool exhaustion (`LIN-1568`).
    #[cold]
    pub fn allocation_failure(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::AllocationFailure,
            message,
            "reduce batch size, enable disk spill, or free host/pool memory",
        )
    }

    /// Embedding model ID or vector dimension mismatch (`LIN-1580`).
    #[cold]
    pub fn incompatible_model(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::IncompatibleModel,
            message,
            "run `reembed <collection> to <model>/<dim>` or bind a matching `Embedder` via `Db::with_embedder`",
        )
    }

    /// Optional compile-time Cargo feature not compiled in (`LIN-1578`).
    #[cold]
    pub fn missing_feature(feature: impl AsRef<str>) -> Self {
        let feat = feature.as_ref();
        Self::runtime(format!(
            "missing feature `{feat}`: capability is not compiled into this build"
        ))
        .with_code(ErrorCode::MissingFeature)
        .with_suggestion(format!(
            "rebuild `lin` with Cargo `--features {feat}` enabled"
        ))
    }

    /// Prepared statement invalidated by catalog/schema evolution (`LIN-1579`).
    #[cold]
    pub fn stale_prepared(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::StalePrepared,
            message,
            "re-prepare the query handle via `db.prepare(...)` against the updated schema generation",
        )
    }

    /// Security capability or sandbox policy denial (`D45`).
    #[cold]
    pub fn permission_denied(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::PermissionDenied,
            message,
            "grant the required permission in `CapabilityPolicy` or adjust `allowed_import_root`",
        )
    }

    /// Storage or network I/O error.
    #[cold]
    pub fn io(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::Io,
            message,
            "inspect underlying I/O cause, disk space, and network reachability",
        )
    }

    /// GPU compute dispatch / adapter error (`LIN-1599`).
    #[cold]
    pub fn gpu(message: impl Into<String>) -> Self {
        Self::runtime_coded(
            ErrorCode::Gpu,
            message,
            "fall back to CPU execution or verify GPU adapter health",
        )
    }

    #[cold]
    fn details_mut(&mut self) -> &mut ErrorDetails {
        self.details
            .get_or_insert_with(|| Box::new(ErrorDetails::default()))
    }

    #[cold]
    pub fn with_code(mut self, code: ErrorCode) -> Self {
        self.details_mut().code = Some(code);
        self
    }

    #[cold]
    pub fn with_span(mut self, start: usize, end: usize) -> Self {
        let end = end.max(start);
        if self.pos.is_none() {
            self.pos = Some(start);
        }
        self.details_mut().span = Some(SourceSpan { start, end });
        self
    }

    #[cold]
    pub fn with_operation(mut self, op: impl Into<String>) -> Self {
        self.details_mut().operation = Some(bounded(op));
        self
    }

    #[cold]
    pub fn with_collection(mut self, col: impl Into<String>) -> Self {
        self.details_mut().collection = Some(bounded(col));
        self
    }

    #[cold]
    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.details_mut().field = Some(bounded(field));
        self
    }

    #[cold]
    pub fn with_constraint(mut self, name: impl Into<String>) -> Self {
        self.details_mut().constraint = Some(bounded(name));
        self
    }

    #[cold]
    pub fn with_wal_offset(mut self, offset: u64) -> Self {
        self.details_mut().wal_offset = Some(offset);
        self
    }

    #[cold]
    pub fn with_vec_offset(mut self, idx: usize) -> Self {
        self.details_mut().vec_offset = Some(idx);
        self
    }

    #[cold]
    pub fn with_suggestion(mut self, hint: impl Into<String>) -> Self {
        self.details_mut().suggestion = Some(bounded(hint));
        self
    }

    #[cold]
    pub fn with_source<E>(mut self, err: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        self.details_mut().source = Some(SharedErrorSource(Arc::new(err)));
        self
    }

    /// Stable error code, either explicitly attached or inferred without heap allocation.
    pub fn code(&self) -> ErrorCode {
        if let Some(code) = self.details.as_ref().and_then(|d| d.code) {
            return code;
        }
        infer_error_code(&self.message, self.pos, self.phase)
    }

    pub fn kind(&self) -> ErrorKind {
        match self.phase {
            Phase::Runtime => ErrorKind::Runtime,
            Phase::Compile => match self.code() {
                ErrorCode::ParseSyntax
                | ErrorCode::ParseUnterminatedStr
                | ErrorCode::ParseTrailing => ErrorKind::Parse,
                _ => ErrorKind::Check,
            },
        }
    }

    pub fn span(&self) -> Option<(usize, usize)> {
        if let Some(s) = self.details.as_ref().and_then(|d| d.span) {
            return Some((s.start, s.end));
        }
        self.pos.map(|p| (p, p))
    }

    pub fn operation(&self) -> Option<&str> {
        self.details.as_ref()?.operation.as_deref()
    }

    pub fn collection(&self) -> Option<&str> {
        self.details.as_ref()?.collection.as_deref()
    }

    pub fn field(&self) -> Option<&str> {
        self.details.as_ref()?.field.as_deref()
    }

    pub fn constraint(&self) -> Option<&str> {
        self.details.as_ref()?.constraint.as_deref()
    }

    pub fn wal_offset(&self) -> Option<u64> {
        self.details.as_ref()?.wal_offset
    }

    pub fn vec_offset(&self) -> Option<usize> {
        self.details.as_ref()?.vec_offset
    }

    pub fn suggestion(&self) -> Option<&str> {
        if let Some(s) = self.details.as_ref().and_then(|d| d.suggestion.as_deref()) {
            return Some(s);
        }
        self.code().default_suggestion()
    }

    /// True when the error never allocated boxed `ErrorDetails` (`LIN-1593`).
    #[inline]
    pub fn has_boxed_details(&self) -> bool {
        self.details.is_some()
    }

    /// Transient concurrency, timeout, stale-handle, I/O, or GPU failure eligible for retry (`LIN-1600`).
    #[inline]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.code(),
            ErrorCode::DeadlineExceeded
                | ErrorCode::CasMismatch
                | ErrorCode::StalePrepared
                | ErrorCode::Io
                | ErrorCode::Gpu
        )
    }

    /// Data or framing checksum/format corruption (`LIN-1565`).
    #[inline]
    pub fn is_corrupt(&self) -> bool {
        self.code() == ErrorCode::CorruptData
    }

    /// Operation terminated due to caller cancellation or deadline expiration (`LIN-1567`).
    #[inline]
    pub fn is_timeout_or_canceled(&self) -> bool {
        matches!(
            self.code(),
            ErrorCode::Canceled | ErrorCode::DeadlineExceeded
        )
    }

    /// GPU compute / adapter classification (`LIN-1599`).
    #[inline]
    pub fn is_gpu_error(&self) -> bool {
        self.code() == ErrorCode::Gpu
    }

    /// Collects the nested `std::error::Error::source` cause chain from immediate source to root (`LIN-1562`, `LIN-1569`).
    pub fn cause_chain(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut cur = std::error::Error::source(self);
        while let Some(err) = cur {
            out.push(err.to_string());
            cur = err.source();
        }
        out
    }

    /// Renders a complete structured diagnostic report suitable for CLI, logs, and async clients (`LIN-1597`, `LIN-1598`).
    pub fn diagnostic_report(&self) -> String {
        use fmt::Write as _;
        let mut buf = String::with_capacity(128);
        let _ = write!(buf, "[{}] {}", self.code().as_str(), self);
        if let Some(d) = self.details.as_deref() {
            if let Some(span) = d.span {
                let _ = write!(buf, " (span {}..{})", span.start, span.end);
            }
            if let Some(ref op) = d.operation {
                let _ = write!(buf, " [op={op}]");
            }
            if let Some(ref col) = d.collection {
                let _ = write!(buf, " [collection={col}]");
            }
            if let Some(ref field) = d.field {
                let _ = write!(buf, " [field={field}]");
            }
            if let Some(ref c) = d.constraint {
                let _ = write!(buf, " [constraint={c}]");
            }
            if let Some(off) = d.wal_offset {
                let _ = write!(buf, " [wal_offset={off}]");
            }
            if let Some(idx) = d.vec_offset {
                let _ = write!(buf, " [vec_offset={idx}]");
            }
        }
        if let Some(hint) = self.suggestion() {
            let _ = write!(buf, "\n  help: {hint}");
        }
        let mut cur = std::error::Error::source(self);
        while let Some(src) = cur {
            let _ = write!(buf, "\n  caused by: {src}");
            cur = src.source();
        }
        buf
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.phase {
            Phase::Compile => "compile error",
            Phase::Runtime => "runtime error",
        };
        match self.pos {
            Some(p) => write!(f, "{kind} at {}: {}", p, self.message),
            None => write!(f, "{kind}: {}", self.message),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.details.as_ref()?.source.as_deref()
    }
}

// ---------------------------------------------------------------------------
// Credential & Path Redaction (`LIN-1570`, `LIN-1572`)
// ---------------------------------------------------------------------------

const SECRET_KEYS: &[&str] = &[
    "aws_secret_access_key",
    "secret_access_key",
    "access_token",
    "refresh_token",
    "auth_token",
    "secret_key",
    "access_key",
    "api_key",
    "apikey",
    "password",
    "passwd",
    "secret",
    "token",
    "pwd",
];

fn is_key_boundary_before(bytes: &[u8], idx: usize) -> bool {
    if idx == 0 {
        return true;
    }
    let b = bytes[idx - 1];
    !b.is_ascii_alphanumeric() && b != b'_'
}

fn redact_url_userinfo(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(scheme_sep) = rest.find("://") {
        let prefix_end = scheme_sep + 3;
        out.push_str(&rest[..prefix_end]);
        rest = &rest[prefix_end..];

        let authority_end = rest
            .find(|c: char| matches!(c, '/' | '?' | '#' | ' ' | '\t' | '\n' | '\r' | '\'' | '"'))
            .unwrap_or(rest.len());
        let authority = &rest[..authority_end];
        if let Some(at_pos) = authority.rfind('@') {
            let userinfo = &authority[..at_pos];
            let hostpart = &authority[at_pos..]; // includes '@'
            if let Some(colon_pos) = userinfo.find(':') {
                let user = &userinfo[..colon_pos];
                out.push_str(user);
                out.push_str(":[REDACTED]");
            } else {
                out.push_str("[REDACTED]");
            }
            out.push_str(hostpart);
        } else {
            out.push_str(authority);
        }
        rest = &rest[authority_end..];
    }
    out.push_str(rest);
    out
}

fn redact_auth_schemes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let mut matched_scheme_len = 0usize;
        for scheme in ["Bearer ", "bearer ", "BEARER ", "Basic ", "basic ", "BASIC "] {
            if input[i..].starts_with(scheme) && is_key_boundary_before(bytes, i) {
                matched_scheme_len = scheme.len();
                break;
            }
        }
        if matched_scheme_len > 0 {
            out.push_str(&input[i..i + matched_scheme_len]);
            i += matched_scheme_len;
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            let tok_start = i;
            while i < bytes.len()
                && !matches!(
                    bytes[i],
                    b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\'' | b',' | b';' | b')' | b']'
                )
            {
                i += 1;
            }
            if i > tok_start {
                out.push_str("[REDACTED]");
            }
            continue;
        }
        let ch = input[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn redact_kv_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let mut matched_key: Option<&str> = None;
        if is_key_boundary_before(bytes, i) {
            for &key in SECRET_KEYS {
                if bytes.len() >= i + key.len()
                    && bytes[i..i + key.len()].eq_ignore_ascii_case(key.as_bytes())
                {
                    let mut after = i + key.len();
                    while after < bytes.len() && (bytes[after] == b' ' || bytes[after] == b'\t') {
                        after += 1;
                    }
                    if after < bytes.len() && (bytes[after] == b'=' || bytes[after] == b':') {
                        // Avoid matching `::` path/module separators.
                        if bytes[after] == b':'
                            && after + 1 < bytes.len()
                            && (bytes[after + 1] == b':' || bytes[after + 1] == b'/')
                        {
                            continue;
                        }
                        matched_key = Some(key);
                        break;
                    }
                }
            }
        }

        if let Some(key) = matched_key {
            let orig_key = &input[i..i + key.len()];
            out.push_str(orig_key);
            i += key.len();
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                out.push(bytes[i] as char);
                i += 1;
            }
            let sep = bytes[i] as char;
            out.push(sep);
            i += 1;
            while i < bytes.len() && (bytes[i] == b' ' || bytes[i] == b'\t') {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'\'' || bytes[i] == b'"') {
                let quote = bytes[i];
                out.push(quote as char);
                i += 1;
                while i < bytes.len() && bytes[i] != quote {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                    } else {
                        let ch = input[i..].chars().next().unwrap();
                        i += ch.len_utf8();
                    }
                }
                out.push_str("[REDACTED]");
                if i < bytes.len() && bytes[i] == quote {
                    out.push(quote as char);
                    i += 1;
                }
            } else {
                let val_start = i;
                while i < bytes.len()
                    && !matches!(
                        bytes[i],
                        b' ' | b'\t' | b'\n' | b'\r' | b'&' | b';' | b',' | b')' | b'\'' | b'"'
                    )
                {
                    let ch = input[i..].chars().next().unwrap();
                    i += ch.len_utf8();
                }
                if i > val_start {
                    out.push_str("[REDACTED]");
                }
            }
            continue;
        }

        let ch = input[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// Redacts credentials (`user:password@host`, `Bearer ...`, `token=...`, `password=...`, etc.)
/// across SQL statements, HTTP headers, and S3/database connection URLs (`LIN-1572`).
pub fn redact_credentials(input: &str) -> String {
    let step1 = redact_url_userinfo(input);
    let step2 = redact_auth_schemes(&step1);
    redact_kv_secrets(&step2)
}

fn is_sensitive_dir_segment(seg: &str) -> bool {
    if seg.starts_with('.') && seg != "." && seg != ".." {
        return true;
    }
    matches!(
        seg.to_ascii_lowercase().as_str(),
        "users"
            | "home"
            | "root"
            | "private"
            | "secrets"
            | "secret"
            | "credentials"
            | "creds"
            | "keys"
            | "tokens"
            | "ssh"
            | "aws"
            | "gnupg"
    )
}

/// Redacts filesystem paths so home/user/secret directory names are never leaked,
/// keeping only the basename and a safe parent directory hint (`LIN-1570`).
pub fn redact_sensitive_path(path: &Path) -> String {
    let mut normal_segments: Vec<String> = Vec::new();
    let mut had_root_or_prefix = false;

    for comp in path.components() {
        match comp {
            Component::Prefix(_) | Component::RootDir => {
                had_root_or_prefix = true;
            }
            Component::CurDir => {}
            Component::ParentDir => {
                normal_segments.push("..".to_string());
            }
            Component::Normal(os) => {
                let raw = os.to_string_lossy();
                for sub in raw.split('\\').filter(|s| !s.is_empty()) {
                    normal_segments.push(sub.to_string());
                }
            }
        }
    }

    if normal_segments.is_empty() {
        return "<redacted-path>".to_string();
    }

    let basename = normal_segments.last().unwrap();
    if normal_segments.len() == 1 {
        return if had_root_or_prefix {
            format!(".../{basename}")
        } else {
            basename.clone()
        };
    }

    let parent_idx = normal_segments.len() - 2;
    let parent = &normal_segments[parent_idx];

    // Check if `parent` is directly under `/Users/<username>` or `/home/<username>` (making `parent` the OS username!).
    let parent_is_os_username = parent_idx >= 1 && {
        let grandparent = normal_segments[parent_idx - 1].to_ascii_lowercase();
        grandparent == "users" || grandparent == "home"
    };

    if parent == ".." || parent_is_os_username || is_sensitive_dir_segment(parent) {
        format!(".../[REDACTED]/{basename}")
    } else if normal_segments.len() > 2 || had_root_or_prefix {
        format!(".../{parent}/{basename}")
    } else {
        format!("{parent}/{basename}")
    }
}

// ---------------------------------------------------------------------------
// Structured Warning Channel (`LIN-1585`..`LIN-1588`, `LIN-1596`)
// ---------------------------------------------------------------------------

/// Severity levels for non-fatal diagnostics (`LIN-1586`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum WarningSeverity {
    Info,
    Low,
    Medium,
    High,
}

/// Scoped suppression rules for [`WarningChannel`] (`LIN-1587`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WarningSuppressionScope {
    Global,
    Code(String),
    SeverityBelow(WarningSeverity),
    Collection(String),
    Operation(String),
}

impl WarningSuppressionScope {
    pub fn matches(&self, w: &DiagnosticWarning) -> bool {
        match self {
            WarningSuppressionScope::Global => true,
            WarningSuppressionScope::Code(c) => w.code == *c,
            WarningSuppressionScope::SeverityBelow(min) => w.severity < *min,
            WarningSuppressionScope::Collection(col) => w.collection.as_deref() == Some(col.as_str()),
            WarningSuppressionScope::Operation(op) => w.operation.as_deref() == Some(op.as_str()),
        }
    }
}

/// Structured non-fatal diagnostic warning (`LIN-1585`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticWarning {
    pub code: String,
    pub severity: WarningSeverity,
    pub message: String,
    pub collection: Option<String>,
    pub operation: Option<String>,
}

impl DiagnosticWarning {
    pub fn new(
        code: impl Into<String>,
        severity: WarningSeverity,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            message: bounded(message),
            collection: None,
            operation: None,
        }
    }

    pub fn with_collection(mut self, collection: impl Into<String>) -> Self {
        self.collection = Some(collection.into());
        self
    }

    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }
}

/// Bounded warning aggregation channel with severity filtering and suppression scopes (`LIN-1585`..`LIN-1588`).
#[derive(Debug, Clone)]
pub struct WarningChannel {
    max_warnings: usize,
    min_severity: WarningSeverity,
    suppressions: Vec<WarningSuppressionScope>,
    warnings: Vec<DiagnosticWarning>,
    suppressed_by_policy: u64,
    dropped_over_budget: u64,
}

impl Default for WarningChannel {
    fn default() -> Self {
        Self::new(64)
    }
}

impl WarningChannel {
    pub fn new(max_warnings: usize) -> Self {
        Self {
            max_warnings,
            min_severity: WarningSeverity::Info,
            suppressions: Vec::new(),
            warnings: Vec::with_capacity(max_warnings.min(64)),
            suppressed_by_policy: 0,
            dropped_over_budget: 0,
        }
    }

    pub fn with_min_severity(mut self, min_severity: WarningSeverity) -> Self {
        self.min_severity = min_severity;
        self
    }

    pub fn set_min_severity(&mut self, min_severity: WarningSeverity) {
        self.min_severity = min_severity;
    }

    pub fn suppress(&mut self, scope: WarningSuppressionScope) {
        if !self.suppressions.contains(&scope) {
            self.suppressions.push(scope);
        }
    }

    pub fn clear_suppressions(&mut self) {
        self.suppressions.clear();
    }

    /// Emits a warning if not suppressed and within `max_warnings` aggregation budget.
    /// Returns `true` if retained, `false` if suppressed or dropped over budget.
    pub fn emit(&mut self, warning: DiagnosticWarning) -> bool {
        if warning.severity < self.min_severity
            || self.suppressions.iter().any(|s| s.matches(&warning))
        {
            self.suppressed_by_policy = self.suppressed_by_policy.saturating_add(1);
            return false;
        }
        if self.warnings.len() >= self.max_warnings {
            self.dropped_over_budget = self.dropped_over_budget.saturating_add(1);
            return false;
        }
        self.warnings.push(warning);
        true
    }

    pub fn max_warnings(&self) -> usize {
        self.max_warnings
    }

    pub fn warnings(&self) -> &[DiagnosticWarning] {
        &self.warnings
    }

    /// Total warnings omitted (policy-suppressed + dropped due to `max_warnings` budget).
    pub fn suppressed_count(&self) -> u64 {
        self.suppressed_by_policy
            .saturating_add(self.dropped_over_budget)
    }

    pub fn suppressed_by_policy_count(&self) -> u64 {
        self.suppressed_by_policy
    }

    pub fn dropped_over_budget_count(&self) -> u64 {
        self.dropped_over_budget
    }

    pub fn drain(&mut self) -> Vec<DiagnosticWarning> {
        std::mem::take(&mut self.warnings)
    }

    pub fn clear(&mut self) {
        self.warnings.clear();
        self.suppressed_by_policy = 0;
        self.dropped_over_budget = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_messages_are_truncated_on_a_char_boundary() {
        let short = Error::new("short");
        assert_eq!(short.message, "short");
        assert!(!short.has_boxed_details());
        let long = Error::runtime("é".repeat(5000));
        assert!(long.message.len() < MAX_MESSAGE_BYTES + 64);
        assert!(long.message.ends_with("bytes truncated]"));
        assert!(long.message.starts_with("éé"));
    }

    #[test]
    fn error_stack_footprint_remains_compact() {
        assert!(std::mem::size_of::<Error>() <= 56);
    }
}
