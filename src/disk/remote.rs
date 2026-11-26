//! Object-storage cold tier (`ColdObjectStore`, local FS & AWS SigV4 S3 clients,
//! `.lseg` CRC32C segment bundles, lazy hydration, and whole-table LRU disk eviction).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};

use super::cache::{HotCacheConfig, HotTierCache};
use super::segment::DiskSegment;
use super::{
    DISK_FORMAT_VERSION, DiskEngine, DiskManifest, MANIFEST_FILE, SEGMENTS_DIR, SegmentMeta,
    SegmentTombstones, TOMBSTONES_FILE,
};
use crate::batch::{read_slice, read_u8, read_u32_le, read_u64_le};
use crate::crc::hardware_crc32c;
use crate::error::Error;
use crate::persist::io_err;
use crate::ship::{PullOpts, TlsClient, connect_net_stream};

const LSEG_MAGIC: &[u8; 5] = b"LSEG\x01";
const LRMF_MAGIC: &[u8; 5] = b"LRMF\x01";
const FRAME_CRC_TAG: &[u8; 4] = b"LCRC";
const LRMF_VERSION: u32 = 1;

static TMP_SEQ: AtomicU64 = AtomicU64::new(1);

#[inline]
fn read_u16_le(cur: &mut &[u8]) -> Result<u16, Error> {
    let s = read_slice(cur, 2)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObjectStoreCounters {
    pub puts: u64,
    pub gets: u64,
    pub lists: u64,
    pub deletes: u64,
    pub bytes_uploaded: u64,
    pub bytes_downloaded: u64,
}

pub trait ColdObjectStore: Send + Sync + 'static {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Error>;
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error>;
    fn list(&self, prefix: &str) -> Result<Vec<String>, Error>;
    fn delete(&self, key: &str) -> Result<(), Error>;
}

pub(crate) fn normalize_prefix(prefix: &str) -> Result<String, Error> {
    let trimmed = prefix.trim_matches('/');
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    validate_key(trimmed)?;
    Ok(trimmed.to_string())
}

fn validate_key(key: &str) -> Result<(), Error> {
    if key.is_empty()
        || key.starts_with('/')
        || key.ends_with('/')
        || key.contains('\\')
        || key.contains('\0')
    {
        return Err(Error::runtime(format!("invalid object store key {key:?}")));
    }
    for seg in key.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(Error::runtime(format!("unsafe object store key {key:?}")));
        }
    }
    Ok(())
}

fn validate_prefix(prefix: &str) -> Result<(), Error> {
    if prefix.is_empty() {
        return Ok(());
    }
    let check = prefix.strip_suffix('/').unwrap_or(prefix);
    validate_key(check)
}

fn object_key(prefix: &str, rel: &str) -> Result<String, Error> {
    validate_key(rel)?;
    if prefix.is_empty() {
        Ok(rel.to_string())
    } else {
        let full = format!("{prefix}/{rel}");
        validate_key(&full)?;
        Ok(full)
    }
}

/// In-memory [`ColdObjectStore`] with atomic operation counters for fast deterministic tests.
#[derive(Default)]
pub struct MemoryObjectStore {
    objects: RwLock<BTreeMap<String, Arc<[u8]>>>,
    puts: AtomicU64,
    gets: AtomicU64,
    lists: AtomicU64,
    deletes: AtomicU64,
    bytes_uploaded: AtomicU64,
    bytes_downloaded: AtomicU64,
}

impl MemoryObjectStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn counters(&self) -> ObjectStoreCounters {
        ObjectStoreCounters {
            puts: self.puts.load(Ordering::Relaxed),
            gets: self.gets.load(Ordering::Relaxed),
            lists: self.lists.load(Ordering::Relaxed),
            deletes: self.deletes.load(Ordering::Relaxed),
            bytes_uploaded: self.bytes_uploaded.load(Ordering::Relaxed),
            bytes_downloaded: self.bytes_downloaded.load(Ordering::Relaxed),
        }
    }

    /// Flip bits inside an existing object to test integrity verification.
    pub fn corrupt_byte(&self, key: &str, offset: usize, xor_mask: u8) -> Result<(), Error> {
        let mut map = self
            .objects
            .write()
            .map_err(|_| Error::runtime("memory object store lock poisoned"))?;
        let cur = map
            .get(key)
            .ok_or_else(|| Error::runtime(format!("object {key:?} not found")))?;
        let mut owned = cur.to_vec();
        if offset >= owned.len() {
            return Err(Error::runtime("corrupt_byte offset out of range"));
        }
        owned[offset] ^= xor_mask;
        map.insert(key.to_string(), Arc::from(owned));
        Ok(())
    }
}

impl ColdObjectStore for MemoryObjectStore {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        validate_key(key)?;
        let mut map = self
            .objects
            .write()
            .map_err(|_| Error::runtime("memory object store lock poisoned"))?;
        map.insert(key.to_string(), Arc::from(bytes));
        self.puts.fetch_add(1, Ordering::Relaxed);
        self.bytes_uploaded
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        validate_key(key)?;
        self.gets.fetch_add(1, Ordering::Relaxed);
        let map = self
            .objects
            .read()
            .map_err(|_| Error::runtime("memory object store lock poisoned"))?;
        let Some(bytes) = map.get(key) else {
            return Ok(None);
        };
        let out = bytes.to_vec();
        self.bytes_downloaded
            .fetch_add(out.len() as u64, Ordering::Relaxed);
        Ok(Some(out))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, Error> {
        validate_prefix(prefix)?;
        self.lists.fetch_add(1, Ordering::Relaxed);
        let map = self
            .objects
            .read()
            .map_err(|_| Error::runtime("memory object store lock poisoned"))?;
        Ok(map
            .range(prefix.to_string()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, _)| k.clone())
            .collect())
    }

    fn delete(&self, key: &str) -> Result<(), Error> {
        validate_key(key)?;
        self.deletes.fetch_add(1, Ordering::Relaxed);
        let mut map = self
            .objects
            .write()
            .map_err(|_| Error::runtime("memory object store lock poisoned"))?;
        map.remove(key);
        Ok(())
    }
}

/// Directory-backed local/NFS [`ColdObjectStore`] with path-traversal guards and atomic writes.
pub struct FsObjectStore {
    root: PathBuf,
}

impl FsObjectStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, Error> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root).map_err(io_err)?;
        let canon = fs::canonicalize(&root).map_err(io_err)?;
        Ok(Self { root: canon })
    }

    fn key_to_path(&self, key: &str) -> Result<PathBuf, Error> {
        validate_key(key)?;
        let mut p = self.root.clone();
        for part in key.split('/') {
            let mut comps = Path::new(part).components();
            match (comps.next(), comps.next()) {
                (Some(Component::Normal(_)), None) => p.push(part),
                _ => return Err(Error::runtime(format!("unsafe path segment {part:?}"))),
            }
        }
        Ok(p)
    }

    fn ensure_within_root(&self, ancestor: &Path) -> Result<(), Error> {
        let canon = fs::canonicalize(ancestor).map_err(io_err)?;
        if !canon.starts_with(&self.root) {
            return Err(Error::runtime("object store path escapes root directory"));
        }
        Ok(())
    }
}

impl ColdObjectStore for FsObjectStore {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        let dst = self.key_to_path(key)?;
        let parent = dst
            .parent()
            .ok_or_else(|| Error::runtime("missing object parent path"))?;
        fs::create_dir_all(parent).map_err(io_err)?;
        self.ensure_within_root(parent)?;
        let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(".tmp_{}_{}", std::process::id(), seq));
        {
            let mut f = File::create(&tmp).map_err(io_err)?;
            f.write_all(bytes).map_err(io_err)?;
            f.sync_all().map_err(io_err)?;
        }
        fs::rename(&tmp, &dst).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            io_err(e)
        })
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        let p = self.key_to_path(key)?;
        match fs::read(&p) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io_err(e)),
        }
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, Error> {
        validate_prefix(prefix)?;
        let mut out = Vec::new();
        collect_fs_keys(&self.root, &self.root, prefix, &mut out)?;
        out.sort();
        Ok(out)
    }

    fn delete(&self, key: &str) -> Result<(), Error> {
        let p = self.key_to_path(key)?;
        match fs::remove_file(&p) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io_err(e)),
        }
    }
}

