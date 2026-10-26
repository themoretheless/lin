# Native SQL Server CI qualification

[Run40274823060](https://github.com/themoretheless/lin/actions/runs/40274823060)
completed against frozen8b0e9717 on native Linux x86_64. Real SQL Server2022
Developer, prepared/API reads and native insertion at1k/10k/100k, three independent
process repetitions each. Exact result validation, source/binary consistency,
sentinel preservation and owned-container cleanup passed.

This public report intentionally contains no comparative performance outcome.
SQL Server2022 Developer license section5 requires Microsoft's prior written
approval for third-party disclosure; see the linked terms in
[benchmark contract](../../../docs/benchmark-contract.md#private-sql-server-qualification-artifacts).
Job success means functional completeness, not a performance win.

Artifact `native-mssql-40274823060` contains the age-encrypted raw results and
a non-sensitive build log. Ciphertext SHA256 and execution-only checks are in
`run-40274823060-public-manifest.json`. The archive was downloaded, decrypted
and all saved repetitions audited locally; detailed comparisons stay in the
owner's ignored private directory. The private identity is never committed.
The artifact from this first encrypted run has nested paths because its build
log and encrypted file had different source directories; find the `.age` file
inside the downloaded artifact before decrypting. Later uploads separate them.

Attempt1 failed compilation before any server measurements. Attempt2 was
cancelled before artifact upload, with its log retained privately and removed
publicly. Those attempts are not completed benchmark evidence. This completed
run is not a measurement of subsequent production changes and does not close
SQLite durable1k or real Kusto qualification.
