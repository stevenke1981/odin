#!/usr/bin/env python3
"""Build a Debian package and portable archive after executing the native Linux ELF."""
import argparse
import hashlib
import json
import os
import platform
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path

from package_tar import ROOT, prepare_contents, write_archive


def validate_static_elf(binary):
    data = binary.read_bytes()
    if data[:6] != b'\x7fELF\x02\x01' or len(data) < 64:
        raise ValueError('Expected a little-endian 64-bit ELF')
    if struct.unpack_from('<H', data, 18)[0] != 62:
        raise ValueError('Expected the x86-64 ELF machine type')
    phoff = struct.unpack_from('<Q', data, 32)[0]
    entry_size, count = struct.unpack_from('<HH', data, 54)
    if entry_size < 56 or count == 0 or phoff + entry_size * count > len(data):
        raise ValueError('Invalid ELF program-header table')
    if any(struct.unpack_from('<I', data, phoff + index * entry_size)[0] == 3 for index in range(count)):
        raise ValueError('This dependency-free Debian recipe requires static musl, without PT_INTERP')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--expected-binary-sha256', required=True)
    parser.add_argument('--output', type=Path, default=ROOT/'dist')
    args = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine().lower() not in ('x86_64', 'amd64'):
        raise ValueError('Run this recipe on an actual Linux x86-64 target (a local VM is supported)')
    if shutil.which('dpkg-deb') is None:
        raise ValueError('dpkg-deb is required; no automatic package-manager modification')
    binary = args.binary.resolve(strict=True)
    validate_static_elf(binary)
    metadata, contents, binary_sha = prepare_contents(binary, args.expected_binary_sha256)
    if metadata['target'] != 'x86_64-unknown-linux-musl':
        raise ValueError('This recipe accepts only the validated static x86_64-unknown-linux-musl target')
    args.output.mkdir(parents=True, exist_ok=True)
    debian_version = metadata['version'].replace('-', '~')
    deb = args.output/f'odin-agent_{debian_version}_amd64.deb'
    if deb.exists():
        raise FileExistsError(f'Refusing to replace {deb}; choose a fresh output directory')
    archive = write_archive(args.output, metadata, contents)
    command = None
    command_log = None
    with tempfile.TemporaryDirectory(prefix='.deb-stage-', dir=args.output) as temporary:
        stage = Path(temporary)
        stage.chmod(0o755)
        for name, data in contents.items():
            relative = 'usr/bin/odin' if name == 'odin' else 'usr/share/odin-agent/'+name
            path = stage/relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            path.chmod(0o755 if name == 'odin' else 0o644)
        copyright_file = stage/'usr/share/doc/odin-agent/copyright'
        copyright_file.parent.mkdir(parents=True, exist_ok=True)
        copyright_file.write_bytes(contents['LICENSE'])
        control = stage/'DEBIAN/control'
        control.parent.mkdir()
        size = sum(path.stat().st_size for path in stage.rglob('*') if path.is_file())
        control.write_text(
            'Package: odin-agent\n'
            f'Version: {debian_version}\n'
            'Section: devel\nPriority: optional\nArchitecture: amd64\n'
            'Maintainer: Odin contributors <stevenke1981@gmail.com>\n'
            f'Installed-Size: {(size+1023)//1024}\n'
            'Homepage: https://github.com/stevenke1981/odin\n'
            'Description: Independent code tools with direct LSP and transactional edits\n'
            ' Static musl CLI and local dashboard. Language servers are configured separately.\n'
            ' No service, shell permission, client configuration or OS sandbox is enabled\n'
            ' automatically. Linux native sandboxing is not implemented in this preview.\n',
            encoding='utf-8', newline='\n')
        control.chmod(0o644)
        command = ['dpkg-deb', '--root-owner-group', '--build', str(stage), str(deb.resolve())]
        environment = dict(os.environ, SOURCE_DATE_EPOCH='0', DPKG_DEB_COMPRESSOR_TYPE='xz')
        completed = subprocess.run(command, capture_output=True, text=True, encoding='utf-8', env=environment)
        command_log = {'command':command, 'exit_code':completed.returncode, 'stdout':completed.stdout, 'stderr':completed.stderr}
        (args.output/'dpkg-build-command.json').write_text(json.dumps(command_log, indent=2)+'\n', encoding='utf-8', newline='\n')
        completed.check_returncode()
    artifacts = [{'path':path.name, 'sha256':hashlib.sha256(path.read_bytes()).hexdigest(), 'bytes':path.stat().st_size} for path in (archive, deb)]
    sums = ''.join(f'{item["sha256"]}  {item["path"]}\n' for item in artifacts)
    (args.output/'SHA256SUMS').write_text(sums, encoding='ascii', newline='\n')
    report = {'scope':'LINUX_NATIVE_PACKAGE', 'status':'PASS', 'platform':platform.platform(),
              'binary_sha256':binary_sha, 'build':metadata, 'elf':'STATIC_X86_64_LINUX',
              'debian_package':'odin-agent', 'artifacts':artifacts, 'signed':False,
              'installer_lifecycle':'NOT_RUN', 'native_guest_cargo_tests':'NOT_RUN',
              'sandbox':'NOT_IMPLEMENTED_ON_LINUX', 'command':command_log}
    (args.output/'package-report.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8', newline='\n')
    (args.output/'build-manifest.json').write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8', newline='\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main()
