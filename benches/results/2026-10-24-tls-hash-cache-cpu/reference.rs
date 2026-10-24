fn cached_content_hash(body: &str) -> Arc<str> {
    CONTENT_HASH_CACHE.with(|cache| {
        let mut guard = cache.lock().unwrap();

        if let Some(cached) = guard.get(body) {
            return cached.clone();
        }

        let hash = content_hash_arc(body);
        guard.insert(body.to_owned(), hash.clone());

        while guard.len() >= 1024 {
            if let Some(key) = guard.keys().next().cloned() {
                guard.remove(&key);
            } else {
                break;
            }
        }

        hash
    })
}
