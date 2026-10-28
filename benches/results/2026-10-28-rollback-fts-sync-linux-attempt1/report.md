# FTS qualification attempt1: infrastructure failure, no timing matrix

Frozen2cc359505, run40649504291 failed before timing. Full Linux release454tests/2ignored, missing-FTS runtime negative and both ordinary builds passed. The fresh smoke script then raised FileExistsError before its loop because the checkout already contained the committed local smoke directory. The 288-process timing, analysis and gate steps were skipped.

No fresh Linux smoke and no timing process/observation was produced. The uploaded package includes previously committed macOS smoke outputs; they are retained as historical local evidence and are not counted as Linux results. No component win or loss can be inferred from this attempt.

All files and the failure log are retained with SHA256 and a verified archive. The repair writes fresh checks and their status to qualification-smoke/qualification-smoke-status.json, leaving historical smoke data in place. Only the output namespace changes; runtime2e5146fce, baseline21dcd7d81, fixtures, sample matrix and gate thresholds remain unchanged. A new frozen run is an infrastructure correction after zero timing measurements, not a retry of a losing performance result.
