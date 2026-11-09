#!/usr/bin/env python3
"""Four persisted, assertion-checked manual mutations; no candidate modification."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import sys

BASE = Path(__file__).resolve().parent
WORK = BASE / 'final-workspace/btae-eval'
CASES = [
    ('inclusive-line-limit', 'if self.line.len() > self.limits.max_line_bytes &&',
     'if self.line.len() >= self.limits.max_line_bytes &&',
     'exact_line_byte_limit_accepts_content_before_a_later_lf'),
    ('reasoning-equality', 'if reasoning > output {', 'if reasoning >= output {',
     'reasoning_equal_to_output_is_a_valid_subset_boundary'),
    ('inclusive-total-limit', 'if total > self.limits.max_total_bytes as u64 {',
     'if total >= self.limits.max_total_bytes as u64 {',
     'exact_total_byte_limit_accepts_whole_and_chunked_real_transcripts'),
    ('open-step-turn-end', '                if self.open_step.is_some() {\n'
     '                    return Err(protocol(event, "turn_end while a step is open"));\n'
     '                }\n', '', 'turn_end_with_an_open_step_is_a_terminal_protocol_error'),
]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    out = (BASE / sys.argv[1]).resolve()
    if not out.is_relative_to(BASE / 'output'):
        raise RuntimeError('output must be in audit/output')
    out.mkdir(parents=True, exist_ok=False)
    original = digest(WORK / 'src/dsh.rs')
    tests_hash = digest(WORK / 'tests/dsh_adapter.rs')
    results = []
    for label, before, after, test in CASES:
        run_id = hashlib.sha256(str(out.relative_to(BASE)).encode()).hexdigest()[:12]
        candidate = BASE / 'mutants' / run_id / label
        shutil.copytree(WORK, candidate, ignore=shutil.ignore_patterns('target'))
        source = candidate / 'src/dsh.rs'
        text = source.read_text()
        if text.count(before) != 1:
            raise RuntimeError(f'{label}: mutation target not unique')
        source.write_text(text.replace(before, after, 1))
        mutated = digest(source)
        if mutated == original or digest(candidate / 'tests/dsh_adapter.rs') != tests_hash:
            raise RuntimeError('mutation missing or tests changed')
        env = dict(os.environ)
        env['CARGO_TARGET_DIR'] = str(BASE / 'mutation-target')
        cmd = ['cargo', 'test', '--test', 'dsh_adapter', '--locked', '--offline', '-j', '2',
               '--', '--test-threads=4']
        run = subprocess.run(cmd, cwd=candidate, env=env, capture_output=True, text=True)
        (out / f'{label}.stdout.txt').write_text(run.stdout)
        (out / f'{label}.stderr.txt').write_text(run.stderr)
        if run.returncode != 101 or f'test {test} ... FAILED' not in run.stdout:
            raise RuntimeError(f'{label}: did not demonstrate a behavioral test failure')
        if 'test result: FAILED.' not in run.stdout or 'could not compile' in run.stderr:
            raise RuntimeError('compile/infrastructure failure is not a killed mutant')
        if digest(source) != mutated or digest(WORK / 'src/dsh.rs') != original:
            raise RuntimeError('source changed during mutation run')
        results.append({'id': label, 'outcome': 'KILLED', 'test': test,
                        'command': cmd, 'exit_code': run.returncode,
                        'source_sha256': original, 'mutant_sha256': mutated,
                        'tests_sha256': tests_hash, 'manifest_path': str(candidate / 'Cargo.toml')})
        (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
        print(label, 'KILLED by', test, flush=True)
    if len(results) != len(CASES):
        raise RuntimeError('not all mutants ran')
    print('4/4 manual mutants killed; no coverage/fuzzing completeness claim')


if __name__ == '__main__':
    main()
