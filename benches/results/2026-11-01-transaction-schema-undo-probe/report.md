# Schema after row mutation: six reproduced rollback failures

Exact corrected baseline 21dcd7d81354aa2f59038aa6cb142c86cd1cd1f5 was compiled on macOS ARM64. All src/*.rs files matched that Git reference before compilation. The owned temporary example was removed and the original experimental source restored in finally. Compiler log, six JSON outcomes, source hashes and runner are retained.

Both explicit rollback and transaction Drop reproduce each defect:

- INSERT then schema declaration retains the inserted row (three docs remain instead of two).
- UPDATE then schema declaration restores rows and Catalog but retains the new collection in Store.extra_collections.
- DELETE then UNIQUE index creation leaves the unique index active after rollback. A duplicate insert that should succeed fails with runtime error: unique index docs[title]: duplicate key.

The executable exited 3 because all six semantic checks failed; this is reproduced runtime evidence, not a build failure or a timing measurement. Undo::Append promotes to a full snapshot taken after earlier appends and loses its initial append mark. Undo::Rows and Undo::Mixed never snapshot later schema/unsupported mutations. The pending UPDATE patch timing job is pinned independently and cannot qualify whole-engine correctness or production adoption.
