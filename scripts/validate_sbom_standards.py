#!/usr/bin/env python3
"""Run the external standard validators used by the release SBOM gate."""

from __future__ import annotations

import argparse
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--spdx", type=Path, required=True)
    args = parser.parse_args()

    from spdx_tools.spdx.parser.parse_anything import parse_file
    from spdx_tools.spdx.validation.document_validator import validate_full_spdx_document

    document = parse_file(file_name=str(args.spdx))
    errors = validate_full_spdx_document(document)
    if errors:
        for error in errors:
            print(f"FAIL SPDX standard: {error}")
        return 1
    print("PASS SPDX standard validation")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
