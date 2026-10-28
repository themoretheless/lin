# BTreeMap CAS Linux component decision

Frozen-source/raw audit passed for144processes/1152wall observations;436release tests/2ignored and8fresh correctness smokes passed. Full median gains41.383%/42.047%,Normal93.836%/93.597%;all control windows passed. Overall component gate failed because Full series1 had5/6positive pairs,below the predefined6/6 requirement.

Losing round1:baseline median400528.5ns,control409121.5ns,candidate8489982.5ns. All eight raw observations retained. Baseline had one11.24ms sample;control had two11.25–11.74ms samples;candidate had five6.45–13.16ms samples and three0.205–0.224ms samples. This shows substantial within-process variation across all roles;no unique cause or kernel/storage mechanism is established by these records. No sample or process is excluded,threshold relaxed or unchanged qualification rerun requested. Median gains alone do not establish acceptance.

The API-compatible candidate is not adopted. Restore src/exec.rs byte-for-byte to corrected BTreeMap6812cc5ca while retaining all behavior/API regressions,prototype source and raw evidence. The independent combined COW/CAS Linux67-case run remains separate and active. The full eight-peer objective remains open.