fn collect_fs_keys(
    root: &Path,
    dir: &Path,
    prefix: &str,
    out: &mut Vec<String>,
) -> Result<(), Error> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io_err(e)),
    };
    for ent in entries {
        let ent = ent.map_err(io_err)?;
        let name = ent.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        if name_str.starts_with(".tmp_") {
            continue;
        }
        let path = ent.path();
        let meta = fs::symlink_metadata(&path).map_err(io_err)?;
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            collect_fs_keys(root, &path, prefix, out)?;
        } else if meta.is_file() {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| Error::runtime(e.to_string()))?;
            let key = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if key.starts_with(prefix) {
                out.push(key);
            }
        }
    }
    Ok(())
}

// ============================================================================
// Pure-Rust FIPS 180-4 SHA-256 + HMAC-SHA256 + AWS SigV4 HTTP/1.1 S3 Client
// ============================================================================

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

struct Sha256 {
    state: [u32; 8],
    buf: [u8; 64],
    buf_len: usize,
    total_bytes: u64,
}

impl Sha256 {
    #[inline]
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            buf: [0u8; 64],
            buf_len: 0,
            total_bytes: 0,
        }
    }

    #[inline]
    fn compress_block(state: &mut [u32; 8], block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            let j = i * 4;
            w[i] = u32::from_be_bytes([block[j], block[j + 1], block[j + 2], block[j + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        let next = [a, b, c, d, e, f, g, h];
        for (dst, add) in state.iter_mut().zip(next) {
            *dst = dst.wrapping_add(add);
        }
    }

    fn update(&mut self, mut data: &[u8]) {
        self.total_bytes = self.total_bytes.wrapping_add(data.len() as u64);
        if self.buf_len > 0 {
            let need = 64 - self.buf_len;
            if data.len() < need {
                self.buf[self.buf_len..self.buf_len + data.len()].copy_from_slice(data);
                self.buf_len += data.len();
                return;
            }
            self.buf[self.buf_len..64].copy_from_slice(&data[..need]);
            Self::compress_block(&mut self.state, &self.buf);
            self.buf_len = 0;
            data = &data[need..];
        }
        let mut chunks = data.chunks_exact(64);
        for chunk in &mut chunks {
            let block: &[u8; 64] = chunk.try_into().unwrap();
            Self::compress_block(&mut self.state, block);
        }
        let rem = chunks.remainder();
        if !rem.is_empty() {
            self.buf[..rem.len()].copy_from_slice(rem);
            self.buf_len = rem.len();
        }
    }

    fn finalize(mut self) -> [u8; 32] {
        let bit_len = self.total_bytes.wrapping_mul(8);
        self.buf[self.buf_len] = 0x80;
        self.buf_len += 1;
        if self.buf_len > 56 {
            self.buf[self.buf_len..64].fill(0);
            Self::compress_block(&mut self.state, &self.buf);
            self.buf[..56].fill(0);
        } else {
            self.buf[self.buf_len..56].fill(0);
        }
        self.buf[56..64].copy_from_slice(&bit_len.to_be_bytes());
        Self::compress_block(&mut self.state, &self.buf);

        let mut out = [0u8; 32];
        for (i, word) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize()
}

fn hmac_sha256_key_parts(key_a: &[u8], key_b: &[u8], data: &[u8]) -> [u8; 32] {
    let mut key_block = [0u8; 64];
    let total_klen = key_a.len() + key_b.len();
    if total_klen > 64 {
        let mut kh = Sha256::new();
        kh.update(key_a);
        kh.update(key_b);
        key_block[..32].copy_from_slice(&kh.finalize());
    } else {
        key_block[..key_a.len()].copy_from_slice(key_a);
        key_block[key_a.len()..total_klen].copy_from_slice(key_b);
    }
    let mut ipad = [0u8; 64];
    let mut opad = [0u8; 64];
    for i in 0..64 {
        ipad[i] = key_block[i] ^ 0x36;
        opad[i] = key_block[i] ^ 0x5c;
    }
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(&inner_hash);
    outer.finalize()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    hmac_sha256_key_parts(key, &[], data)
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

/// Format Unix epoch seconds into `(YYYYMMDD, YYYYMMDDTHHMMSSZ)` using Howard Hinnant's civil calendar algorithm.
fn format_amz_utc(unix_secs: u64) -> (String, String) {
    let days = (unix_secs / 86_400) as i64;
    let secs_of_day = unix_secs % 86_400;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;
    let date = format!("{year:04}{m:02}{d:02}");
    let datetime = format!("{date}T{hh:02}{mm:02}{ss:02}Z");
    (date, datetime)
}

fn uri_encode(input: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(input.len());
    for &b in input.as_bytes() {
        let keep = b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'_' | b'.' | b'~')
            || (!encode_slash && b == b'/');
        if keep {
            out.push(b as char);
        } else {
            out.push('%');
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            out.push(HEX[(b >> 4) as usize] as char);
            out.push(HEX[(b & 0x0f) as usize] as char);
        }
    }
    out
}

fn sigv4_signing_key(secret: &str, date: &str, region: &str, service: &str) -> [u8; 32] {
    let k_date = hmac_sha256_key_parts(b"AWS4", secret.as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

#[allow(clippy::too_many_arguments)]
fn build_sigv4_authorization(
    method: &str,
    canonical_uri: &str,
    query_params: &[(&str, &str)],
    host_header: &str,
    payload_sha256_hex: &str,
    amz_date: &str,
    date_stamp: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
    session_token: Option<&str>,
) -> (String, String, Vec<(String, String)>) {
    let mut encoded_query: Vec<(String, String)> = query_params
        .iter()
        .map(|(k, v)| (uri_encode(k, true), uri_encode(v, true)))
        .collect();
    encoded_query.sort();
    let canonical_query = encoded_query
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    let mut hdrs: Vec<(String, String)> = vec![
        ("host".to_string(), host_header.trim().to_string()),
        (
            "x-amz-content-sha256".to_string(),
            payload_sha256_hex.to_string(),
        ),
        ("x-amz-date".to_string(), amz_date.to_string()),
    ];
    if let Some(tok) = session_token {
        hdrs.push(("x-amz-security-token".to_string(), tok.trim().to_string()));
    }
    hdrs.sort_by(|a, b| a.0.cmp(&b.0));

    let mut canonical_headers = String::new();
    let mut signed_headers = String::new();
    for (i, (k, v)) in hdrs.iter().enumerate() {
        canonical_headers.push_str(k);
        canonical_headers.push(':');
        canonical_headers.push_str(v);
        canonical_headers.push('\n');
        if i > 0 {
            signed_headers.push(';');
        }
        signed_headers.push_str(k);
    }

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_sha256_hex}"
    );
    let scope = format!("{date_stamp}/{region}/s3/aws4_request");
    let canonical_hash = hex_lower(&sha256(canonical_request.as_bytes()));
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{canonical_hash}");
    let signing_key = sigv4_signing_key(secret_key, date_stamp, region, "s3");
    let signature = hex_lower(&hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );
    (canonical_query, auth, hdrs)
}

#[derive(Clone)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub prefix: String,
    pub access_key: String,
    pub secret_key: String,
    pub session_token: Option<String>,
    pub path_style: bool,
    pub ca_pem: Option<Vec<u8>>,
    pub timeout: Duration,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:9000".to_string(),
            region: "us-east-1".to_string(),
            bucket: String::new(),
            prefix: String::new(),
            access_key: String::new(),
            secret_key: String::new(),
            session_token: None,
            path_style: true,
            ca_pem: None,
            timeout: Duration::from_secs(30),
        }
    }
}

impl fmt::Debug for S3Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Config")
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("bucket", &self.bucket)
            .field("prefix", &self.prefix)
            .field("access_key", &self.access_key)
            .field("secret_key", &"<redacted>")
            .field(
                "session_token",
                &self.session_token.as_ref().map(|_| "<redacted>"),
            )
            .field("path_style", &self.path_style)
            .field("has_ca_pem", &self.ca_pem.is_some())
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// Zero-dependency HTTP/1.1 + AWS SigV4 S3-compatible [`ColdObjectStore`].
pub struct S3HttpObjectStore {
    cfg: S3Config,
    is_https: bool,
    host: String,
    port: u16,
    base_path: String,
}

