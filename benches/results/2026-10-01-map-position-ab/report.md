# Identity-map position reuse: rejected

Candidate retains existing moved-row ID/URI/SPO keys and updates positions, relying on column arrays that already followed swap_remove. Three alternating independent prebuilt process pairs, 24 fresh fixtures per case. Setup/destruction and affected-count/readback checks are outside timing.

| Pair | Rows | Baseline Lin µs | Candidate Lin µs | Baseline SQLite µs | Candidate SQLite µs |
|---|---|---:|---:|---:|---:|
| 1 | 1k | 7.854 | 7.562 | 2.896 | 4.000 |
| 1 | 10k | 13.875 | 14.521 | 9.812 | 12.521 |
| 1 | 100k | 23.896 | 19.709 | 16.750 | 15.959 |
| 2 | 1k | 6.521 | 7.438 | 3.208 | 4.604 |
| 2 | 10k | 13.521 | 15.104 | 10.042 | 10.687 |
| 2 | 100k | 21.354 | 20.062 | 16.834 | 15.938 |
| 3 | 1k | 6.250 | 5.084 | 3.583 | 3.604 |
| 3 | 10k | 13.771 | 14.459 | 10.396 | 12.625 |
| 3 | 100k | 20.645 | 21.541 | 17.438 | 17.729 |

Not retained: 1k and 100k won two pairs each, but 10k lost all three. SQLite is still faster in all measured matched deletion cases. Controls also fluctuate, so no causal regression percentage is claimed. Workspace tests and all sampled validators passed. The prior retained inline index/bound source was restored exactly; this report does not establish a universal peer win. Sources and raw observations remain.
