# Bounded prefix emptiness incremental warm diagnostic

Six rotated A/A/B rounds,18processes;16samples of512prepared alternating updates and one rollback per size; exact readback every sample. Both variants are integrated COW on main047; only index emptiness changes. UPDATE1k/10k/100k -0.017/+3.172/+12.620%, A/A1.957/2.149/1.426%, positive3/6,6/6,6/6. Rollback -0.720/+3.042/+8.163%, A/A4.226/2.253/1.813%. All raw samples/outliers retained. Source hashes prove incremental source scope. These warm results do not qualify fresh-fixture or peers.
