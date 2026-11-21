//! Ollama HTTP embedder (`feature = "embed-ollama"`).
//!
//! POST `{base}/api/embeddings` with `{ "model", "prompt" }` → `{ "embedding": [f32…] }`.
//! Blocking (`Embedder::embed` is sync). Default hashing embedder stays the Db default.

use std::sync::Arc;

use crate::embed::Embedder;
use crate::error::Error;

/// Neural embeddings via a local [Ollama](https://ollama.com) server.
#[derive(Debug, Clone)]
pub struct OllamaEmbedder {
    id: String,
    dim: usize,
    base_url: String,
    endpoint_url: String,
    model: String,
    agent: ureq::Agent,
    zero_vec: Arc<[f32]>,
}

impl OllamaEmbedder {
    /// `embed_id` like `nomic-embed-text/768` (model name + `/dim`).
    /// `base_url` e.g. `http://127.0.0.1:11434`.
    pub fn new(base_url: impl Into<String>, embed_id: impl Into<String>) -> Result<Self, Error> {
        let id = embed_id.into();
        let dim = id
            .rsplit('/')
            .next()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(768)
            .max(1);
        let model = id
            .rsplit_once('/')
            .map(|(m, _)| m.to_string())
            .unwrap_or_else(|| id.clone());
        let base = base_url.into().trim_end_matches('/').to_string();
        if base.is_empty() {
            return Err(Error::runtime("ollama base_url is empty"));
        }
        if base.starts_with("http://") {
            let host_part = base
                .strip_prefix("http://")
                .unwrap_or("")
                .split(&[':', '/'][..])
                .next()
                .unwrap_or("");
            if host_part != "127.0.0.1"
                && host_part != "localhost"
                && host_part != "[::1]"
                && host_part != "::1"
            {
                return Err(Error::runtime("remote HTTP requires TLS (https://)"));
            }
        }
        let endpoint_url = format!("{base}/api/embeddings");
        let agent = ureq::Agent::new_with_defaults();
        let zero_vec: Arc<[f32]> = Arc::from(vec![0f32; dim]);
        Ok(Self {
            id,
            dim,
            base_url: base,
            endpoint_url,
            model,
            agent,
            zero_vec,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Embedder for OllamaEmbedder {
    fn id(&self) -> &str {
        &self.id
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Arc<[f32]> {
        match embed_ollama_endpoint(&self.agent, &self.endpoint_url, &self.model, text) {
            Ok(v) => {
                if v.len() == self.dim {
                    Arc::from(v)
                } else if v.is_empty() {
                    Arc::clone(&self.zero_vec)
                } else {
                    // Truncate / pad to catalog dim so cosine stays defined.
                    let mut out = vec![0f32; self.dim];
                    let n = v.len().min(self.dim);
                    out[..n].copy_from_slice(&v[..n]);
                    Arc::from(out)
                }
            }
            Err(_) => Arc::clone(&self.zero_vec),
        }
    }
}

#[derive(serde::Serialize)]
struct OllamaEmbedReq<'a> {
    model: &'a str,
    prompt: &'a str,
}

#[derive(serde::Deserialize)]
struct OllamaEmbedResp {
    embedding: Vec<f32>,
}

fn embed_ollama_endpoint(
    agent: &ureq::Agent,
    endpoint_url: &str,
    model: &str,
    text: &str,
) -> Result<Vec<f32>, Error> {
    let body = OllamaEmbedReq {
        model,
        prompt: text,
    };
    let resp = agent
        .post(endpoint_url)
        .send_json(&body)
        .map_err(|e| Error::runtime(format!("ollama embed: {e}")))?;
    let status = resp.status();
    if !(200..300).contains(&status.as_u16()) {
        return Err(Error::runtime(format!(
            "ollama embed: HTTP {}",
            status.as_u16()
        )));
    }
    let parsed: OllamaEmbedResp = resp
        .into_body()
        .read_json()
        .map_err(|e| Error::runtime(format!("ollama embed json: {e}")))?;
    Ok(parsed.embedding)
}

#[cfg(test)]
fn embed_ollama(base_url: &str, model: &str, text: &str) -> Result<Vec<f32>, Error> {
    let agent = ureq::Agent::new_with_defaults();
    let endpoint_url = format!("{base_url}/api/embeddings");
    embed_ollama_endpoint(&agent, &endpoint_url, model, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn ollama_embedder_parses_embed_id() {
        let e = OllamaEmbedder::new("http://127.0.0.1:11434", "nomic-embed-text/768").unwrap();
        assert_eq!(e.id(), "nomic-embed-text/768");
        assert_eq!(e.dim(), 768);
        assert_eq!(e.model(), "nomic-embed-text");
    }

    #[test]
    fn ollama_embed_against_mock_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            assert!(req.contains("POST"), "{req}");
            let body = r#"{"embedding":[0.1,0.2,0.3,0.4]}"#;
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.flush();
        });

        let url = format!("http://{addr}");
        let v = embed_ollama(&url, "mock", "hello").expect("mock ollama");
        assert_eq!(v, vec![0.1, 0.2, 0.3, 0.4]);

        let e = OllamaEmbedder::new(&url, "mock/4").unwrap();
        // Second request — spin another accept by restarting is awkward; just check dims.
        assert_eq!(e.dim(), 4);
        assert_eq!(e.id(), "mock/4");
    }
}
