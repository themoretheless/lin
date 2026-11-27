# Security Policy & Trust Boundaries

## Supported Versions

| Version | Supported | Storage Format Compatibility |
|---|---|---|
| `0.7.x` (`main` / `release`) | Yes | Bidirectional WAL & snapshot compatibility verified via `./scripts/format-compat.sh` |
| `< 0.7.0` | Upgrade recommended | Forward replay supported via versioned WAL/snapshot decoders |

## Responsible Vulnerability Disclosure (`LIN-1794`)

Please **do not** open public GitHub issues with exploit payloads for unpatched memory-safety, path-traversal, credential-leak, or remote-replication vulnerabilities.

1. Report privately via GitHub Security Advisories (`Security -> Report a vulnerability`) on the repository, including:
   - Affected commit hash and enabled Cargo feature flags (`lin version --capabilities`).
   - Minimal deterministic reproducer (query, WAL/snapshot fixture, or replication frame).
   - Impact classification (corruption, DoS / resource-limit bypass, unauthorized file/network access, or information disclosure).
2. Maintainers acknowledge receipt within **72 hours**, validate with a minimized regression test under `tests/`, and coordinate fix disclosure once a patched release tag is published.

## Engine Security & Isolation Model (`D45`)

- **Memory Safety:** Core storage, parser, planner, query execution, WAL/snapshot persistence, and replication contain zero ambient `unsafe` APIs; OS syscalls (`fcntl`, `fdatasync`, `fallocate`, `mmap`) are isolated behind checked boundaries.
- **Capability Policy (`lin::security::CapabilityPolicy`):**
  - Supports read-only handles (`ReadDb` and `CapabilityPolicy::read_only()`), per-collection read/write/DDL allowlists, explicit network-embedder permission gates (`allow_network_models`), and canonicalized directory sandboxes (`validate_sandboxed_path`) rejecting `..` traversal, NUL bytes, and out-of-root symlinks (`LIN-1777`..`LIN-1788`).
- **Resource Limits & Receipts:**
  - Enforces bounded parser recursion, escaped-string expansion caps, WAL/snapshot frame & section byte limits, per-query scan/group/join/sort row & memory budgets, and emits structured `ResourceLimitReceipt` diagnostics (`LIN-1785`).
- **Credential & Path Redaction (`lin::error` / `lin::telemetry`):**
  - Connection URIs, `Bearer` tokens, `password=`/`token=`/`secret=` parameters, and host home paths are redacted before entering error attachments, telemetry spans, or slow-query ring buffers (`LIN-1570`, `LIN-1572`, `LIN-1619`).
- **Replication Transport (`lin::ship`):**
  - Supports TLS (`rustls` + `ring`) and constant-time bearer-token verification with bounded frame headers and handshake failure categorization (`LIN-1373`..`LIN-1387`).
- **Supply Chain & Provenance (`LIN-1795`, `LIN-1945`..`LIN-1948`):**
  - `python3 scripts/sbom-and-provenance.py --verify` validates dependency licenses, optional feature isolation, and SHA-256 artifact manifests from `cargo metadata --locked`.
