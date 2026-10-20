#!/usr/bin/env python3
"""Package the binary that actually ran metadata, with a per-file SHA-256 inventory."""
import argparse, hashlib, io, json, platform, re, subprocess, tarfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT/'dist')
    args = parser.parse_args()
    run = subprocess.run([str(args.binary.resolve()), 'metadata'], capture_output=True, check=True, text=True, encoding='utf-8')
    metadata = json.loads(run.stdout)
    version, target = metadata['version'], metadata['target']
    if not re.fullmatch(r'[0-9A-Za-z.+-]+', version) or not re.fullmatch(r'[0-9A-Za-z_-]+', target):
        raise ValueError('Invalid version or target')
    if platform.system() not in ('Linux', 'Darwin') or not any(tag in target for tag in ('linux', 'apple-darwin')):
        raise ValueError('Tar packaging must run on a native Linux/macOS binary; use package.ps1 on Windows')
    package = f'odin-{version}-{target}'
    sources = {'odin': args.binary.resolve(), 'odin.example.toml': ROOT/'examples/odin.native.toml'}
    for name in ['README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md']:
        sources[name] = ROOT/name
    for directory in ['docs', 'schemas']:
        sources.update({path.relative_to(ROOT).as_posix(): path for path in (ROOT/directory).rglob('*') if path.is_file()})
    contents = {name: path.read_bytes() for name, path in sources.items()}
    contents['build-metadata.json'] = (json.dumps(metadata, indent=2)+'\n').encode()
    contents['SHA256SUMS.json'] = (json.dumps([{'path': name, 'sha256': hashlib.sha256(data).hexdigest()} for name, data in sorted(contents.items())], indent=2)+'\n').encode()
    args.output.mkdir(parents=True, exist_ok=True)
    archive = args.output/(package+'.tar.gz')
    # Exclusive creation never overwrites an existing artifact.
    with archive.open('xb') as handle, tarfile.open(fileobj=handle, mode='w:gz') as bundle:
        for name, data in sorted(contents.items()):
            info = tarfile.TarInfo(package+'/'+name)
            info.size, info.mode, info.mtime = len(data), 0o755 if name == 'odin' else 0o644, 0
            bundle.addfile(info, io.BytesIO(data))
    report = {'scope':'NATIVE_TAR_PACKAGE', 'platform':platform.platform(), 'status':'PASS', 'artifact':str(archive), 'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(), 'build':metadata, 'installer_lifecycle':'NOT_APPLICABLE', 'timestamp_unix':time.time()}
    (args.output/'package-report.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
