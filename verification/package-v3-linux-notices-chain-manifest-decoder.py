import base64,gzip,hashlib,json,pathlib,re
tools=pathlib.Path(r'E:\odin\.odin\validation-tools\packaging-v3')
raw=(tools/'vm/notices-transfer-manifest-01.txt').read_text(encoding='utf-8').replace('\r','')
payload=raw.split('NOTICES_MANIFEST_BEGIN\n',1)[1].split('NOTICES_MANIFEST_END',1)[0]
noise=re.findall(r'\[\s*\d+\.\d+\] hrtimer: interrupt took \d+ ns\n',payload)
payload=re.sub(r'\[\s*\d+\.\d+\] hrtimer: interrupt took \d+ ns\n','',payload)
data=gzip.decompress(base64.b64decode(''.join(payload.splitlines()),validate=True))
announcement=json.loads(raw.split('NOTICES_MANIFEST_END\n',1)[1].splitlines()[0])
assert hashlib.sha256(data).hexdigest()==announcement['sha256']=='287ed457bc75ded7543eb93240929b34c78edd329a7ef8a2384233053d38ac47'
assert len(data)==announcement['bytes']
manifest=json.loads(data)
assert manifest['source_revision']=='0fe03ba30da9600c25588d506d7e094df89cf56f'
assert manifest['packaging_revision']=='f5878bfa725db8c0dcab7e45ca3cf89159508975'
assert len(manifest['files'])==announcement['files']<=100
assert sum(row['bytes'] for row in manifest['files'])==announcement['total_artifact_bytes']<=384*1024*1024
output=tools/'vm/notices-transfer-manifest-02.json'
assert not output.exists()
output.write_bytes(data)
print(json.dumps(dict(announcement,path=str(output),transport_kernel_lines=noise)))
