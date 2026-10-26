# Deterministic mutation allocation gate

Original core versus borrowed-row candidate, three independent processes per binary, nine cases each (wing/title/delete at1k/10k/100k). All54 fresh-fixture operations passed exact affected-count/generation/row/index-count readback. Setup and validation outside counting; Handle destruction included. Counting System allocator calls/requested bytes is not latency, RSS or retained-memory measurement.

At every size/process: wing update71→51 allocations,4940→3776 requested bytes; title update83→63 allocations,6142→4978 bytes. Both save20 allocations and1164 bytes. Delete is unchanged68 allocations/4975bytes. Realloc counts as an allocation request; requested bytes are not peak/live memory. Do not convert allocation deltas to time.

Initial fixture attempted immutable docs.body and failed compile; first wing count/log/gate/manifest are retained under failed-immutable-body/. Revised fixture uses mutable title, exercising changed FTS fields. No failed measurement overwritten. Initial summary driver had an else-token SyntaxError before any writes; corrected without rerunning measurements.

Production source restored after builds. Candidate remains unaccepted for latency: local A/A/B showed large unchanged-control variation. A separate Linux runner must qualify the full operation before rollout.
