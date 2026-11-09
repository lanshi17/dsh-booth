#!/usr/bin/env python3
"""Persisted baseline gauntlet. No successful verdict if a command or source check fails."""
import hashlib,json,pathlib,subprocess,sys
BASE=pathlib.Path(__file__).resolve().parent
WORK=BASE/'workspace/btae-eval'

def manifest():
 return {str(p.relative_to(WORK)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(WORK.rglob('*')) if p.is_file() and 'target' not in p.relative_to(WORK).parts}

def verify(expected):
 actual=manifest()
 if actual!=expected:raise RuntimeError('frozen source manifest mismatch')
 return hashlib.sha256(json.dumps(actual,sort_keys=True,separators=(',',':')).encode()).hexdigest()

if __name__=='__main__':
 spec=json.loads((BASE/'source-state.json').read_text())
 if sys.argv[1:]==['--verify-only']:
  print(verify(spec['files_sha256']));sys.exit(0)
 out=(BASE/sys.argv[1]).resolve();out.mkdir(parents=True,exist_ok=False)
 state=verify(spec['files_sha256'])
 assert state==spec['tree_sha256']
 cmds=[('tests',['cargo','test','--all-targets','--locked','--offline','-j','2','--','--test-threads=4']),('types',['cargo','check','--all-targets','--locked','--offline','-j','2']),('lint',['cargo','clippy','--all-targets','--locked','--offline','--','-D','warnings']),('format',['cargo','fmt','--all','--check'])]
 results=[]
 import os
 env=dict(os.environ);env['CARGO_TARGET_DIR']=str(BASE/'target')
 for name,cmd in cmds:
  with (out/(name+'.stdout.txt')).open('w') as stdout,(out/(name+'.stderr.txt')).open('w') as stderr:r=subprocess.run(cmd,cwd=WORK,env=env,stdout=stdout,stderr=stderr)
  results.append({'layer':name,'command':cmd,'exit_code':r.returncode});(out/'results.json').write_text(json.dumps({'source_tree_sha256':state,'layers':results},indent=2)+'\n')
  print(name,r.returncode,flush=True)
  if r.returncode!=0:sys.exit(r.returncode if r.returncode>0 else 1)
 assert [r['layer'] for r in results]==['tests','types','lint','format']
 verify(spec['files_sha256'])
 print('BASELINE_LAYERS_COMPLETE; NOT A FULL REVIEW VERDICT')
