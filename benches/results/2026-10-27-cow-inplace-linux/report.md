# COW in-place Linux qualification: stopped before measurements

Run40427160286,frozen headd01e0fe3026b1d24dc12b98429cc0670b9b2ec47. Candidate test step failed in tests/persist.rs checkpoint_compacts_log: second Db::open after explicit close returned "data dir locked by another writer". Full tests.log and failed-step.log retained. No paired timing series or speed qualification produced; do not describe this as a measured performance regression or win.

Current source review: test tmp() uses PID+atomic sequence,so a simple same-process duplicate directory is not established. Db::close takes Persist and relies on dropping File to release flock. Other tests in same integration binary spawn child processes. On Unix inherited open-file descriptions can temporarily retain a flock before exec/exit; this is a hypothesis requiring a controlled reproducer,not a proven cause of this CI failure. Do not silently serialize tests,add sleep,retry the same job or waive the failing test to obtain acceptance.

Next action: reproduce the close/reopen lock behavior independently on baseline and candidate with controlled child descriptor lifetime. Candidate remains experimental;407macOS tests and allocation counts do not replace failed Linux correctness qualification. Production source unchanged; all peer/read/native/durable/API scope remains open.
