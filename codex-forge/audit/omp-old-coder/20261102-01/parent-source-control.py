#!/usr/bin/env python3
import importlib.util,json,pathlib,sys
base=pathlib.Path(__file__).resolve().parent
spec=importlib.util.spec_from_file_location('review_gauntlet',base/'gauntlet.py');module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
expected=json.loads((base/'source-state.json').read_text())['files_sha256']
if '--corrupt-expected' in sys.argv:
 expected['src/dsh.rs']='0'*64
print(module.verify(expected))
