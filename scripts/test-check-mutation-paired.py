#!/usr/bin/env python3
import copy,json,subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
class Gate(unittest.TestCase):
 def fixture(self):
  names=[f'compare/{kind}_1row_{size}/{peer}'for kind in ['update','delete']for size in ['1k','10k','100k']for peer in ['lin','sqlite']]+[f'compare/tx_{kind}_1row_{size}/{peer}'for kind in ['commit','rollback']for size in ['1k','10k']for peer in ['lin','sqlite']]
  return {name:{series:dict(pairs=6,gain_percent=10 if '/update_'in name and name.endswith('/lin')else 0,aa_absolute_percent=1,positive_pairs=6,sqlite_wins=0)for series in ['series1','series2']}for name in names}
 def run_gate(self,data,status_count=36):
  with tempfile.TemporaryDirectory()as directory:
   p=Path(directory);(p/'summary.json').write_text(json.dumps(data));(p/'status.json').write_text(json.dumps([dict(exit_code=0)]*status_count));result=subprocess.run(['python3',str(ROOT/'scripts/check-mutation-paired.py'),str(p)],capture_output=True,text=True);q=json.loads((p/'qualification.json').read_text())if(p/'qualification.json').exists()else None
   return result.returncode,q
 def test_complete_gain_is_scoped(self):
  code,q=self.run_gate(self.fixture());self.assertEqual(code,0);self.assertFalse(q['full_objective_achieved'])
 def test_target_loss_fails(self):
  data=self.fixture();data['compare/update_1row_1k/lin']['series1']['gain_percent']=-1;self.assertEqual(self.run_gate(data)[0],3)
 def test_target_below_noise_fails(self):
  data=self.fixture();data['compare/update_1row_1k/lin']['series1']['aa_absolute_percent']=15;self.assertEqual(self.run_gate(data)[0],3)
 def test_unchanged_delete_regression_fails(self):
  data=self.fixture();data['compare/delete_1row_1k/lin']['series1']['gain_percent']=-10;self.assertEqual(self.run_gate(data)[0],3)
 def test_incomplete_runs_invalid(self):self.assertNotEqual(self.run_gate(self.fixture(),35)[0],0)
 def test_partial_series_invalid(self):
  data=self.fixture();data['compare/update_1row_1k/lin']['series1']['pairs']=5;self.assertNotEqual(self.run_gate(data)[0],0)
if __name__=='__main__':unittest.main()
