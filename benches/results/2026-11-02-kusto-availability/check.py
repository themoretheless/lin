from pathlib import Path
import os,json,subprocess,datetime
p=Path(__file__).resolve().parent;names=['LIN_BENCH_KUSTO_URL','LIN_BENCH_KUSTO_DATABASE','LIN_BENCH_KUSTO_TOKEN'];status=dict(observed_at_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),local_environment_present={n:bool(os.environ.get(n))for n in names},no_secret_values_recorded=True,github_secret_metadata_verified=False,github_secret_list_complete=False)
r=subprocess.run(['gh','api','--paginate','--slurp','repos/themoretheless/lin/actions/secrets?per_page=100'],capture_output=True,text=True)
if r.returncode==0:
 pages=json.loads(r.stdout);totals={page['total_count']for page in pages};secrets=[s['name']for page in pages for s in page['secrets']];assert len(totals)==1 and len(secrets)==len(set(secrets))==next(iter(totals));status.update(github_secret_metadata_verified=True,github_secret_list_complete=True,github_required_secret_names_present={n:n in secrets for n in names},github_secret_metadata_pages=len(pages))
else:status['github_metadata_error']='metadata request failed; no absence conclusion from this attempt'
status['actual_backend_benchmark_executed']=False
(p/'status.json').write_text(json.dumps(status,indent=2)+'\n');print(json.dumps(status))
