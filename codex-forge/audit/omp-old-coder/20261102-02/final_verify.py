#!/usr/bin/env python3
"""Re-run bounded review-gap validation. Success is not complete Tier 3 assurance."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

BASE = Path(__file__).resolve().parent
WORK = BASE / 'final-workspace/btae-eval'


def check_source(expected):
    actual = {str(p.relative_to(WORK)): hashlib.sha256(p.read_bytes()).hexdigest()
              for p in sorted(WORK.rglob('*'))
              if p.is_file() and 'target' not in p.relative_to(WORK).parts}
    if actual != expected:
        raise RuntimeError('source manifest mismatch')


def main():
    state = json.loads((BASE / 'final-source-state.json').read_text())
    if not WORK.exists():
        original = BASE.parents[2] / 'btae-eval'
        for name, expected in state['files_sha256'].items():
            src = original / name
            if hashlib.sha256(src.read_bytes()).hexdigest() != expected:
                raise RuntimeError(f'cannot reconstruct expected source: {name}')
            dst = WORK / name
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(src, dst)
    check_source(state['files_sha256'])
    if sys.argv[1:] == ['--verify-only']:
        print(state['tree_sha256'])
        return
    out = (BASE / sys.argv[1]).resolve()
    if not out.is_relative_to(BASE / 'output'):
        raise RuntimeError('output must be inside audit/output')
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ)
    env['CARGO_TARGET_DIR'] = str(BASE / 'target')
    commands = [
        ('tests', ['cargo', 'test', '--all-targets', '--locked', '--offline', '-j', '2', '--', '--test-threads=4'], 0),
        ('lint', ['cargo', 'clippy', '--all-targets', '--locked', '--offline', '--', '-D', 'warnings'], 0),
        ('format', ['cargo', 'fmt', '--all', '--check'], 0),
        ('readiness', ['cargo', 'run', '--locked', '--offline', '--', 'check-ready', 'datasets/pilot.json'], 1),
    ]
    results = []
    for name, command, expected_exit in commands:
        run = subprocess.run(command, cwd=WORK, env=env, capture_output=True, text=True)
        (out / (name + '.stdout.txt')).write_text(run.stdout)
        (out / (name + '.stderr.txt')).write_text(run.stderr)
        results.append({'layer': name, 'command': command, 'actual_exit': run.returncode, 'expected_exit': expected_exit})
        (out / 'commands.json').write_text(json.dumps(results, indent=2) + '\n')
        if run.returncode != expected_exit:
            raise RuntimeError(f'{name}: unexpected exit {run.returncode}')
        print(name, run.returncode, flush=True)
    mutation_out = out / 'mutations'
    mutation = subprocess.run(['python3', str(BASE / 'final_mutations.py'), str(mutation_out)],
                              cwd=BASE, capture_output=True, text=True)
    (out / 'mutations.stdout.txt').write_text(mutation.stdout)
    (out / 'mutations.stderr.txt').write_text(mutation.stderr)
    if mutation.returncode != 0:
        raise RuntimeError('mutation layer did not pass')
    mutants = json.loads((mutation_out / 'results.json').read_text())
    if len(mutants) != 4 or any(m['outcome'] != 'KILLED' for m in mutants):
        raise RuntimeError('missing or surviving mutants')
    check_source(state['files_sha256'])
    expected_layers = ['tests', 'lint', 'format', 'readiness']
    if [r['layer'] for r in results] != expected_layers:
        raise RuntimeError('layer manifest mismatch')
    (out / 'summary.json').write_text(json.dumps({
        'source_tree_sha256': state['tree_sha256'], 'bounded_validation_passed': True,
        'layers': results, 'manual_mutants_killed': 4, 'manual_mutants_total': 4,
        'full_old_coder_assurance': False, 'independent_verification': 'not performed', 'formal_pilot': 'NOT_RUN',
        'limits': ['no line/branch coverage threshold', 'no fuzzing/proptest',
                   'no random-order test run', 'no minimum-Rust-version matrix',
                   'no human preimplementation SPEC approval'],
    }, indent=2) + '\n')
    print('BOUNDED_REVIEW_GAP_VALIDATION_PASSED; full Tier 3 assurance remains unavailable')


if __name__ == '__main__':
    main()