impl S3HttpObjectStore {
    pub fn new(mut cfg: S3Config) -> Result<Self, Error> {
        if cfg.bucket.trim().is_empty() {
            return Err(Error::runtime("S3Config bucket cannot be empty"));
        }
        if cfg.region.trim().is_empty() {
            cfg.region = "us-east-1".to_string();
        }
        cfg.prefix = normalize_prefix(&cfg.prefix)?;

        let trimmed = cfg.endpoint.trim();
        let (is_https, rest) = if let Some(r) = trimmed.strip_prefix("https://") {
            (true, r)
        } else if let Some(r) = trimmed.strip_prefix("http://") {
            (false, r)
        } else {
            return Err(Error::runtime(format!(
                "S3 endpoint must start with http:// or https://, got {trimmed:?}"
            )));
        };
        if is_https && cfg.ca_pem.is_none() {
            return Err(Error::runtime(
                "https:// S3 endpoint requires S3Config::ca_pem with root CA PEM bytes",
            ));
        }
        let (authority, path_tail) = match rest.find('/') {
            Some(pos) => (&rest[..pos], rest[pos..].trim_end_matches('/')),
            None => (rest, ""),
        };
        let default_port = if is_https { 443 } else { 80 };
        let (host, port) = if let Some(stripped) = authority.strip_prefix('[') {
            let end = stripped
                .find(']')
                .ok_or_else(|| Error::runtime("invalid IPv6 host in S3 endpoint"))?;
            let h = &stripped[..end];
            let rem = &stripped[end + 1..];
            let p = if let Some(ps) = rem.strip_prefix(':') {
                ps.parse::<u16>()
                    .map_err(|_| Error::runtime("invalid port in S3 endpoint"))?
            } else {
                default_port
            };
            (h.to_string(), p)
        } else if let Some(colon) = authority.rfind(':') {
            let h = &authority[..colon];
            let p = authority[colon + 1..]
                .parse::<u16>()
                .map_err(|_| Error::runtime("invalid port in S3 endpoint"))?;
            (h.to_string(), p)
        } else {
            (authority.to_string(), default_port)
        };
        if host.is_empty() {
            return Err(Error::runtime("empty host in S3 endpoint"));
        }
        let base_path = path_tail.to_string();
        Ok(Self {
            cfg,
            is_https,
            host,
            port,
            base_path,
        })
    }

    fn full_key(&self, key: &str) -> Result<String, Error> {
        object_key(&self.cfg.prefix, key)
    }

    fn strip_prefix<'a>(&self, full: &'a str) -> Option<&'a str> {
        if self.cfg.prefix.is_empty() {
            Some(full)
        } else {
            let pfx = format!("{}/", self.cfg.prefix);
            full.strip_prefix(&pfx)
        }
    }

    fn target_host_and_uri(&self, full_key: Option<&str>) -> (String, String) {
        let host_port = if (self.is_https && self.port == 443) || (!self.is_https && self.port == 80)
        {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        };
        if self.cfg.path_style {
            let uri = match full_key {
                Some(k) => format!(
                    "{}/{}/{}",
                    self.base_path,
                    uri_encode(&self.cfg.bucket, true),
                    uri_encode(k, false)
                ),
                None => format!("{}/{}", self.base_path, uri_encode(&self.cfg.bucket, true)),
            };
            (host_port, uri)
        } else {
            let vhost = format!("{}.{host_port}", self.cfg.bucket);
            let uri = match full_key {
                Some(k) => format!("{}/{}", self.base_path, uri_encode(k, false)),
                None => {
                    if self.base_path.is_empty() {
                        "/".to_string()
                    } else {
                        self.base_path.clone()
                    }
                }
            };
            (vhost, uri)
        }
    }

    fn do_http(
        &self,
        method: &str,
        full_key: Option<&str>,
        query: &[(&str, &str)],
        body: &[u8],
    ) -> Result<(u16, Vec<u8>), Error> {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let (date_stamp, amz_date) = format_amz_utc(now_secs);
        let payload_hash = hex_lower(&sha256(body));
        let (host_header, canonical_uri) = self.target_host_and_uri(full_key);
        let (canonical_query, auth, signed_hdrs) = build_sigv4_authorization(
            method,
            &canonical_uri,
            query,
            &host_header,
            &payload_hash,
            &amz_date,
            &date_stamp,
            &self.cfg.region,
            &self.cfg.access_key,
            &self.cfg.secret_key,
            self.cfg.session_token.as_deref(),
        );

        let connect_addr = format!("{}:{}", self.host, self.port);
        let tls = if self.is_https {
            TlsClient::CaPem(self.cfg.ca_pem.clone().unwrap_or_default())
        } else {
            TlsClient::Off
        };
        let pull_opts = PullOpts {
            token: None,
            tls,
            server_name: if self.is_https {
                Some(if self.cfg.path_style {
                    self.host.clone()
                } else {
                    format!("{}.{}", self.cfg.bucket, self.host)
                })
            } else {
                None
            },
            ..Default::default()
        };
        let mut stream = connect_net_stream(&connect_addr, &pull_opts, self.cfg.timeout)?;

        let request_target = if canonical_query.is_empty() {
            canonical_uri
        } else {
            format!("{canonical_uri}?{canonical_query}")
        };
        let mut req_head = format!("{method} {request_target} HTTP/1.1\r\n");
        for (k, v) in &signed_hdrs {
            req_head.push_str(&format!("{k}: {v}\r\n"));
        }
        req_head.push_str(&format!("Authorization: {auth}\r\n"));
        req_head.push_str(&format!("Content-Length: {}\r\n", body.len()));
        req_head.push_str("Connection: close\r\n\r\n");

        stream.write_all(req_head.as_bytes()).map_err(io_err)?;
        if !body.is_empty() {
            stream.write_all(body).map_err(io_err)?;
        }
        stream.flush().map_err(io_err)?;

        read_http_response(&mut stream, method == "HEAD")
    }
}

impl ColdObjectStore for S3HttpObjectStore {
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), Error> {
        let full = self.full_key(key)?;
        let (status, resp_body) = self.do_http("PUT", Some(&full), &[], bytes)?;
        if !(200..300).contains(&status) {
            return Err(Error::runtime(format!(
                "S3 PUT {full} failed with HTTP {status}: {}",
                String::from_utf8_lossy(&resp_body)
            )));
        }
        Ok(())
    }

    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        let full = self.full_key(key)?;
        let (status, resp_body) = self.do_http("GET", Some(&full), &[], &[])?;
        if status == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            return Err(Error::runtime(format!(
                "S3 GET {full} failed with HTTP {status}: {}",
                String::from_utf8_lossy(&resp_body)
            )));
        }
        Ok(Some(resp_body))
    }

    fn list(&self, prefix: &str) -> Result<Vec<String>, Error> {
        validate_prefix(prefix)?;
        let full_prefix = if self.cfg.prefix.is_empty() {
            prefix.to_string()
        } else if prefix.is_empty() {
            format!("{}/", self.cfg.prefix)
        } else {
            format!("{}/{}", self.cfg.prefix, prefix)
        };
        let mut keys = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query: Vec<(&str, &str)> =
                vec![("list-type", "2"), ("prefix", full_prefix.as_str())];
            if let Some(ref tok) = token {
                query.push(("continuation-token", tok.as_str()));
            }
            let (status, body) = self.do_http("GET", None, &query, &[])?;
            if !(200..300).contains(&status) {
                return Err(Error::runtime(format!(
                    "S3 LIST failed with HTTP {status}: {}",
                    String::from_utf8_lossy(&body)
                )));
            }
            let xml = std::str::from_utf8(&body)
                .map_err(|e| Error::runtime(format!("invalid UTF-8 in S3 LIST response: {e}")))?;
            let page = parse_list_v2_xml(xml);
            for raw_key in page.keys {
                if let Some(rel) = self.strip_prefix(&raw_key)
                    && rel.starts_with(prefix)
                {
                    keys.push(rel.to_string());
                }
            }
            if page.is_truncated {
                token = page.next_token;
                if token.is_none() {
                    break;
                }
            } else {
                break;
            }
        }
        keys.sort();
        Ok(keys)
    }

    fn delete(&self, key: &str) -> Result<(), Error> {
        let full = self.full_key(key)?;
        let (status, resp_body) = self.do_http("DELETE", Some(&full), &[], &[])?;
        if status == 404 || (200..300).contains(&status) {
            return Ok(());
        }
        Err(Error::runtime(format!(
            "S3 DELETE {full} failed with HTTP {status}: {}",
            String::from_utf8_lossy(&resp_body)
        )))
    }
}

