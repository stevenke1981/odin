"""One-shot task receiver: loopback, fixed manifest, exclusive files, bounded streaming."""
import argparse
import datetime
import hashlib
import http.server
import io
import json
import os
import pathlib
import re
import socket
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', type=pathlib.Path, required=True)
    parser.add_argument('--manifest-sha256', required=True)
    parser.add_argument('--output', type=pathlib.Path, required=True)
    parser.add_argument('--port', type=int, default=47654, choices=[47654, 47655])
    parser.add_argument('--timeout', type=int, default=600)
    args = parser.parse_args()
    manifest_bytes = args.manifest.read_bytes()
    if hashlib.sha256(manifest_bytes).hexdigest() != args.manifest_sha256:
        raise ValueError('The preannounced manifest SHA256 must match')
    manifest = json.loads(manifest_bytes)
    rows = manifest['files']
    if not 1 <= len(rows) <= 100 or not 1 <= args.timeout <= 600:
        raise ValueError('Bounded file count and finite timeout are required')
    expected = {}
    for row in rows:
        name, size, digest = row['path'], row['bytes'], row['sha256']
        if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.-]{0,159}', name) or name in expected:
            raise ValueError('Only unique flat ASCII artifact names are accepted')
        if not isinstance(size, int) or not 0 <= size <= 128*1024*1024:
            raise ValueError('Every artifact is limited to 128 MiB')
        if not re.fullmatch(r'[a-f0-9]{64}', digest):
            raise ValueError('Invalid SHA256')
        expected[name] = row
    if sum(row['bytes'] for row in rows) > 384*1024*1024:
        raise ValueError('Manifest total exceeds 384 MiB')
    output = args.output.absolute()
    intended_parent = pathlib.Path(r'E:\odin\dist').resolve(strict=True)
    if output.parent.resolve(strict=True) != intended_parent or not output.name.startswith('package-v3-'):
        raise ValueError('Output must be a fresh task directory directly under E:/odin/dist')
    output.mkdir(exist_ok=False)
    output = output.resolve(strict=True)
    receipt = {'scope':'TASK_ONLY_LOOPBACK_GUEST_ARTIFACT_TRANSFER', 'bind':f'127.0.0.1:{args.port}',
               'manifest_sha256':args.manifest_sha256, 'expected':rows, 'received':[], 'requests':[],
               'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(), 'status':'RUNNING'}
    received = set()
    failed = False
    request_count = 0

    class DeadlineReader(io.RawIOBase):
        def __init__(self, underlying, connection):
            self.underlying = underlying
            self.connection = connection

        def readable(self):
            return True

        def readinto(self, buffer):
            available_time = deadline-time.monotonic()
            if available_time <= 0:
                raise TimeoutError('Absolute request deadline reached')
            self.connection.settimeout(min(10, available_time))
            return self.underlying.readinto(buffer)

        def close(self):
            try:
                self.underlying.close()
            finally:
                super().close()

    class Handler(http.server.BaseHTTPRequestHandler):
        protocol_version = 'HTTP/1.0'
        rbufsize = 0

        def setup(self):
            super().setup()
            self.rfile = io.BufferedReader(DeadlineReader(self.rfile, self.connection))

        def log_message(self, *_):
            pass

        def reject(self, status, reason):
            receipt['requests'].append({'path':self.path, 'status':status, 'reason':reason})
            self.close_connection = True
            self.send_error(status, reason)

        def do_GET(self):
            self.reject(405, 'POST only')

        do_HEAD = do_GET
        do_PUT = do_GET
        do_DELETE = do_GET

        def do_POST(self):
            nonlocal failed, request_count
            request_count += 1
            self.connection.settimeout(10)
            if request_count > 200:
                failed = True
                return self.reject(429, 'Request limit reached')
            name = self.path[1:] if self.path.startswith('/') else ''
            if self.path != '/'+name or name not in expected or '/' in name or '?' in name:
                return self.reject(404, 'Unknown artifact path')
            if name in received:
                return self.reject(409, 'Artifact already received')
            lengths = self.headers.get_all('Content-Length', [])
            if len(lengths) != 1 or not lengths[0].isdigit() or self.headers.get('Transfer-Encoding'):
                return self.reject(400, 'Exactly one fixed Content-Length is required')
            row = expected[name]
            if int(lengths[0]) != row['bytes'] or self.headers.get('Content-Type') != 'application/octet-stream':
                return self.reject(413, 'Expected artifact length and type are required')
            destination = output/name
            partial = output/('.'+name+'.uploading')
            if destination.exists() or destination.is_symlink() or partial.exists() or partial.is_symlink():
                failed = True
                return self.reject(409, 'Exclusive destination already exists')
            digest = hashlib.sha256()
            remaining = row['bytes']
            try:
                descriptor = os.open(partial, os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_BINARY, 0o600)
                with os.fdopen(descriptor, 'wb') as handle:
                    while remaining:
                        available_time = deadline-time.monotonic()
                        if available_time <= 0:
                            raise TimeoutError('Absolute transfer deadline reached')
                        self.connection.settimeout(min(10, available_time))
                        chunk = self.rfile.read1(min(64*1024, remaining))
                        if not chunk:
                            raise EOFError('Truncated upload')
                        handle.write(chunk)
                        digest.update(chunk)
                        remaining -= len(chunk)
                if digest.hexdigest() != row['sha256']:
                    raise ValueError('Uploaded SHA256 mismatch')
                if destination.exists() or destination.is_symlink():
                    raise FileExistsError('Destination changed during upload')
                partial.rename(destination)
                received.add(name)
                receipt['received'].append(dict(row, status='PASS'))
                receipt['requests'].append({'path':self.path,'status':201})
                self.send_response(201)
                self.send_header('Content-Length', '0')
                self.end_headers()
            except (OSError, ValueError, EOFError, socket.timeout) as error:
                failed = True
                if partial.exists() and not partial.is_symlink():
                    quarantine = output/('.'+name+'.quarantined')
                    if not quarantine.exists():
                        partial.rename(quarantine)
                self.reject(422, type(error).__name__+': '+str(error))

    class Server(http.server.HTTPServer):
        active_connection = None

        def get_request(self):
            connection, address = super().get_request()
            connection.settimeout(10)
            self.active_connection = connection
            return connection, address

        def handle_error(self, *_):
            nonlocal failed
            failed = True
            receipt['requests'].append({'status':'CONNECTION_ERROR'})

    deadline = time.monotonic()+args.timeout
    with Server(('127.0.0.1',args.port), Handler) as server:
        server.timeout = 1
        print(json.dumps({'status':'LISTENING','pid':os.getpid(),'bind':receipt['bind'],'files':len(expected)}),flush=True)
        while len(received) < len(expected) and not failed and time.monotonic() < deadline:
            server.handle_request()
    receipt['status'] = 'PASS' if len(received) == len(expected) and not failed else 'FAIL'
    receipt['missing'] = sorted(set(expected)-received)
    receipt['finished_utc'] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    (output/'transfer-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(json.dumps({'status':receipt['status'],'received':len(received),'missing':receipt['missing']}),flush=True)
    return 0 if receipt['status'] == 'PASS' else 1


if __name__ == '__main__':
    raise SystemExit(main())
