#!/usr/bin/env python3
"""Assert the reviewed default WASM feature boundary without replacing cargo-audit."""
from __future__ import annotations
import argparse
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
REQUIRED = {'component-model', 'cranelift', 'pulley', 'runtime'}


def assess(tree: str) -> dict:
    records = set()
    for line in tree.splitlines():
        if line.startswith('wasmtime v'):
            identity, separator, features = line.partition('|')
            if not separator:
                raise ValueError('missing resolved Wasmtime features')
            records.add((identity, features.removesuffix(' (*)')))
    if len(records) != 1:
        raise ValueError('expected exactly one resolved Wasmtime engine')
    identity, raw = records.pop()
    features = set(raw.split(','))
    if not REQUIRED <= features:
        raise ValueError('default full WASM runtime capability was removed')
    if 'component-model-async' in features:
        raise ValueError('component-model-async requires a separate security review')
    return {'engine': identity, 'features': sorted(features)}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', action='append', required=True)
    args = parser.parse_args()
    try:
        result = []
        for target in args.target:
            output = subprocess.check_output(['cargo', 'tree', '--locked', '-p', 'rill-runtime',
                '--target', target, '--prefix', 'none', '--format', '{p}|{f}'], cwd=ROOT, text=True)
            result.append({'target': target, **assess(output)})
        print(json.dumps({'schemaVersion': 1, 'profiles': result}, indent=2))
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'Runtime security profile rejected: {error}\n')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