fn read_http_response<R: Read>(reader: &mut R, is_head: bool) -> Result<(u16, Vec<u8>), Error> {
    let mut raw = Vec::with_capacity(4096);
    let mut buf = [0u8; 4096];
    let head_end;
    loop {
        if let Some(pos) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            head_end = pos;
            break;
        }
        if raw.len() > 256 * 1024 {
            return Err(Error::runtime("S3 HTTP response header exceeded 256 KiB"));
        }
        let n = reader.read(&mut buf).map_err(io_err)?;
        if n == 0 {
            return Err(Error::runtime("unexpected EOF reading S3 HTTP headers"));
        }
        raw.extend_from_slice(&buf[..n]);
    }
    let header_str = std::str::from_utf8(&raw[..head_end])
        .map_err(|_| Error::runtime("invalid UTF-8 in HTTP headers"))?;
    let mut lines = header_str.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| Error::runtime("missing HTTP status line"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| Error::runtime(format!("malformed HTTP status line {status_line:?}")))?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            let kl = k.trim().to_ascii_lowercase();
            let vl = v.trim();
            if kl == "content-length" {
                content_length = Some(
                    vl.parse::<usize>()
                        .map_err(|_| Error::runtime("invalid Content-Length header"))?,
                );
            } else if kl == "transfer-encoding" && vl.to_ascii_lowercase().contains("chunked") {
                chunked = true;
            }
        }
    }

    if is_head || status == 204 || status == 304 {
        return Ok((status, Vec::new()));
    }

    let mut leftover = raw[head_end + 4..].to_vec();
    if chunked {
        reader.read_to_end(&mut leftover).map_err(io_err)?;
        let decoded = decode_http_chunked(&leftover)?;
        return Ok((status, decoded));
    }
    if let Some(expected) = content_length {
        if leftover.len() < expected {
            // Read straight into spare capacity: zero-filling the whole remainder before
            // every short read made large bodies quadratic.
            let need = expected - leftover.len();
            leftover.reserve(need.min(1 << 20));
            reader
                .by_ref()
                .take(need as u64)
                .read_to_end(&mut leftover)
                .map_err(io_err)?;
            if leftover.len() < expected {
                return Err(Error::runtime(format!(
                    "truncated HTTP response body: got {} of {} bytes",
                    leftover.len(),
                    expected
                )));
            }
        }
        leftover.truncate(expected);
        return Ok((status, leftover));
    }
    reader.read_to_end(&mut leftover).map_err(io_err)?;
    Ok((status, leftover))
}

fn decode_http_chunked(data: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    loop {
        let Some(rel_crlf) = data[pos..].windows(2).position(|w| w == b"\r\n") else {
            return Err(Error::runtime("malformed HTTP chunked size line"));
        };
        let size_line = std::str::from_utf8(&data[pos..pos + rel_crlf])
            .map_err(|_| Error::runtime("non-UTF8 chunk size"))?;
        let hex_part = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(hex_part, 16)
            .map_err(|_| Error::runtime(format!("invalid HTTP chunk size {hex_part:?}")))?;
        pos += rel_crlf + 2;
        if size == 0 {
            break;
        }
        let end = pos
            .checked_add(size)
            .filter(|&e| e + 2 <= data.len())
            .ok_or_else(|| Error::runtime("truncated HTTP chunk body"))?;
        out.extend_from_slice(&data[pos..end]);
        if &data[end..end + 2] != b"\r\n" {
            return Err(Error::runtime("missing CRLF after HTTP chunk"));
        }
        pos = end + 2;
    }
    Ok(out)
}

struct ListV2Page {
    keys: Vec<String>,
    is_truncated: bool,
    next_token: Option<String>,
}

fn unescape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(sub) = after.strip_prefix("&amp;") {
            out.push('&');
            rest = sub;
        } else if let Some(sub) = after.strip_prefix("&lt;") {
            out.push('<');
            rest = sub;
        } else if let Some(sub) = after.strip_prefix("&gt;") {
            out.push('>');
            rest = sub;
        } else if let Some(sub) = after.strip_prefix("&quot;") {
            out.push('"');
            rest = sub;
        } else if let Some(sub) = after.strip_prefix("&apos;") {
            out.push('\'');
            rest = sub;
        } else {
            out.push('&');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    out
}

fn parse_list_v2_xml(xml: &str) -> ListV2Page {
    fn collect_tags(xml: &str, open: &str, close: &str) -> Vec<String> {
        let mut items = Vec::new();
        let mut cursor = xml;
        while let Some(s) = cursor.find(open) {
            let after_open = &cursor[s + open.len()..];
            let Some(e) = after_open.find(close) else {
                break;
            };
            items.push(unescape_xml(after_open[..e].trim()));
            cursor = &after_open[e + close.len()..];
        }
        items
    }
    let keys = collect_tags(xml, "<Key>", "</Key>");
    let is_truncated = collect_tags(xml, "<IsTruncated>", "</IsTruncated>")
        .into_iter()
        .any(|v| v.eq_ignore_ascii_case("true"));
    let next_token = collect_tags(xml, "<NextContinuationToken>", "</NextContinuationToken>")
        .into_iter()
        .next();
    ListV2Page {
        keys,
        is_truncated,
        next_token,
    }
}

// ============================================================================
// Binary `.lseg` Segment Bundles & `.lrmf` Remote Cold Manifests
// ============================================================================

/// Pack a local immutable `DiskSegment` directory into one self-verifying `LSEG\x01` bundle.
pub fn pack_segment_dir(seg_dir: &Path) -> Result<Vec<u8>, Error> {
    let (out, _) = pack_segment_dir_with_crc(seg_dir)?;
    Ok(out)
}

