#!/usr/bin/env python3
"""Read pinned upstream archives as data and collect offline native-LSP notices.

No extraction, build, repository hook, package restore, or network request occurs.
The sets are Cargo.lock supersets or the actual Marksman embedded NuGet inventory;
they are evidence inventories, not a certification of which code is linked or law.
"""
from __future__ import annotations
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import stat
import tarfile
import tomllib
import xml.etree.ElementTree as ET
import zipfile

PREFIXES = ('LICENSE', 'LICENCE', 'NOTICE', 'COPYING', 'COPYRIGHT', 'PATENTS', 'THIRD-PARTY', 'THIRDPARTY')
MAX_NOTICE_BYTES = 16 * 1024**2


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def json_bytes(value: object) -> bytes:
    return (json.dumps(value, indent=2, sort_keys=True) + '\n').encode('utf-8')


def reject_links(path: Path) -> None:
    current = Path(os.path.abspath(path))
    for item in (current, *current.parents):
        try:
            info = item.lstat()
        except FileNotFoundError:
            continue
        if stat.S_ISLNK(info.st_mode) or getattr(info, 'st_file_attributes', 0) & 0x400:
            raise ValueError('Reparse/symlink path rejected: ' + str(item))


def archive_notices(data: bytes, kind: str) -> tuple[dict, list[tuple[str, bytes]]]:
    files = {}
    metadata = {}
    if kind == 'crate':
        with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
            members = archive.getmembers()
            manifest = next(m for m in members if m.isfile() and m.name.count('/') == 1 and m.name.endswith('/Cargo.toml'))
            if manifest.size > MAX_NOTICE_BYTES:
                raise ValueError('Oversized crate manifest')
            package = tomllib.loads(archive.extractfile(manifest).read().decode('utf-8'))['package']
            metadata = {'license': package.get('license'), 'license_file': package.get('license-file'),
                        'repository': package.get('repository'), 'authors': package.get('authors', []),
                        'description': package.get('description')}
            for member in members:
                name = member.name.split('/', 1)[-1]
                if not member.isfile() or not (Path(name).name.upper().startswith(PREFIXES) or name == metadata['license_file']):
                    continue
                if member.size > MAX_NOTICE_BYTES:
                    raise ValueError('Oversized notice in crate')
                files[name] = archive.extractfile(member).read()
    elif kind == 'nuget':
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            nuspecs = [i for i in archive.infolist() if i.filename.endswith('.nuspec')]
            if len(nuspecs) != 1 or nuspecs[0].file_size > MAX_NOTICE_BYTES:
                raise ValueError('Expected one bounded NuGet nuspec')
            raw = archive.read(nuspecs[0])
            tree = ET.fromstring(raw)
            for element in tree.iter():
                name = element.tag.rsplit('}', 1)[-1]
                if name == 'license':
                    metadata['license'] = element.text
                    metadata['license_type'] = element.get('type')
                elif name in ('authors', 'copyright', 'licenseUrl'):
                    metadata[name] = element.text
                elif name == 'repository':
                    metadata['repository'] = dict(element.attrib)
            files[nuspecs[0].filename] = raw
            for member in archive.infolist():
                name = member.filename
                if member.is_dir() or not (Path(name).name.upper().startswith(PREFIXES) or
                    (metadata.get('license_type') == 'file' and name == metadata.get('license'))):
                    continue
                if member.file_size > MAX_NOTICE_BYTES:
                    raise ValueError('Oversized NuGet notice')
                files[name] = archive.read(member)
    else:
        raise ValueError('Unsupported source archive kind')
    return metadata, sorted(files.items())


def collect(metadata: dict, cache: Path, output: Path) -> dict:
    reject_links(cache)
    reject_links(output)
    if output.exists():
        raise ValueError('Notice output must be fresh and absent')
    for parent in (output, *output.parents):
        if (parent / '.git').exists():
            raise ValueError('Notice output must be outside every Git checkout')
    output.mkdir(parents=True)
    inventory = []
    decoded = {}
    for group in metadata['notice_sets']:
        parts = [b'Native LSP dependency notice inventory. Scope: ' + group['scope'].encode() + b'\n']
        packages = []
        for source in group['sources']:
            path = cache / source['filename']
            reject_links(path)
            data = path.read_bytes()
            if digest(data) != source['sha256']:
                raise ValueError('Pinned archive SHA256 mismatch: ' + source['id'])
            key = (source['sha256'], source['kind'])
            if key not in decoded:
                decoded[key] = archive_notices(data, source['kind'])
            info, original_files = decoded[key]
            files = list(original_files)
            for supplement in source.get('supplemental_notice_sources', []):
                notice_path = cache / supplement['filename']
                reject_links(notice_path)
                notice_data = notice_path.read_bytes()
                if digest(notice_data) != supplement['sha256']:
                    raise ValueError('Supplemental notice SHA256 mismatch: ' + supplement['id'])
                files.append(('UPSTREAM/' + supplement['filename'], notice_data))
            parts.append(b'\n=== ' + source['package'].encode() + b' ' + source['version'].encode() + b' ===\n')
            parts.append(json_bytes({'source': source['url'], 'sha256': source['sha256'], **info}))
            for name, content in files:
                parts.append(b'\n--- ' + name.encode('utf-8') + b' ---\n' + content + b'\n')
            packages.append({'package': source['package'], 'version': source['version'], 'source': source['url'],
                             'sha256': source['sha256'], **info,
                             'notice_files': [{'path': name, 'sha256': digest(content)} for name, content in files],
                             'applicability': group['applicability'],
                             'supplemental_source_status': source.get('supplemental_source_status'),
                             'supplemental_source_revision': source.get('supplemental_source_revision'),
                             'status': 'NOTICES_COLLECTED' if any(not name.endswith('.nuspec') for name, _ in files) else 'NO_LICENSE_TEXT_IN_PUBLISHED_ARCHIVE_OR_FIXED_SOURCE'})
        directory = output / group['id']
        directory.mkdir()
        content = b''.join(parts)
        (directory / 'THIRD-PARTY-NOTICES.txt').write_bytes(content)
        (directory / 'inventory.json').write_bytes(json_bytes({'scope': group['scope'], 'packages': packages}))
        inventory.append({'id': group['id'], 'scope': group['scope'], 'package_count': len(packages),
                          'missing_notice_text': sum(p['status'] != 'NOTICES_COLLECTED' for p in packages),
                          'unresolved': [{key: p.get(key) for key in ('package', 'version', 'license', 'authors', 'repository', 'applicability', 'supplemental_source_revision', 'status')} for p in packages if p['status'] != 'NOTICES_COLLECTED'],
                          'aggregate_sha256': digest(content)})
    result = {'schema_version': 1, 'status': 'WITH_DECLARED_NOTICE_GAPS' if any(g['missing_notice_text'] for g in inventory) else 'NOTICES_COLLECTED_NOT_LEGAL_CERTIFICATION', 'scope': 'DATA_ONLY_PINNED_NATIVE_NOTICE_COLLECTION_NOT_LEGAL_CERTIFICATION', 'groups': inventory}
    (output / 'inventory.json').write_bytes(json_bytes(result))
    return result


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--metadata', type=Path, required=True)
    parser.add_argument('--expected-metadata-sha256', required=True)
    parser.add_argument('--cache-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    reject_links(args.metadata)
    data = args.metadata.read_bytes()
    if digest(data) != args.expected_metadata_sha256:
        parser.error('Metadata differs from the reviewed SHA256')
    print(json.dumps(collect(json.loads(data), args.cache_dir, args.output)))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
