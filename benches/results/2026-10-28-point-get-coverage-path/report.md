# Point-get coverage call-path diagnostic

Baselinea7d4b02c3 versus rejected allocation-free candidate884ebeb81. Both sources compile withidentical counter hooks atindex_covers_pred entry andanidentical temporary hidden counter accessor. The unchanged full-engine fixture builder seeds10kdocs,includingtheusualindex/embedder/FTS/data checks. Prepared query definitions are reused fromthefrozen compare source. The original suite main is unused;thecounter driver executes1000queries afterone reference query andcompares allreturned-row bytes oneachoperation.

12fresh processes:2sources×3queries×2repeats. Both repeats match.

|Source|Query|Prepared operations|Coverage calls|
|---|---|---:|---:|
|baseline|point_get|1000|0|
|baseline|filter_eq|1000|1000|
|baseline|filter_range|1000|1000|
|candidate|point_get|1000|0|
|candidate|filter_eq|1000|1000|
|candidate|filter_range|1000|1000|

Inthis bounded source/fixture pair,point_get never enters the changed coverage helper;filter_eqandfilter_range eachenteronceperquery. This rules outdirect helper execution cost asanexplanationoftherecordedpoint-getregression. Itdoesnotidentifycompiler/code-layout/host effects asthecause anddoesnotjustifyacceptingtherejectedcandidate. No latency,RSS,allocation count orpeer victory is measured here. Counter-instrumented binaries must not beusedforperformance acceptance.

Source inspection also confirms thatPrepared::run takes prepared.point_get andis_docs_id_title specialization directly,using Store::point_get_docs_id_title. Itdoesnot executethegeneric try_point_get field_names conversion forthebenchmarkquery. Eliminatingthatgeneric conversion thereforecannot beclaimedasanimprovementtothispreparedpoint-getcase.

Every original library,index andbenchmark source was restored exactly;theproduction API remains unchanged. Probe/library/index/binary hashes areretainedinprovenance.json. ThemacOScounter run isnotaninstrumentedLinuxrerunoftheperformancegate;theoriginalLinuxfailure remains authoritative.
