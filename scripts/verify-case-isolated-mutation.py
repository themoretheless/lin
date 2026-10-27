#!/usr/bin/env python3
"""Independent complete-result audit for the case-isolated mutation protocol."""
import ast,json,math,statistics,sys
from pathlib import Path

def verify(p):
    cases=next(ast.literal_eval(n.value)for n in ast.parse((p/'run.py').read_text()).body if isinstance(n,ast.Assign)and any(isinstance(t,ast.Name)and t.id=='CASES'for t in n.targets))
    assert len(cases)==len(set(cases))==20
    expected={(i,mode,case)for i in range(1,13)for mode in ['baseline','control','candidate']for case in cases}
    status=json.loads((p/'process-status.json').read_text())
    assert len(status)==720 and all(r['exit_code']==0 for r in status)
    assert {(r['round'],r['mode'],r['case'])for r in status}==expected
    groups=json.loads((p/'status.json').read_text())
    assert len(groups)==36 and {r['run']for r in groups}=={f'{i:02}-{m}'for i in range(1,13)for m in ['baseline','control','candidate']}
    assert all(r['exit_code']==0 and r['subprocesses']==20 for r in groups)
    medians={};count=0
    for i,mode,case in sorted(expected):
        d=json.loads((p/f'{i:02}-{mode}'/case.replace('/','_')/'run.json').read_text())
        assert d['status']=='complete' and len(d['cases'])==1 and len(d['observations'])==8
        samples=[]
        for o in d['observations']:
            assert o['case']==case and o['metric']=='wall' and o['operations']==1
            v=float(o['value']);assert math.isfinite(v)and v>0;samples.append(v);count+=1
        medians[i,mode,case]=statistics.median(samples)
    s=json.loads((p/'summary.json').read_text());assert set(s)==set(cases)
    accepted=True;wins=0;comparisons=0
    for case in cases:
        assert set(s[case])=={'series1','series2'}
        for label,rounds in [('series1',range(1,7)),('series2',range(7,13))]:
            group=s[case][label];assert group['pairs']==6 and len(group['processes'])==6
            assert {r['round']for r in group['processes']}==set(rounds)
            gains=[];aas=[];peer_wins=0
            for i in rounds:
                a,b,c=(medians[i,m,case]for m in ['baseline','control','candidate']);gain=100*(1-c/a);aa=100*(1-b/a);gains.append(gain);aas.append(abs(aa))
                r=next(r for r in group['processes']if r['round']==i)
                assert (r['baseline_ns'],r['control_ns'],r['candidate_ns'])==(a,b,c)
                assert math.isclose(r['gain_percent'],gain,abs_tol=1e-10)and math.isclose(r['aa_percent'],aa,abs_tol=1e-10)
                if case.endswith('/lin'):
                    peer=medians[i,'candidate',case[:-3]+'sqlite'];win=c<peer
                    assert r['candidate_sqlite_ns']==peer and r['wins_sqlite']==win;peer_wins+=win;comparisons+=1
            gain=statistics.median(gains);noise=statistics.median(aas);positive=sum(v>0 for v in gains)
            assert math.isclose(group['gain_percent'],gain,abs_tol=1e-10)and math.isclose(group['aa_absolute_percent'],noise,abs_tol=1e-10)and group['positive_pairs']==positive
            if case.endswith('/lin'):assert group['sqlite_wins']==peer_wins;wins+=peer_wins
            accepted &= (positive==6 and gain>noise)if case.startswith('compare/update_1row_')and case.endswith('/lin')else(gain>=-max(2,noise))
    q=json.loads((p/'qualification.json').read_text());assert q['complete_rounds']==12 and q['candidate_qualified']==bool(accepted)and q['full_objective_achieved']is False
    assert count==5760 and comparisons==120
    return dict(processes=720,observations=count,comparisons=240,candidate_sqlite_wins=wins,candidate_sqlite_comparisons=comparisons,candidate_qualified=bool(accepted),full_objective_achieved=False)
if __name__=='__main__':print(json.dumps(verify(Path(sys.argv[1])),indent=2))
