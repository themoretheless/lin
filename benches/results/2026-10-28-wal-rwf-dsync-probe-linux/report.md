# Synchronized-prefix RWF_DSYNC probe: rejected

Run40614973547, frozen c3f735b7a. Independent frozen-source/raw audit passed:432fresh processes/3456observations,24Linux golden/replay smokes,4error-path unit tests. No production source/WAL format/sync-mode edits were made.

All16Full target windows failed the frozen6/6positive plus median-above-A/A criterion. One of8Normal control windows failed. Full median changes range from-1.4354% to+3.1117%; no consistent I/O speedup is established. No unchanged rerun or production integration. All failed pairs and startup observations remain in the archive.

|Series|Sync|Shape|Commits|Gain %|A/A %|Positive pairs|Pass|
|---|---|---|---:|---:|---:|---:|---|
|1|full|scalar|1|2.4709|3.5572|3/6|False|
|1|full|scalar|32|0.7696|3.9836|4/6|False|
|1|full|doc_update|1|0.1467|6.7468|3/6|False|
|1|full|doc_update|32|1.8205|1.4819|5/6|False|
|1|full|doc_delete|1|0.5682|1.8215|3/6|False|
|1|full|doc_delete|32|-0.9363|0.8386|1/6|False|
|1|full|doc_batch|1|2.5709|0.7213|4/6|False|
|1|full|doc_batch|32|0.7284|2.7021|4/6|False|
|1|normal|scalar|1|-3.0284|2.4611|1/6|False|
|1|normal|scalar|32|-0.0153|0.4743|3/6|True|
|1|normal|doc_update|1|0.5344|9.4612|3/6|True|
|1|normal|doc_update|32|-0.9254|0.9813|2/6|True|
|2|full|scalar|1|-1.4354|1.8937|3/6|False|
|2|full|scalar|32|0.4182|2.4010|3/6|False|
|2|full|doc_update|1|3.1117|5.5278|5/6|False|
|2|full|doc_update|32|-0.2278|1.5910|3/6|False|
|2|full|doc_delete|1|-1.2933|6.1590|2/6|False|
|2|full|doc_delete|32|-1.1185|2.5583|3/6|False|
|2|full|doc_batch|1|-1.0155|3.7839|2/6|False|
|2|full|doc_batch|32|1.7682|1.3027|5/6|False|
|2|normal|scalar|1|6.0182|6.9072|5/6|True|
|2|normal|scalar|32|-0.0563|0.8866|3/6|True|
|2|normal|doc_update|1|-1.5043|11.0833|3/6|True|
|2|normal|doc_update|32|1.0654|1.0737|4/6|True|

The kernel accepted RWF_DSYNC and all measured paths passed exact canonical WAL bytes/file-position/length/replay/generation/float-bit/corruption checks. The premise was an explicitly synchronized valid prefix, not arbitrary dirty data. No partial/error case is silently counted as success.

The host reports ext4 with nobarrier,data=writeback,journal_async_commit andcommit=30 (linux-environment.txt). These observations describe API synchronization cost on this hosted runner; they do not establish hardware power-loss durability. No unique causal explanation for the timing variation is established. No filesystem or kernel policy was changed by the probe.

Encoding,fixture setup,prefix/directory sync,replay,validation and cleanup are outside the measured write loop. Single-commit and32-commit average contracts stay separate. No DB execution,checkpoint,end-to-end peer,RSS or production durability qualification. The original eight-peer goal remains incomplete.
