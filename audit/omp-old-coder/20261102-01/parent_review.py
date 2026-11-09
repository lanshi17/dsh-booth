#!/usr/bin/env python3
"""Reproduce the review without changing the candidate. Exit 2 = surviving mutants."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

BASE = Path(__file__).resolve().parent
WORK = BASE / 'workspace/btae-eval'
ORIGINAL = BASE.parents[2] / 'btae-eval'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save_json(path, value):
    path.write_text(json.dumps(value, indent=2) + '\n')


def run(cmd, cwd, out, name, target=None, expected=0):
    env = dict(os.environ)
    if target is not None:
        env['CARGO_TARGET_DIR'] = str(target)
    proc = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    (out / (name + '.stdout.txt')).write_text(proc.stdout)
    (out / (name + '.stderr.txt')).write_text(proc.stderr)
    if proc.returncode != expected:
        raise RuntimeError(f'{name}: expected exit {expected}, got {proc.returncode}')
    return proc


def main():
    state = json.loads((BASE / 'source-state.json').read_text())
    # The product is currently untracked; reconstruct the exact explicit snapshot
    # if the ignored local copy is absent. Never use Git HEAD as its source state.
    for name, expected in state['files_sha256'].items():
        if sha(ORIGINAL / name) != expected:
            raise RuntimeError(f'original source changed: {name}')
        if not WORK.exists():
            break
    if not WORK.exists():
        for name, expected in state['files_sha256'].items():
            if sha(ORIGINAL / name) != expected:
                raise RuntimeError(f'original source changed: {name}')
            target = WORK / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ORIGINAL / name, target)

    out = (BASE / sys.argv[1]).resolve()
    if not out.is_relative_to(BASE / 'output'):
        raise RuntimeError('output must be inside this audit/output directory')
    out.mkdir(parents=True, exist_ok=False)
    run(['python3', str(BASE / 'gauntlet.py'), '--verify-only'], BASE, out, 'source-before')
    baseline = out / 'baseline'
    run(['python3', str(BASE / 'gauntlet.py'), str(baseline)], BASE, out, 'baseline-command')
    counts = re.findall(r'test result: ok\. (\d+) passed;', (baseline / 'tests.stdout.txt').read_text())
    if sum(map(int, counts)) != 54:
        raise RuntimeError('unexpected baseline test count; inspect evidence')

    harness_target = BASE / 'parent-harness-target'
    harness = BASE / 'harness'
    for mode in [1, 2, 3]:
        run(['cargo', 'run', '--manifest-path', str(harness / 'Cargo.toml'), '--locked',
             '--offline', '--bin', 'mutant_probe', '--', str(mode)], BASE, out,
            f'original-probe-{mode}', harness_target)
    expectations = {
        1: 'OK completed=true status=Complete steps=1/1',
        2: 'OK completed=false status=Unknown steps=0/0',
        3: 'ERR dsh event 4 violates the transcript protocol: turn_end while a step is open',
    }
    for mode, expected in expectations.items():
        if (out / f'original-probe-{mode}.stdout.txt').read_text().strip() != expected:
            raise RuntimeError(f'original behavior disagrees with review for probe {mode}')

    changes = [
        (1, 'if reasoning > output {', 'if reasoning >= output {',
         'ERR dsh self-reported token usage is invalid: reasoningTokens exceeds outputTokens at event 4'),
        (2, 'if total > self.limits.max_total_bytes as u64 {',
         'if total >= self.limits.max_total_bytes as u64 {',
         'ERR dsh transcript exceeds the 35-byte total limit'),
        (3, '                if self.open_step.is_some() {\n'
            '                    return Err(protocol(event, "turn_end while a step is open"));\n'
            '                }\n', '', 'OK completed=false status=Unknown steps=0/1'),
    ]
    mutants = []
    for mode, before, after, expected_probe in changes:
        candidate = out / 'mutants' / f'm{mode}'
        shutil.copytree(WORK, candidate)
        source = candidate / 'src/dsh.rs'
        text = source.read_text()
        if text.count(before) != 1:
            raise RuntimeError(f'mutant {mode}: replacement must match exactly once')
        original_hash = sha(source)
        source.write_text(text.replace(before, after, 1))
        mutant_hash = sha(source)
        if mutant_hash == original_hash:
            raise RuntimeError('mutant was not actually applied')
        example = candidate / 'examples/probe.rs'
        shutil.copy2(harness / 'src/bin/mutant_probe.rs', example)
        target = BASE / f'parent-mutant-target-{mode}'
        tests = run(['cargo', 'test', '--locked', '--offline', '--all-targets', '-j', '2',
                     '--', '--test-threads=4'], candidate, out, f'mutant-{mode}-tests', target)
        passed = sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', tests.stdout)))
        if passed != 54:
            raise RuntimeError(f'mutant {mode}: unexpected suite count')
        probe = run(['cargo', 'run', '--locked', '--offline', '--example', 'probe', '--', str(mode)],
                    candidate, out, f'mutant-{mode}-probe', target)
        if probe.stdout.strip() != expected_probe or probe.stdout.strip() == expectations[mode]:
            raise RuntimeError(f'mutant {mode}: concrete divergence not established')
        if sha(source) != mutant_hash:
            raise RuntimeError('mutant source changed while running')
        mutants.append({'id': f'm{mode}', 'outcome': 'SURVIVED', 'tests_passed': passed,
                        'original_sha256': original_hash, 'mutant_sha256': mutant_hash,
                        'concrete_original_output': expectations[mode],
                        'concrete_mutant_output': probe.stdout.strip(),
                        'manifest_path': str(candidate / 'Cargo.toml'),
                        'compile_and_test_log': f'mutant-{mode}-tests.stderr.txt'})
        save_json(out / 'mutants-progress.json', mutants)

    # Reproduce the low-severity source-comment mismatch independently.
    prefix = ('{"type":"session","sessionId":"parent-review"}\n'
              '{"type":"status","phase":"turn_start","turn":1}\n')
    opaque = '{"type":"thinking","unexpected_field":{"newField":1}}\n'
    run(['cargo', 'build', '--manifest-path', str(harness / 'Cargo.toml'), '--locked',
         '--offline', '--bin', 'stdin_probe'], BASE, out, 'opaque-build', harness_target)
    process = subprocess.run([str(harness_target / 'debug/stdin_probe')], input=prefix + opaque,
                             text=True, capture_output=True, cwd=BASE)
    (out / 'opaque-probe.stdout.txt').write_text(process.stdout)
    if process.returncode != 0 or not process.stdout.startswith('OK completed=false lines=3'):
        raise RuntimeError('opaque event reproduction failed')

    run(['python3', str(BASE / 'gauntlet.py'), '--verify-only'], BASE, out, 'source-after')
    for name, expected in state['files_sha256'].items():
        if sha(ORIGINAL / name) != expected:
            raise RuntimeError(f'original changed during review: {name}')
    save_json(out / 'review-results.json', {
        'execution_complete': True, 'source_tree_sha256': state['tree_sha256'],
        'baseline_tests_passed': 54, 'mutants_executed': 3, 'mutants_killed': 0,
        'mutants_survived': 3, 'mutants': mutants,
        'source_comment_mismatch_reproduced': True,
        'original_and_frozen_source_unchanged': True,
        'old_coder_assurance_verdict': 'FAILED_TEST_GAPS_AND_UNAVAILABLE_LAYERS',
        'not_a_current_behavioral_defect_demonstration': True,
        'expected_process_exit': 2,
    })
    print('REVIEW_REPRODUCED: 54 baseline tests pass; 3/3 non-equivalent mutants survive; NO ASSURANCE PASS')
    return 2


if __name__ == '__main__':
    sys.exit(main())
