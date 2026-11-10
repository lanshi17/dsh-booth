#!/usr/bin/env python3
"""Known-bad controls for final custom gates; neither edits the candidate."""
from pathlib import Path
import hashlib
import json
import sys
import tempfile
import final_verify
import final_mutations

BASE = Path(__file__).resolve().parent
out = BASE / 'output/checker-controls'
out.mkdir(exist_ok=False)
records = []
with tempfile.TemporaryDirectory(dir=BASE / 'mutants') as td:
    w = Path(td)
    (w / 'probe').write_bytes(b'good')
    expected = {'probe': hashlib.sha256(b'good').hexdigest()}
    old = final_verify.WORK
    final_verify.WORK = w
    final_verify.check_source(expected)
    (w / 'probe').write_bytes(b'bad')
    try:
        final_verify.check_source(expected)
    except RuntimeError as e:
        if str(e) != 'source manifest mismatch':
            raise
        records.append({'control': 'tampered-source', 'rejected': True, 'error': str(e)})
    else:
        raise RuntimeError('tampered source accepted')
    finally:
        final_verify.WORK = old
# A harmless comment edit compiles and leaves the tests green. The gate must reject it.
final_mutations.CASES = [('equivalent-comment', '// Allow one trailing', '// Allow a single trailing',
                         'exact_line_byte_limit_accepts_content_before_a_later_lf')]
sys.argv = [__file__, 'output/checker-controls/equivalent']
try:
    final_mutations.main()
except RuntimeError as e:
    if 'did not demonstrate a behavioral test failure' not in str(e):
        raise
    stdout = (out / 'equivalent/equivalent-comment.stdout.txt').read_text()
    if 'test result: ok. 27 passed' not in stdout:
        raise RuntimeError('equivalent control did not run the passing suite')
    records.append({'control': 'surviving-equivalent-edit', 'rejected': True, 'error': str(e), 'adapter_tests_passed': 27})
else:
    raise RuntimeError('equivalent edit counted as a killed mutant')
(out / 'results.json').write_text(json.dumps(records, indent=2) + '\n')
print('2/2 known-bad checker controls rejected; this does not prove checker completeness')
