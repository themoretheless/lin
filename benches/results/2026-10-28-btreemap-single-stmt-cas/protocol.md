# API-compatible BTreeMap single-statement CAS candidate

Independent portability/attribution experiment on corrected BTreeMap6812cc5ca9fc35a4adc81e0948df45dfe3629742. Only exec_program need_snap condition changes: require more than one statement before whole-DB hash snapshot and cross-statement written tracking. Exact public Row=BTreeMap alias, original row/update/undo/index implementation, WAL writer, schema/transaction corrections, sync, quota and complete Handle/Pack payload stay unchanged. No COW or other rejected performance variants ported.

Purpose: qualify the discovered whole-DB CAS snapshot cost independently of the next-API COW representation. This does not replace the separate combined COW/CAS full Linux qualification or the eight-peer objective. Four public behavior regressions retained from the COW candidate; an extra compile/runtime integration checks implicit BTreeMap-to-Row identity using an actual CAS result.

First gates: exact-source release functional suite and runtime rejection of an incorrect multi-statement snapshot bypass. Then fresh paired scalar Full/Normal and native/read/durable/peer controls. COW component gains are not transferred to this source. No performance measurement or adoption is claimed yet. Existing untracked prior benchmark data in this checkout are preserved.

Planned component measurement: unchanged67-case compare.rs used only for four selected cases; scalar Full/Normal targets and native UPDATE1k/100k controls. Two six-pair series;baseline/control same executable,candidate separate;rotated case/role order;eight observations,max-iterations1,no warmup.144fresh processes/1152observations. Target requires6/6positive pairs and median gain above median absolute A/A; controls gain>=-max(2%,A/A). No peers,RSS or full-engine qualification from this component.

Measurement host chosen before any timing: fresh native Linux x86_64 runner. A local process inventory found unrelated active Rust compilations; no local performance observations were started. Local functional tests/builds remain valid; their duration is not performance evidence. Analyzer records the actual host from environment.json.
