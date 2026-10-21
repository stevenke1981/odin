#!/usr/bin/env python3
"""Package the binary that actually ran metadata, with a per-file SHA-256 inventory."""
import argparse, gzip, hashlib, io, json, platform, re, subprocess, tarfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def prepare_contents(binary, expected_sha256=None):
    """Require execution on the matching target OS before recording package contents."""
    binary = binary.resolve(strict=True)
    digest = hashlib.sha256(binary.read_bytes()).hexdigest()
    if expected_sha256 is not None and digest != expected_sha256.lower():
        raise ValueError('Binary does not match the selected release candidate SHA-256')
    run = subprocess.run([str(binary), 'metadata'], capture_output=True, check=True, text=True, encoding='utf-8')
    metadata = json.loads(run.stdout)
    version, target = metadata['version'], metadata['target']
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?', version) or not re.fullmatch(r'[0-9A-Za-z_-]+', target):
        raise ValueError('Invalid version or target')
    expected_os = {'Linux': 'linux', 'Darwin': 'apple-darwin'}.get(platform.system())
    if expected_os is None or expected_os not in target:
        raise ValueError('Tar packaging must run on a native Linux/macOS binary; use package.ps1 on Windows')
    if metadata.get('tracked_source_dirty') is not False or metadata.get('profile') != 'release':
        raise ValueError('Packaging requires a clean-source release build')
    sources = {'odin': binary, 'odin.example.toml': ROOT/'packaging/odin.host.example.toml'}
    for name in ['README.md', 'LICENSE', 'THIRD_PARTY_NOTICES.md']:
        sources[name] = ROOT/name
    for directory in ['docs', 'schemas']:
        sources.update({path.relative_to(ROOT).as_posix(): path for path in (ROOT/directory).rglob('*') if path.is_file()})
    sbom = ROOT/'verification/sbom.cdx.json'
    if sbom.is_file():
        sources['verification/sbom.cdx.json'] = sbom
    contents = {name: path.read_bytes() for name, path in sources.items()}
    contents['build-metadata.json'] = (json.dumps(metadata, indent=2)+'\n').encode()
    contents['SHA256SUMS.json'] = (json.dumps([{'path': name, 'sha256': hashlib.sha256(data).hexdigest()} for name, data in sorted(contents.items())], indent=2)+'\n').encode()
    return metadata, contents, digest


def write_archive(output, metadata, contents):
    package = f'odin-{metadata["version"]}-{metadata["target"]}'
    output.mkdir(parents=True, exist_ok=True)
    archive = output/(package+'.tar.gz')
    # Exclusive creation never overwrites an existing artifact.
    with archive.open('xb') as handle, gzip.GzipFile(filename='', mode='wb', fileobj=handle, mtime=0) as compressed, tarfile.open(fileobj=compressed, mode='w') as bundle:
        for name, data in sorted(contents.items()):
            info = tarfile.TarInfo(package+'/'+name)
            info.size, info.mode, info.mtime = len(data), 0o755 if name == 'odin' else 0o644, 0
            bundle.addfile(info, io.BytesIO(data))
    return archive


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT/'dist')
    parser.add_argument('--expected-binary-sha256')
    args = parser.parse_args()
    metadata, contents, digest = prepare_contents(args.binary, args.expected_binary_sha256)
    archive = write_archive(args.output, metadata, contents)
    report = {'scope':'NATIVE_TAR_PACKAGE', 'platform':platform.platform(), 'status':'PASS', 'artifact':str(archive), 'sha256':hashlib.sha256(archive.read_bytes()).hexdigest(), 'binary_sha256':digest, 'build':metadata, 'installer_lifecycle':'NOT_APPLICABLE', 'signed':False, 'timestamp_unix':time.time()}
    (args.output/'package-report.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
