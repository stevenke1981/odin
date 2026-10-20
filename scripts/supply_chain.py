"""Inventory actual locked packages and check only Git-tracked public candidates."""
from pathlib import Path
import datetime
import hashlib
import json
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[1]


def main():
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--format-version', '1', '--locked'], cwd=ROOT))
    lock = tomllib.loads((ROOT/'Cargo.lock').read_text(encoding='utf-8'))
    checksums = {(p['name'], p['version']): p.get('checksum') for p in lock['package']}
    components = []
    notices = ['Odin third-party dependency license texts', 'Generated from the real Cargo.lock and local crate sources.',
               'Includes all resolved target variants; not every component is linked on each OS.', '']
    inventory = []
    supplemental = json.loads((ROOT/'docs/dependency-licenses/index.json').read_text(encoding='utf-8'))
    for package in sorted(metadata['packages'], key=lambda p: (p['name'], p['version'])):
        if package['source'] is None:
            continue
        checksum = checksums.get((package['name'], package['version']))
        component = {'type':'library', 'name':package['name'], 'version':package['version'],
                     'purl':f"pkg:cargo/{package['name']}@{package['version']}",
                     'licenses':[{'expression':package['license']}] if package['license'] else []}
        if checksum:
            component['hashes'] = [{'alg':'SHA-256', 'content':checksum}]
        components.append(component)
        folder = Path(package['manifest_path']).parent
        license_files = set()
        if package.get('license_file'):
            license_files.add(folder/package['license_file'])
        for pattern in ['LICENSE*', 'COPYING*', 'NOTICE*', 'UNLICENSE*', 'LICENCE*']:
            license_files.update(folder.glob(pattern))
        authors = folder/'AUTHORS'
        if authors.is_file() and 'LICENSE' in authors.read_text(encoding='utf-8', errors='replace'):
            license_files.add(authors)
        texts = []
        for path in sorted(license_files):
            if path.is_file() and path.stat().st_size <= 512*1024:
                texts.append((path.name, path.read_text(encoding='utf-8', errors='replace')))
        sources = []
        for entry in supplemental:
            if f"{package['name']}@{package['version']}" not in entry['packages']:
                continue
            path = ROOT/'docs/dependency-licenses'/entry['file']
            if hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
                raise RuntimeError('Supplemental license hash mismatch')
            texts.append((path.name, path.read_text(encoding='utf-8')))
            sources.append(entry['source'])
        inventory.append({'name':package['name'], 'version':package['version'], 'license_expression':package['license'],
                          'checksum_sha256':checksum, 'license_files':[name for name, _ in texts], 'supplemental_sources':sources})
        notices += ['='*72, f"{package['name']} {package['version']} | {package['license']}", '='*72]
        for name, content in texts:
            notices += [name, content, '']
    verification = ROOT/'verification'
    verification.mkdir(exist_ok=True)
    missing = [p['name'] for p in inventory if not p['license_expression'] or not p['license_files'] or not p['checksum_sha256']]
    sbom = {'bomFormat':'CycloneDX', 'specVersion':'1.5', 'version':1,
            'metadata':{'timestamp':datetime.datetime.now(datetime.UTC).isoformat(),
                        'component':{'type':'application', 'name':'odin', 'version':next(p['version'] for p in metadata['packages'] if p['name'] == 'odin-agent')},
                        'properties':[{'name':'odin:scope','value':'all Cargo.lock target variants; package inventory'}]},
            'components':components}
    sbom_path = verification/'sbom.cdx.json'
    if sbom_path.is_file():
        previous = json.loads(sbom_path.read_text(encoding='utf-8'))
        if previous.get('components') == components and previous.get('metadata', {}).get('component') == sbom['metadata']['component']:
            # Retain inventory identity across a later scan of additional public evidence files.
            sbom['metadata']['timestamp'] = previous['metadata']['timestamp']
    sbom_path.write_text(json.dumps(sbom, indent=2)+'\n', encoding='utf-8', newline='\n')
    (verification/'license-inventory.json').write_text(json.dumps({'status':'FAIL' if missing else 'PASS', 'packages':inventory, 'missing':missing}, indent=2)+'\n', encoding='utf-8', newline='\n')
    (ROOT/'docs/THIRD_PARTY_LICENSES.txt').write_text('\n'.join(notices).rstrip('\n')+'\n', encoding='utf-8', newline='\n')

    rules = {
        'github_token':r'\bgh[pousr]_[A-Za-z0-9]{30,255}\b|\bgithub_pat_[A-Za-z0-9_]{30,255}\b',
        'aws_access_key':r'\bAKIA[A-Z0-9]{16}\b',
        'private_key':r'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----',
        'api_secret':r'(?i)\b(?:api[_-]?key|access[_-]?token|client[_-]?secret|password)\s*[:=]\s*[\x22\x27]([A-Za-z0-9/_+=.-]{24,})[\x22\x27]',
    }
    tracked = subprocess.check_output(['git', 'ls-files', '-z'], cwd=ROOT).decode('utf-8').split('\0')
    findings = []
    for name in filter(None, tracked):
        if name.split('/')[0] in {'.odin', '.serena', 'target', 'state', '.venv', 'dist'}:
            findings.append({'path':name, 'rule':'private_or_build_path'})
            continue
        path = ROOT/name
        if not path.is_file():
            continue
        try:
            content = path.read_text(encoding='utf-8')
        except UnicodeDecodeError:
            continue
        for rule, pattern in rules.items():
            if re.search(pattern, content):
                findings.append({'path':name, 'rule':rule})
    report = {'status':'FAIL' if findings else 'PASS', 'scope':'Git-tracked current public candidate only',
              'files_checked':len(list(filter(None, tracked))), 'findings':findings,
              'license_inventory_packages':len(inventory), 'missing_license_metadata_or_text':missing,
              'sbom_sha256':hashlib.sha256((verification/'sbom.cdx.json').read_bytes()).hexdigest(),
              'limit':'Pattern scan and package metadata are not a formal security or legal audit.'}
    (verification/'public-candidate-scan.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8', newline='\n')
    print(json.dumps(report))
    return 1 if missing or findings else 0


if __name__ == '__main__':
    raise SystemExit(main())
