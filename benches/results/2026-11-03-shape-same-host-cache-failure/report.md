# Same-host diagnostic invalidated by build-cache contamination

Run 41612695460 finished green, but independent candidate audit failed: candidate ordinary worker emits INSERT_PHASE/DETAIL diagnostics and its SHA256 equals baseline diagnostic worker c81e87d433b0b8f2290a95b8f7484beca0105226eab686bc1c29739309e1a598. Candidate ordinary build log reports Finished in 0.12s with no compilation. The shared target symlink allowed stale output reuse across worktrees. Source-file hashes alone do not prove binary provenance.

Baseline full audit succeeded; overall source comparison is invalid. Preserve every artifact and do not claim a speedup or causal attribution. Candidate diagnostic data is not promoted to an independently verified comparison here. A corrected diagnostic run must force package rebuilds and reject any ordinary stderr before marking success. This is an infrastructure correction to diagnostic attribution; the separate 216-process qualification refusal remains binding and is not retried. Its ordinary baseline and candidate logs both explicitly compile lin/lin-peer-worker from their respective source directories (43.09s and 44.11s); the preceding single-candidate ordinary profile likewise compiled from its exact candidate directory (48.25s).

All downloaded members were archived and reread against manifest.json. No runtime code changed.
