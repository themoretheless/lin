# Allocation-free atomic coverage: rejected component

Run40620340816,frozen182d1204d. Independent frozen-source/raw audit passed:288fresh processes/2304observations,452Linux release tests/2ignored,16fresh correctness smokes,numeric-precision mutant rejected andallocation shim excluded. Bothfilter_eqtarget windows passed,but2control windows failed. No production adoption orunchanged rerun.

|Case|Series|Gain %|Absolute A/A %|Positive pairs|Pass|
|---|---|---:|---:|---:|---|
|compare/filter_eq/lin|series1|10.6756|2.2074|6/6|True|
|compare/filter_eq/lin|series2|9.7391|1.2510|6/6|True|
|compare/filter_range/lin|series1|2.4758|4.8185|5/6|True|
|compare/filter_range/lin|series2|-2.1044|4.5756|0/6|True|
|compare/point_get/lin|series1|-5.1305|2.0078|0/6|False|
|compare/point_get/lin|series2|-2.8514|3.9865|3/6|True|
|compare/text_substr/lin|series1|0.9662|0.5818|5/6|True|
|compare/text_substr/lin|series2|0.5034|0.5051|5/6|True|
|compare/update_1row_1k/lin|series1|10.6646|6.6107|5/6|True|
|compare/update_1row_1k/lin|series2|2.0490|8.7496|3/6|True|
|compare/wal_scalar_full_update/lin|series1|-0.9792|10.1433|3/6|True|
|compare/wal_scalar_full_update/lin|series2|-4.1999|6.8586|1/6|True|
|compare/filter_eq/sqlite|series1|0.3829|1.8621|4/6|True|
|compare/filter_eq/sqlite|series2|-0.2689|0.7720|3/6|True|
|compare/filter_eq/duckdb|series1|-0.7423|14.9471|1/6|True|
|compare/filter_eq/duckdb|series2|-2.8046|1.4351|1/6|False|

filter_eqgains10.6756%/9.7391%passed6/6aboveA/A2.2074%/1.2510%. Point_getseries1failed-5.1305%againstA/A2.0078%,0/6positive pairs. UnchangedDuckDBfilter_eqseries2failed-2.8046%againstA/A1.4351%. No unique cause is established;thepoint-getregression cannot be explained away bytheunchanged-peer failure.

The24-process System allocation diagnostic (leaf6000→0,dotted7000→1000,partial6000→0,compoundunchanged) remains bounded allocation evidence. Neitherthose counts northisreliablefilter_eqtarget gain qualifythewholecomponent. Preserve allfailed pairs/startup observations andsource/build/test/smoke assets inraw-artifact.tar.gz;everymemberhashverified.

The index core is restored to preceding COW+CAS a7d4b02c3 after this rejection. Exactcandidate/test/mutant evidence remains inGit/rawassets. The already-running bulk phase diagnostic is pinned separately at0ea1c38c3andprofilesa7d4b02c3withtemporaryoverlays;its checkout-restore metadata must be interpreted againstthat frozenhead,not a later localcheckout. Full-source/eight-peer acceptance remains false.
