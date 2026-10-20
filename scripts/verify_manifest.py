#!/usr/bin/env python3
"""Verify the shipped source hashes. Rerunning generators/build formatters changes them."""
from pathlib import Path
import hashlib
ROOT=Path(__file__).resolve().parents[1]
failed=[];checked=0
for line in (ROOT/'MANIFEST.sha256').read_text(encoding='utf-8').splitlines():
    expected,name=line.split('  ',1);path=ROOT/name
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest()!=expected:failed.append(name)
    checked+=1
print(f'Checked {checked} manifest entries; mismatches={len(failed)}')
for name in failed:print(name)
raise SystemExit(1 if failed else 0)
