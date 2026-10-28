# Native bulk phase diagnostic: preparation

Frozen profile reference a7d4b02c3 (COW+single-statementCAS),excluding pending allocation-free index andDELETE-WAL experiments. All production sources remain unchanged. Instrumentation lives in separate exec/lib/index overlays plus a temporary profile module;every overlay must restore current source hashes.

The unchanged full-engine compare harness supplies identical bulk1k/10kfixtures,default embedder,scalar index,FTS,andfullvalue validation. Only the diagnostic copy wraps fill_lin with trace start/finish andflushes events after the suite. Input setup/drop andvalidation remain outside the ordinary timer. The temporary execution Handle is dropped inside fill_lin,as in the original source.

Nine child phase names:row_build,id_hash_fk,embedding,edges,indexes_reserve,fts,hnsw,row_maps,store_install. Clock scopes are sequential;row_build includes both construction threads andjoin. Index0isunused/reserved andmust remainzero. Phase sums may be smaller than the outer prepared-execution wall due to uninstrumented work;never force them to100%. No nested-parent/child double counting.

Ten focused release record-construction regressions passed withinstrumented source andtracing disabled. Current sources restored exactly. Full instrumented release suite,active trace count/full-value smokes,original/control/disabled/traced observer calibration andLinux raw qualification are still pending. No phase measurements,speedup orfull-engine acceptance are claimed at this preparation stage. Trace invocation records include validation/calibration andmust not be presented as individually aligned with measured benchmark observations.
