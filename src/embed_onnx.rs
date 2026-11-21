//! Local ONNX embedder via [`fastembed`] (`feature = "embed-onnx"`).
//!
//! First construction may download ORT binaries / HF model weights. Not the Db
//! default — hashing remains default; wire with [`crate::Db::with_embedder`].

use std::sync::{Arc, Mutex};

use fastembed::{InitOptions, TextEmbedding};

pub use fastembed::EmbeddingModel;

use crate::embed::Embedder;
use crate::error::Error;

/// Neural embeddings via ONNX Runtime (fastembed models).
pub struct OnnxEmbedder {
    id: String,
    dim: usize,
    inner: Mutex<TextEmbedding>,
    zero_vec: Arc<[f32]>,
}

impl std::fmt::Debug for OnnxEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OnnxEmbedder")
            .field("id", &self.id)
            .field("dim", &self.dim)
            .finish_non_exhaustive()
    }
}

impl OnnxEmbedder {
    /// Load a named fastembed model. `embed_id` should be `name/dim` matching catalog.
    pub fn try_new(model: EmbeddingModel, embed_id: impl Into<String>) -> Result<Self, Error> {
        let id = embed_id.into();
        let dim = id
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(model_dim_hint(&model))
            .max(8);
        let inner =
            TextEmbedding::try_new(InitOptions::new(model).with_show_download_progress(false))
                .map_err(|e| Error::runtime(format!("onnx embedder: {e}")))?;
        let zero_vec: Arc<[f32]> = Arc::from(vec![0f32; dim]);
        Ok(Self {
            id,
            dim,
            inner: Mutex::new(inner),
            zero_vec,
        })
    }

    /// Convenience: All-MiniLM-L6-v2 (384-d) under embed_id `all-minilm-l6-v2/384`.
    pub fn all_minilm_l6_v2() -> Result<Self, Error> {
        Self::try_new(EmbeddingModel::AllMiniLML6V2, "all-minilm-l6-v2/384")
    }
}

impl Embedder for OnnxEmbedder {
    fn id(&self) -> &str {
        &self.id
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Arc<[f32]> {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return Arc::clone(&self.zero_vec),
        };
        match guard.embed(std::slice::from_ref(&text), None) {
            Ok(mut batches) => match batches.pop() {
                Some(v) if !v.is_empty() => normalize_onnx_dim(v, self.dim),
                _ => Arc::clone(&self.zero_vec),
            },
            Err(_) => Arc::clone(&self.zero_vec),
        }
    }

    fn embed_batch(&self, texts: &[&str]) -> Vec<Arc<[f32]>> {
        if texts.is_empty() {
            return Vec::new();
        }
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return vec![Arc::clone(&self.zero_vec); texts.len()],
        };
        match guard.embed(texts, None) {
            Ok(batches) => batches
                .into_iter()
                .map(|v| {
                    if v.is_empty() {
                        Arc::clone(&self.zero_vec)
                    } else {
                        normalize_onnx_dim(v, self.dim)
                    }
                })
                .collect(),
            Err(_) => vec![Arc::clone(&self.zero_vec); texts.len()],
        }
    }

    fn embed_batch_owned(&self, texts: &[&str]) -> Vec<Vec<f32>> {
        if texts.is_empty() {
            return Vec::new();
        }
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(_) => return (0..texts.len()).map(|_| vec![0f32; self.dim]).collect(),
        };
        match guard.embed(texts, None) {
            Ok(mut batches) => {
                for v in &mut batches {
                    if v.len() != self.dim {
                        v.resize(self.dim, 0.0);
                    }
                }
                batches
            }
            Err(_) => (0..texts.len()).map(|_| vec![0f32; self.dim]).collect(),
        }
    }
}

#[inline]
fn normalize_onnx_dim(v: Vec<f32>, dim: usize) -> Arc<[f32]> {
    if v.len() == dim {
        Arc::from(v)
    } else {
        let mut out = vec![0f32; dim];
        let n = v.len().min(dim);
        out[..n].copy_from_slice(&v[..n]);
        Arc::from(out)
    }
}

fn model_dim_hint(model: &EmbeddingModel) -> usize {
    // Best-effort; catalog embed_id `/dim` wins when present.
    match model {
        EmbeddingModel::BGELargeENV15
        | EmbeddingModel::BGELargeENV15Q
        | EmbeddingModel::BGELargeZHV15
        | EmbeddingModel::MxbaiEmbedLargeV1
        | EmbeddingModel::MxbaiEmbedLargeV1Q
        | EmbeddingModel::MultilingualE5Large
        | EmbeddingModel::GTEBaseENV15 => 1024,
        EmbeddingModel::BGEBaseENV15
        | EmbeddingModel::BGEBaseENV15Q
        | EmbeddingModel::NomicEmbedTextV1
        | EmbeddingModel::NomicEmbedTextV15
        | EmbeddingModel::NomicEmbedTextV15Q
        | EmbeddingModel::ParaphraseMLMpnetBaseV2
        | EmbeddingModel::MultilingualE5Base => 768,
        _ => 384,
    }
}