fn pack_segment_dir_with_crc(seg_dir: &Path) -> Result<(Vec<u8>, u32), Error> {
    let mut files: Vec<(String, u64)> = Vec::new();
    for ent in fs::read_dir(seg_dir).map_err(io_err)? {
        let ent = ent.map_err(io_err)?;
        let meta = ent.metadata().map_err(io_err)?;
        if !meta.is_file() {
            continue;
        }
        let name = ent
            .file_name()
            .into_string()
            .map_err(|_| Error::runtime("non-UTF8 file name in disk segment"))?;
        files.push((name, meta.len()));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut total_cap: usize = LSEG_MAGIC.len() + 4 + FRAME_CRC_TAG.len() + 4;
    for (name, flen) in &files {
        total_cap = total_cap
            .saturating_add(2 + name.len() + 8)
            .saturating_add(*flen as usize);
    }
    let mut out = Vec::with_capacity(total_cap);
    out.extend_from_slice(LSEG_MAGIC);
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (name, flen) in files {
        let nbytes = name.as_bytes();
        let nlen = u16::try_from(nbytes.len())
            .map_err(|_| Error::runtime("segment file name exceeds u16::MAX"))?;
        out.extend_from_slice(&nlen.to_le_bytes());
        out.extend_from_slice(nbytes);
        let len_slot_pos = out.len();
        out.extend_from_slice(&flen.to_le_bytes());
        let data_start = out.len();
        let mut f = File::open(seg_dir.join(&name)).map_err(io_err)?;
        f.read_to_end(&mut out).map_err(io_err)?;
        let actual_len = (out.len() - data_start) as u64;
        if actual_len != flen {
            out[len_slot_pos..len_slot_pos + 8].copy_from_slice(&actual_len.to_le_bytes());
        }
    }
    let mut hasher = crate::crc::Hasher::new();
    hasher.update(&out);
    let body_crc = hasher.finalize();
    out.extend_from_slice(FRAME_CRC_TAG);
    out.extend_from_slice(&body_crc.to_le_bytes());
    hasher.update(&out[out.len() - 8..]);
    let bundle_crc = hasher.finalize();
    Ok((out, bundle_crc))
}

/// Verify `LSEG\x01` + `LCRC` integrity first, then atomically extract into `dst_dir`.
pub fn unpack_segment_dir(bytes: &[u8], dst_dir: &Path) -> Result<(), Error> {
    unpack_segment_dir_checked(bytes, dst_dir, None)
}

fn unpack_segment_dir_checked(
    bytes: &[u8],
    dst_dir: &Path,
    expected_bundle_crc: Option<(u64, u32)>,
) -> Result<(), Error> {
    if bytes.len() < LSEG_MAGIC.len() + 4 + FRAME_CRC_TAG.len() + 4 {
        if let Some((seg_id, exp_bundle_crc)) = expected_bundle_crc
            && hardware_crc32c(bytes) != exp_bundle_crc
        {
            return Err(Error::runtime(format!(
                "remote segment seg_{seg_id:08} CRC mismatch against manifest"
            )));
        }
        return Err(Error::runtime("truncated LSEG segment bundle"));
    }
    let crc_off = bytes.len() - 8;
    let mut hasher = crate::crc::Hasher::new();
    hasher.update(&bytes[..crc_off]);
    let actual_crc = hasher.finalize();
    if let Some((seg_id, exp_bundle_crc)) = expected_bundle_crc {
        let mut bundle_hasher = hasher;
        bundle_hasher.update(&bytes[crc_off..]);
        if bundle_hasher.finalize() != exp_bundle_crc {
            return Err(Error::runtime(format!(
                "remote segment seg_{seg_id:08} CRC mismatch against manifest"
            )));
        }
    }
    if &bytes[..LSEG_MAGIC.len()] != LSEG_MAGIC {
        return Err(Error::runtime("invalid LSEG magic"));
    }
    if &bytes[crc_off..crc_off + 4] != FRAME_CRC_TAG {
        return Err(Error::runtime("missing LCRC trailer in LSEG bundle"));
    }
    let expected_crc = u32::from_le_bytes([
        bytes[crc_off + 4],
        bytes[crc_off + 5],
        bytes[crc_off + 6],
        bytes[crc_off + 7],
    ]);
    if expected_crc != actual_crc {
        return Err(Error::runtime(format!(
            "corrupt LSEG segment bundle (crc expected {expected_crc:#010x}, got {actual_crc:#010x})"
        )));
    }

    let mut cur = &bytes[LSEG_MAGIC.len()..crc_off];
    let count = read_u32_le(&mut cur)? as usize;
    let mut entries: Vec<(&str, &[u8])> = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let nlen = read_u16_le(&mut cur)? as usize;
        let name_bytes = read_slice(&mut cur, nlen)?;
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| Error::runtime("non-UTF8 file name in LSEG bundle"))?;
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains('/')
            || name.contains('\\')
            || name.contains('\0')
        {
            return Err(Error::runtime(format!("unsafe LSEG member name {name:?}")));
        }
        let dlen = read_u64_le(&mut cur)? as usize;
        let data = read_slice(&mut cur, dlen)?;
        entries.push((name, data));
    }
    if !cur.is_empty() {
        return Err(Error::runtime("trailing bytes inside LSEG segment bundle"));
    }

    let parent = dst_dir
        .parent()
        .ok_or_else(|| Error::runtime("segment destination directory has no parent"))?;
    fs::create_dir_all(parent).map_err(io_err)?;
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp_dir = parent.join(format!(".tmp_lseg_{}_{}", std::process::id(), seq));
    fs::create_dir_all(&tmp_dir).map_err(io_err)?;
    let write_res = (|| -> Result<(), Error> {
        for (name, data) in entries {
            let fpath = tmp_dir.join(name);
            fs::write(&fpath, data).map_err(io_err)?;
        }
        if dst_dir.exists() {
            fs::remove_dir_all(dst_dir).map_err(io_err)?;
        }
        fs::rename(&tmp_dir, dst_dir).map_err(io_err)?;
        Ok(())
    })();
    if write_res.is_err() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }
    write_res
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteSegEntry {
    pub seg_id: u64,
    pub table: String,
    pub row_count: u32,
    pub bundle_bytes: u64,
    pub bundle_crc32c: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteColdManifest {
    pub(crate) disk_manifest: DiskManifest,
    pub(crate) remote_segs: Vec<RemoteSegEntry>,
    pub(crate) tombstones_raw: Vec<u8>,
    pub(crate) db_snapshot: Option<Vec<u8>>,
}

fn download_remote_segment(tier: &RemoteTier, seg_id: u64, dir: &Path) -> Result<(), Error> {
    let key = object_key(
        &tier.inner.prefix,
        &format!("segments/seg_{seg_id:08}.lseg"),
    )?;
    let bundle = tier
        .inner
        .store
        .get(&key)?
        .ok_or_else(|| Error::runtime(format!("remote cold segment {key:?} missing")))?;
    let checksum = tier
        .inner
        .seg_entries
        .get(&seg_id)
        .map(|entry| (seg_id, entry.bundle_crc32c));
    unpack_segment_dir_checked(&bundle, dir, checksum)
}

fn segment_may_match(meta: &SegmentMeta, pred: &crate::ast::Pred) -> bool {
    use crate::ast::{CmpOp, Pred, Value};
    match pred {
        Pred::And(a, b) => segment_may_match(meta, a) && segment_may_match(meta, b),
        Pred::Or(a, b) => segment_may_match(meta, a) || segment_may_match(meta, b),
        Pred::Cmp { field, op, value } => {
            let Some(name) = field.leaf() else {
                return true;
            };
            let Some(&(lo, hi)) = meta.integer_bounds.get(name) else {
                return true;
            };
            let n = match value {
                Value::Int(n)
                    if meta
                        .columns
                        .iter()
                        .any(|c| c.name == name && matches!(c.kind, super::DiskColKind::Int64)) =>
                {
                    *n
                }
                Value::Timestamp(n)
                    if meta.columns.iter().any(|c| {
                        c.name == name && matches!(c.kind, super::DiskColKind::Time64)
                    }) =>
                {
                    *n
                }
                _ => return true,
            };
            match op {
                CmpOp::Eq => lo <= n && n <= hi,
                CmpOp::Gt => hi > n,
                CmpOp::Ge => hi >= n,
                CmpOp::Lt => lo < n,
                CmpOp::Le => lo <= n,
                CmpOp::Ne => true,
            }
        }
        _ => true,
    }
}

impl RemoteColdManifest {
    pub(crate) fn encode_wire(&self) -> Result<Vec<u8>, Error> {
        let manifest_json = serde_json::to_vec(&self.disk_manifest).map_err(io_err)?;
        let mut out =
            Vec::with_capacity(64 + manifest_json.len() + self.tombstones_raw.len());
        out.extend_from_slice(LRMF_MAGIC);
        out.extend_from_slice(&LRMF_VERSION.to_le_bytes());
        out.extend_from_slice(&(manifest_json.len() as u32).to_le_bytes());
        out.extend_from_slice(&manifest_json);
        out.extend_from_slice(&(self.remote_segs.len() as u32).to_le_bytes());
        for seg in &self.remote_segs {
            out.extend_from_slice(&seg.seg_id.to_le_bytes());
            let tb = seg.table.as_bytes();
            let tlen = u16::try_from(tb.len())
                .map_err(|_| Error::runtime("table name exceeds u16::MAX in LRMF"))?;
            out.extend_from_slice(&tlen.to_le_bytes());
            out.extend_from_slice(tb);
            out.extend_from_slice(&seg.row_count.to_le_bytes());
            out.extend_from_slice(&seg.bundle_bytes.to_le_bytes());
            out.extend_from_slice(&seg.bundle_crc32c.to_le_bytes());
        }
        out.extend_from_slice(&(self.tombstones_raw.len() as u64).to_le_bytes());
        out.extend_from_slice(&self.tombstones_raw);
        match &self.db_snapshot {
            Some(snap) => {
                out.push(1);
                out.extend_from_slice(&(snap.len() as u64).to_le_bytes());
                out.extend_from_slice(snap);
            }
            None => out.push(0),
        }
        let crc = hardware_crc32c(&out);
        out.extend_from_slice(FRAME_CRC_TAG);
        out.extend_from_slice(&crc.to_le_bytes());
        Ok(out)
    }

    pub(crate) fn decode_wire(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < LRMF_MAGIC.len() + 12 + FRAME_CRC_TAG.len() + 4 {
            return Err(Error::runtime("truncated LRMF remote manifest"));
        }
        if &bytes[..LRMF_MAGIC.len()] != LRMF_MAGIC {
            return Err(Error::runtime("invalid LRMF magic"));
        }
        let crc_off = bytes.len() - 8;
        if &bytes[crc_off..crc_off + 4] != FRAME_CRC_TAG {
            return Err(Error::runtime("missing LCRC trailer in LRMF manifest"));
        }
        let expected_crc = u32::from_le_bytes([
            bytes[crc_off + 4],
            bytes[crc_off + 5],
            bytes[crc_off + 6],
            bytes[crc_off + 7],
        ]);
        let actual_crc = hardware_crc32c(&bytes[..crc_off]);
        if expected_crc != actual_crc {
            return Err(Error::runtime(format!(
                "corrupt LRMF remote manifest (crc expected {expected_crc:#010x}, got {actual_crc:#010x})"
            )));
        }

        let mut cur = &bytes[LRMF_MAGIC.len()..crc_off];
        let ver = read_u32_le(&mut cur)?;
        if ver != LRMF_VERSION {
            return Err(Error::runtime(format!("unsupported LRMF version {ver}")));
        }
        let man_len = read_u32_le(&mut cur)? as usize;
        let man_bytes = read_slice(&mut cur, man_len)?;
        let disk_manifest: DiskManifest = serde_json::from_slice(man_bytes).map_err(io_err)?;

        let seg_count = read_u32_le(&mut cur)? as usize;
        let mut remote_segs = Vec::with_capacity(seg_count.min(4096));
        for _ in 0..seg_count {
            let seg_id = read_u64_le(&mut cur)?;
            let tlen = read_u16_le(&mut cur)? as usize;
            let tb = read_slice(&mut cur, tlen)?;
            let table = std::str::from_utf8(tb)
                .map_err(|_| Error::runtime("non-UTF8 table name in LRMF"))?
                .to_string();
            let row_count = read_u32_le(&mut cur)?;
            let bundle_bytes = read_u64_le(&mut cur)?;
            let bundle_crc32c = read_u32_le(&mut cur)?;
            remote_segs.push(RemoteSegEntry {
                seg_id,
                table,
                row_count,
                bundle_bytes,
                bundle_crc32c,
            });
        }

        let tomb_len = read_u64_le(&mut cur)? as usize;
        let tombstones_raw = read_slice(&mut cur, tomb_len)?.to_vec();
        let has_snap = read_u8(&mut cur)?;
        let db_snapshot = if has_snap != 0 {
            let slen = read_u64_le(&mut cur)? as usize;
            Some(read_slice(&mut cur, slen)?.to_vec())
        } else {
            None
        };
        if !cur.is_empty() {
            return Err(Error::runtime("trailing bytes in LRMF remote manifest"));
        }
        Ok(Self {
            disk_manifest,
            remote_segs,
            tombstones_raw,
            db_snapshot,
        })
    }
}

// ============================================================================
// Remote Tier Configuration, Lazy Hydration & Table-Granularity LRU Eviction
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteCacheConfig {
    pub lazy: bool,
    pub max_local_bytes: u64,
}

impl Default for RemoteCacheConfig {
    fn default() -> Self {
        Self {
            lazy: true,
            max_local_bytes: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteSyncStats {
    pub generation: u64,
    pub segments_uploaded: usize,
    pub segments_reused: usize,
    pub segments_deleted_remote: usize,
    pub bytes_uploaded: u64,
}

#[derive(Default)]
struct LruState {
    clock: u64,
    table_bytes: FxHashMap<String, u64>,
    table_tick: FxHashMap<String, u64>,
    local_bytes: u64,
}

struct RemoteTierInner {
    store: Arc<dyn ColdObjectStore>,
    prefix: String,
    cfg: RemoteCacheConfig,
    seg_entries: FxHashMap<u64, RemoteSegEntry>,
    lru: Mutex<LruState>,
}

#[derive(Clone)]
pub(crate) struct RemoteTier {
    inner: Arc<RemoteTierInner>,
}

impl RemoteTier {
    fn touch_table(&self, table: &str, bytes: u64) {
        if let Ok(mut lru) = self.inner.lru.lock() {
            lru.clock += 1;
            let tick = lru.clock;
            if let Some(prev) = lru.table_bytes.insert(table.to_string(), bytes) {
                lru.local_bytes = lru.local_bytes.saturating_sub(prev).saturating_add(bytes);
            } else {
                lru.local_bytes = lru.local_bytes.saturating_add(bytes);
            }
            lru.table_tick.insert(table.to_string(), tick);
        }
    }

    fn touch_existing(&self, table: &str) {
        if let Ok(mut lru) = self.inner.lru.lock()
            && lru.table_bytes.contains_key(table)
        {
            lru.clock += 1;
            let tick = lru.clock;
            lru.table_tick.insert(table.to_string(), tick);
        }
    }

    fn local_bytes(&self) -> u64 {
        self.inner
            .lru
            .lock()
            .map(|lru| lru.local_bytes)
            .unwrap_or(0)
    }

    fn eviction_candidates(&self, keep: Option<&str>) -> Vec<String> {
        let Ok(lru) = self.inner.lru.lock() else {
            return Vec::new();
        };
        let mut items: Vec<(u64, String)> = lru
            .table_tick
            .iter()
            .filter(|(t, _)| keep != Some(t.as_str()))
            .map(|(t, &tick)| (tick, t.clone()))
            .collect();
        items.sort_unstable();
        items.into_iter().map(|(_, t)| t).collect()
    }

    fn record_evicted(&self, table: &str) {
        if let Ok(mut lru) = self.inner.lru.lock() {
            if let Some(b) = lru.table_bytes.remove(table) {
                lru.local_bytes = lru.local_bytes.saturating_sub(b);
            }
            lru.table_tick.remove(table);
        }
    }
}

fn dir_size_bytes(dir: &Path) -> Result<u64, Error> {
    let mut total = 0u64;
    for ent in fs::read_dir(dir).map_err(io_err)? {
        let ent = ent.map_err(io_err)?;
        let meta = ent.metadata().map_err(io_err)?;
        if meta.is_file() {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

impl DiskEngine {
    pub(crate) fn push_to_object_store_with_snapshot(
        &mut self,
        store: &dyn ColdObjectStore,
        prefix: &str,
        prune_remote_orphans: bool,
        db_snapshot: Option<&[u8]>,
    ) -> Result<RemoteSyncStats, Error> {
        let norm_prefix = normalize_prefix(prefix)?;
        if !self.read_only {
            self.checkpoint()?;
        }

        let latest_key = object_key(&norm_prefix, "manifests/latest.lrmf")?;
        let existing_segs: FxHashMap<u64, RemoteSegEntry> = match store.get(&latest_key)? {
            Some(bytes) => {
                let prev = RemoteColdManifest::decode_wire(&bytes)?;
                prev.remote_segs
                    .into_iter()
                    .map(|e| (e.seg_id, e))
                    .collect()
            }
            None => FxHashMap::default(),
        };

        let mut stats = RemoteSyncStats {
            generation: self.manifest.generation,
            ..RemoteSyncStats::default()
        };
        let mut remote_segs = Vec::with_capacity(self.manifest.segments.len());
        let mut live_ids = BTreeSet::new();

        for meta in &self.manifest.segments {
            live_ids.insert(meta.seg_id);
            if let Some(prev) = existing_segs.get(&meta.seg_id)
                && prev.table == meta.table
                && prev.row_count == meta.row_count
            {
                stats.segments_reused += 1;
                remote_segs.push(prev.clone());
                continue;
            }
            let seg_dir = self
                .dir
                .join(SEGMENTS_DIR)
                .join(format!("seg_{:08}", meta.seg_id));
            let (bundle, seg_crc) = pack_segment_dir_with_crc(&seg_dir)?;
            let seg_key = object_key(
                &norm_prefix,
                &format!("segments/seg_{:08}.lseg", meta.seg_id),
            )?;
            store.put(&seg_key, &bundle)?;
            stats.segments_uploaded += 1;
            stats.bytes_uploaded = stats.bytes_uploaded.saturating_add(bundle.len() as u64);
            remote_segs.push(RemoteSegEntry {
                seg_id: meta.seg_id,
                table: meta.table.clone(),
                row_count: meta.row_count,
                bundle_bytes: bundle.len() as u64,
                bundle_crc32c: seg_crc,
            });
        }

        let tombstones_raw = rmp_serde::to_vec(&self.tombstones).map_err(io_err)?;
        let mut disk_manifest = self.manifest.clone();
        disk_manifest.tombstones_crc = Some((
            tombstones_raw.len() as u64,
            crate::crc::hardware_crc32c(&tombstones_raw),
        ));
        let remote_manifest = RemoteColdManifest {
            disk_manifest,
            remote_segs,
            tombstones_raw,
            db_snapshot: db_snapshot.map(|s| s.to_vec()),
        };
        let manifest_bytes = remote_manifest.encode_wire()?;
        let gen_key = object_key(
            &norm_prefix,
            &format!("manifests/gen_{:016}.lrmf", self.manifest.generation),
        )?;
        store.put(&gen_key, &manifest_bytes)?;
        stats.bytes_uploaded = stats
            .bytes_uploaded
            .saturating_add(manifest_bytes.len() as u64);
        store.put(&latest_key, &manifest_bytes)?;
        stats.bytes_uploaded = stats
            .bytes_uploaded
            .saturating_add(manifest_bytes.len() as u64);

        if prune_remote_orphans {
            let seg_pfx = if norm_prefix.is_empty() {
                "segments/".to_string()
            } else {
                format!("{norm_prefix}/segments/")
            };
            for key in store.list(&seg_pfx)? {
                let fname = key.rsplit('/').next().unwrap_or("");
                if let Some(stem) = fname
                    .strip_prefix("seg_")
                    .and_then(|s| s.strip_suffix(".lseg"))
                    && let Ok(id) = stem.parse::<u64>()
                    && !live_ids.contains(&id)
                {
                    store.delete(&key)?;
                    stats.segments_deleted_remote += 1;
                }
            }
        }
        Ok(stats)
    }

    pub fn push_to_object_store(
        &mut self,
        store: &dyn ColdObjectStore,
        prefix: &str,
        prune_remote_orphans: bool,
    ) -> Result<RemoteSyncStats, Error> {
        self.push_to_object_store_with_snapshot(store, prefix, prune_remote_orphans, None)
    }

    pub(crate) fn open_from_object_store_with_snapshot(
        local_cache_dir: impl AsRef<Path>,
        store: Arc<dyn ColdObjectStore>,
        prefix: &str,
        cfg: RemoteCacheConfig,
    ) -> Result<(Self, Option<Vec<u8>>), Error> {
        let dir = local_cache_dir.as_ref().to_path_buf();
        let norm_prefix = normalize_prefix(prefix)?;
        let latest_key = object_key(&norm_prefix, "manifests/latest.lrmf")?;
        let raw_manifest = store
            .get(&latest_key)?
            .ok_or_else(|| Error::runtime(format!("remote manifest {latest_key:?} not found")))?;
        let mut remote_manifest = RemoteColdManifest::decode_wire(&raw_manifest)?;
        if remote_manifest.disk_manifest.version > DISK_FORMAT_VERSION {
            return Err(Error::runtime(format!(
                "remote disk format version {} is newer than this build supports ({DISK_FORMAT_VERSION})",
                remote_manifest.disk_manifest.version
            )));
        }

        fs::create_dir_all(dir.join(SEGMENTS_DIR)).map_err(io_err)?;
        let tombstones: SegmentTombstones =
            rmp_serde::from_slice(&remote_manifest.tombstones_raw).map_err(io_err)?;
        fs::write(dir.join(TOMBSTONES_FILE), &remote_manifest.tombstones_raw).map_err(io_err)?;
        let man_json = serde_json::to_vec_pretty(&remote_manifest.disk_manifest).map_err(io_err)?;
        fs::write(dir.join(MANIFEST_FILE), man_json).map_err(io_err)?;

        let seg_entries: FxHashMap<u64, RemoteSegEntry> = remote_manifest
            .remote_segs
            .into_iter()
            .map(|e| (e.seg_id, e))
            .collect();
        let tier = RemoteTier {
            inner: Arc::new(RemoteTierInner {
                store,
                prefix: norm_prefix,
                cfg,
                seg_entries,
                lru: Mutex::new(LruState::default()),
            }),
        };

        let db_snap = remote_manifest.db_snapshot.take();
        let mut engine = Self {
            dir,
            manifest: remote_manifest.disk_manifest,
            segments: BTreeMap::new(),
            memtables: BTreeMap::new(),
            has_any_overlay: false,
            tombstones,
            hot_cache: Arc::new(HotTierCache::new(HotCacheConfig::default())),
            active_savepoint: None,
            overlay_epoch: 0,
            dirty_overlay: FxHashSet::default(),
            wal_file: None,
            wal_buf: Vec::new(),
            wal_bytes: 0,
            sync_full: false,
            read_only: true,
            remote_tier: Some(tier),
            join_dim_cache: Mutex::new(None),
        };

        if !cfg.lazy {
            let tables: Vec<String> = engine.manifest.tables.keys().cloned().collect();
            for t in tables {
                let _ = engine.hydrate_table(&t)?;
            }
        }
        Ok((engine, db_snap))
    }

    pub fn open_from_object_store(
        local_cache_dir: impl AsRef<Path>,
        store: Arc<dyn ColdObjectStore>,
        prefix: &str,
        cfg: RemoteCacheConfig,
    ) -> Result<Self, Error> {
        let (eng, _) =
            Self::open_from_object_store_with_snapshot(local_cache_dir, store, prefix, cfg)?;
        Ok(eng)
    }

    pub fn is_table_hydrated(&self, table: &str) -> bool {
        if self.segments.contains_key(table) {
            return true;
        }
        !self
            .manifest
            .segments
            .iter()
            .any(|m| m.table == table)
    }

    pub fn has_unhydrated_tables(&self) -> bool {
        self.remote_tier.is_some()
            && self
                .manifest
                .segments
                .iter()
                .any(|m| !self.segments.contains_key(&m.table))
    }

    pub fn local_cache_bytes(&self) -> u64 {
        if let Some(rt) = &self.remote_tier {
            return rt.local_bytes();
        }
        self.manifest
            .segments
            .iter()
            .filter_map(|m| {
                let d = self.dir.join(SEGMENTS_DIR).join(format!("seg_{:08}", m.seg_id));
                dir_size_bytes(&d).ok()
            })
            .sum()
    }

    pub(crate) fn remote_pruned_segments(
        &self,
        table: &str,
        pred: &crate::ast::Pred,
    ) -> Option<Vec<SegmentMeta>> {
        if self.remote_tier.is_none() || self.is_table_hydrated(table) {
            return None;
        }
        let mut pruned = false;
        let selected = self
            .manifest
            .segments
            .iter()
            .filter(|meta| meta.table == table)
            .filter(|meta| {
                let matches = segment_may_match(meta, pred);
                pruned |= !matches;
                matches
            })
            .cloned()
            .collect();
        pruned.then_some(selected)
    }

    /// Load an isolated view for a single filtered scan. The caller owns the
    /// staging directory and removes it after the view has been dropped.
    pub(crate) fn remote_filtered_view(
        &self,
        table: &str,
        selected: Vec<SegmentMeta>,
        staging: &Path,
    ) -> Result<Self, Error> {
        let tier = self
            .remote_tier
            .as_ref()
            .ok_or_else(|| Error::runtime("filtered scan requires remote storage"))?;
        let mut view = self.snapshot_view();
        view.dir = staging.to_path_buf();
        view.remote_tier = None;
        view.manifest.segments.retain(|meta| meta.table != table);
        for meta in selected {
            let dir = staging.join(format!("seg_{:08}", meta.seg_id));
            download_remote_segment(tier, meta.seg_id, &dir)?;
            let segment = DiskSegment::open(&dir, meta.clone())?;
            view.manifest.segments.push(meta);
            view.segments
                .entry(table.to_owned())
                .or_default()
                .push(Arc::new(segment));
        }
        Ok(view)
    }

    pub fn hydrate_table(&mut self, table: &str) -> Result<usize, Error> {
        if self.segments.contains_key(table) {
            if let Some(rt) = &self.remote_tier {
                rt.touch_existing(table);
            }
            return Ok(0);
        }
        let Some(rt) = self.remote_tier.clone() else {
            return Ok(0);
        };
        let metas: Vec<SegmentMeta> = self
            .manifest
            .segments
            .iter()
            .filter(|m| m.table == table)
            .cloned()
            .collect();
        if metas.is_empty() {
            return Ok(0);
        }

        let mut downloaded_dirs = Vec::new();
        let mut opened = Vec::with_capacity(metas.len());
        let mut total_bytes = 0u64;
        let load_res = (|| -> Result<usize, Error> {
            let mut segs_loaded = 0usize;
            for meta in metas {
                let seg_dir = self
                    .dir
                    .join(SEGMENTS_DIR)
                    .join(format!("seg_{:08}", meta.seg_id));
                if !seg_dir.exists() {
                    download_remote_segment(&rt, meta.seg_id, &seg_dir)?;
                    downloaded_dirs.push(seg_dir.clone());
                }
                total_bytes = total_bytes.saturating_add(dir_size_bytes(&seg_dir)?);
                let seg = DiskSegment::open(&seg_dir, meta)?;
                opened.push(Arc::new(seg));
                segs_loaded += 1;
            }
            Ok(segs_loaded)
        })();

        let segs_loaded = match load_res {
            Ok(n) => n,
            Err(e) => {
                for d in downloaded_dirs {
                    let _ = fs::remove_dir_all(d);
                }
                return Err(e);
            }
        };

        self.segments.insert(table.to_string(), opened);
        self.hot_cache.invalidate_table(table);
        rt.touch_table(table, total_bytes);
        if rt.inner.cfg.max_local_bytes > 0
            && rt.local_bytes() > rt.inner.cfg.max_local_bytes
        {
            self.evict_local_cache_except(rt.inner.cfg.max_local_bytes, Some(table))?;
        }
        Ok(segs_loaded)
    }

    pub fn evict_local_cache_to(&mut self, max_bytes: u64) -> Result<u64, Error> {
        self.evict_local_cache_except(max_bytes, None)
    }

    fn evict_local_cache_except(
        &mut self,
        max_bytes: u64,
        keep: Option<&str>,
    ) -> Result<u64, Error> {
        let Some(rt) = self.remote_tier.clone() else {
            return Ok(self.local_cache_bytes());
        };
        let mut evicted_bytes = 0u64;
        for cand in rt.eviction_candidates(keep) {
            if rt.local_bytes() <= max_bytes {
                break;
            }
            let can_evict = self
                .segments
                .get(&cand)
                .is_some_and(|segs| segs.iter().all(|s| Arc::strong_count(s) == 1));
            if !can_evict {
                continue;
            }
            if let Some(segs) = self.segments.remove(&cand) {
                for seg in segs {
                    let seg_id = seg.meta.seg_id;
                    drop(seg);
                    let seg_dir = self
                        .dir
                        .join(SEGMENTS_DIR)
                        .join(format!("seg_{seg_id:08}"));
                    if let Ok(sz) = dir_size_bytes(&seg_dir) {
                        evicted_bytes = evicted_bytes.saturating_add(sz);
                    }
                    if seg_dir.exists() {
                        fs::remove_dir_all(&seg_dir).map_err(io_err)?;
                    }
                }
                rt.record_evicted(&cand);
                self.hot_cache.invalidate_table(&cand);
            }
        }
        let _ = evicted_bytes;
        Ok(rt.local_bytes())
    }

    pub(crate) fn export_cold_bootstrap_parts(
        &self,
    ) -> Result<(Vec<u8>, Vec<u8>, Vec<(u64, Vec<u8>)>), Error> {
        let manifest_json = serde_json::to_vec_pretty(&self.manifest).map_err(io_err)?;
        let tombstones_raw = rmp_serde::to_vec(&self.tombstones).map_err(io_err)?;
        let mut segs = Vec::with_capacity(self.manifest.segments.len());
        for meta in &self.manifest.segments {
            let seg_dir = self
                .dir
                .join(SEGMENTS_DIR)
                .join(format!("seg_{:08}", meta.seg_id));
            let bundle = if seg_dir.exists() {
                pack_segment_dir(&seg_dir)?
            } else if let Some(rt) = &self.remote_tier {
                let key = object_key(
                    &rt.inner.prefix,
                    &format!("segments/seg_{:08}.lseg", meta.seg_id),
                )?;
                rt.inner.store.get(&key)?.ok_or_else(|| {
                    Error::runtime(format!("cold bootstrap segment {key:?} missing"))
                })?
            } else {
                return Err(Error::runtime(format!(
                    "cold segment seg_{:08} missing on disk",
                    meta.seg_id
                )));
            };
            segs.push((meta.seg_id, bundle));
        }
        Ok((manifest_json, tombstones_raw, segs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_and_hmac_match_standard_vectors() {
        assert_eq!(
            hex_lower(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex_lower(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex_lower(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );

        // RFC 4231 Test Case 1 & 2
        let key1 = [0x0bu8; 20];
        assert_eq!(
            hex_lower(&hmac_sha256(&key1, b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
        assert_eq!(
            hex_lower(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn sigv4_matches_aws_reference_vector() {
        // Official AWS SigV4 GetObject reference test vector.
        let empty_sha = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        let (_, auth, _) = build_sigv4_authorization(
            "GET",
            "/test.txt",
            &[],
            "examplebucket.s3.amazonaws.com",
            empty_sha,
            "20130524T000000Z",
            "20130524",
            "us-east-1",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            None,
        );
        assert!(
            auth.contains("Signature=df548e2ce037944d03f3e68682813b093763996d597cf890ca3d9037fd231eb4"),
            "unexpected SigV4 signature: {auth}"
        );
    }

    #[test]
    fn list_v2_xml_unescapes_and_handles_pagination() {
        let xml = r#"<ListBucketResult><IsTruncated>true</IsTruncated><NextContinuationToken>tok&amp;2</NextContinuationToken><Contents><Key>pfx/a&amp;b.lseg</Key></Contents><Contents><Key>pfx/c&lt;d&gt;.lrmf</Key></Contents></ListBucketResult>"#;
        let page = parse_list_v2_xml(xml);
        assert!(page.is_truncated);
        assert_eq!(page.next_token.as_deref(), Some("tok&2"));
        assert_eq!(page.keys, vec!["pfx/a&b.lseg", "pfx/c<d>.lrmf"]);
    }

    #[test]
    fn fs_object_store_rejects_traversal_and_roundtrips() {
        let dir = std::env::temp_dir().join(format!("lin_fs_obj_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let fs_store = FsObjectStore::open(&dir).expect("open FsObjectStore");
        assert!(fs_store.put("../escape", b"bad").is_err());
        assert!(fs_store.put("/abs", b"bad").is_err());
        assert!(fs_store.put("a\\b", b"bad").is_err());
        assert!(fs_store.put("a/../b", b"bad").is_err());

        fs_store.put("cold/segments/seg_00000001.lseg", b"hello").unwrap();
        assert_eq!(
            fs_store.get("cold/segments/seg_00000001.lseg").unwrap().as_deref(),
            Some(b"hello".as_slice())
        );
        assert_eq!(
            fs_store.list("cold/").unwrap(),
            vec!["cold/segments/seg_00000001.lseg".to_string()]
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
