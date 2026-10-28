from pathlib import Path
import subprocess,json,hashlib,argparse
ROOT=Path(__file__).resolve().parents[1];REF='d021bf29a21f1b91a626153063c6f1fa3df1aa98';SOURCE='benches/results/2026-10-28-hashing-fixed-array-output-component/compare.rs'
def audit():
 data=subprocess.check_output(['git','show',REF+':'+SOURCE],cwd=ROOT);text=data.decode();evidence={}
 for engine in ['lin','sqlite','duckdb','hashmap','postgres','mysql']:
  start=text.index('.bench_fixture("point_get/'+engine+'"');end=text.index('.tag("point_get")',start);block=text[start:end];assert 'bench_checked'not in block and 'validate'not in block
  if engine=='lin':assert '.done.n'in block
  elif engine in ['sqlite','duckdb']:assert 'SELECT COUNT(*)'in block and 'black_box(n)'in block
  elif engine=='hashmap':assert '.title.len()'in block
  else:assert 'black_box(n)'in block
  evidence[engine]=dict(line=text[:start].count('\n')+1,source=block,full_result_value_validation_registered=False)
 a=text.index('let point_get = db');b=text.index('let filter_eq = db',a);assert '{{ id, title }}'in text[a:b]
 assert 'SELECT COUNT(*)::bigint FROM docs WHERE id = $1'in text and 'SELECT COUNT(*) FROM docs WHERE id = ?'in text
 return dict(audit_complete=True,source_reference=REF,source_path=SOURCE,source_sha256=hashlib.sha256(data).hexdigest(),legacy_point_payloads_equivalent=False,lin_payload='id/title Row materialization, timed closure returns done.n',sql_payload='COUNT(*) scalar for SQLite/DuckDB/Postgres/MySQL',hashmap_payload='title byte length',read_smoke_scope='successful execution; does not compare point id/title values',does_not_invalidate_same_case_source_pair_timings=True,historical_observations_preserved=True,performance_qualified=False,evidence=evidence),data
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('result_directory',type=Path);args=parser.parse_args();q,data=audit();args.result_directory.mkdir(exist_ok=True,parents=True);(args.result_directory/'legacy-point-contract-audit.json').write_text(json.dumps(q,indent=2)+'\n');(args.result_directory/'legacy-compare.rs').write_bytes(data);print('Legacy point projection/count/title-length asymmetry confirmed; all original observations retained')
