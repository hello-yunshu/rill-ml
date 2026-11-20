#!/usr/bin/env python3
"""Compare Runtime API against exact historical source/lock, without re-resolving yanked dependencies."""
from __future__ import annotations
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib
from release_compatibility import load

ROOT = Path(__file__).resolve().parents[1]


def validate_doc(path, version):
    data = json.loads(path.read_text())
    if data.get('crate_version') != version or data.get('includes_private') is not True:
        raise ValueError('historical rustdoc must match version and include private items')
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(version, output):
    authoritative, historical = load(ROOT)
    if version not in [authoritative, *historical]:
        raise ValueError('baseline is outside the declared compatibility policy')
    pins = json.loads((ROOT/'api-baseline/semver-sources.json').read_text())['sources']
    pin = pins[version]
    commit = pin['commit']
    if not re.fullmatch('[a-f0-9]{40}', commit) or not re.fullmatch('[a-f0-9]{64}', pin['lockSha256']):
        raise ValueError('invalid immutable historical source identity')
    if subprocess.run(['git', '-C', str(ROOT), 'cat-file', '-e', commit+'^{commit}'], capture_output=True).returncode:
        subprocess.run(['git', '-C', str(ROOT), 'fetch', '--no-tags', 'origin', commit], check=True)
    raw = subprocess.check_output(['git', '-C', str(ROOT), 'archive', commit])
    output = output.resolve();output.mkdir(parents=True, exist_ok=False)
    source = output/'source';source.mkdir()
    with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
        for member in archive.getmembers():
            relative = Path(member.name)
            if relative.is_absolute() or '..' in relative.parts or not (member.isfile() or member.isdir()):
                raise ValueError('unsupported historical source archive member')
            path = source/relative
            if member.isdir():path.mkdir(parents=True, exist_ok=True)
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(archive.extractfile(member).read())
    lock = source/'Cargo.lock'
    if hashlib.sha256(lock.read_bytes()).hexdigest() != pin['lockSha256']:
        raise ValueError('historical source lock differs from immutable fixture')
    manifest = tomllib.loads((source/'crates/rill-runtime/Cargo.toml').read_text())
    # All pinned Runtime baselines expose exactly these two features. Adding a
    # historical feature policy requires review rather than silently reducing it.
    if set(manifest.get('features', {})) != {'default', 'wasm'}:
        raise ValueError('historical Runtime feature policy requires explicit review')
    env = dict(os.environ)
    env['CARGO_TARGET_DIR'] = str(output/'target')
    # Match cargo-semver-checks rustdoc generation. Old source lint warnings
    # cannot determine compatibility; every SemVer lint still runs unchanged.
    env['RUSTFLAGS'] = '--cap-lints=allow'
    subprocess.run(['cargo', 'rustdoc', '--locked', '--manifest-path', str(source/'Cargo.toml'),
                    '-p', 'rill-runtime', '--lib', '--all-features', '--', '-Z', 'unstable-options',
                    '--output-format', 'json', '--document-private-items', '--document-hidden-items',
                    '--cap-lints=allow'], check=True, env=env, cwd=source)
    if hashlib.sha256(lock.read_bytes()).hexdigest() != pin['lockSha256']:
        raise ValueError('historical rustdoc modified the frozen lock')
    doc = output/'target/doc/rill_runtime.json'
    evidence = {'purpose': 'historical API fixture only; not production security qualification',
                'version': version, **pin, 'rustdocSha256': validate_doc(doc, version),
                'features': ['default', 'wasm'], 'privateItems': True, 'hiddenItems': True,
                'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip()}
    (output/'source-identity.json').write_text(json.dumps(evidence, indent=2)+'\n')
    subprocess.run(['cargo', 'semver-checks', '--package', 'rill-runtime', '--baseline-rustdoc', str(doc)],
                   cwd=ROOT, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-version', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args();run(args.baseline_version, args.output)


if __name__ == '__main__':main()
