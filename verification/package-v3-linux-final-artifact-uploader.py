import argparse,hashlib,http.client,json,pathlib,time
p=argparse.ArgumentParser();p.add_argument('--manifest',type=pathlib.Path,required=True);p.add_argument('--directory',type=pathlib.Path,required=True);a=p.parse_args()
manifest=json.loads(a.manifest.read_text())
records=[]
for row in manifest['files']:
    path=a.directory/row['path']
    assert path.is_file() and not path.is_symlink() and path.stat().st_size==row['bytes']
    assert hashlib.sha256(path.read_bytes()).hexdigest()==row['sha256']
    started=time.monotonic()
    connection=http.client.HTTPConnection('10.0.2.2',47654,timeout=30)
    connection.putrequest('POST','/'+row['path'])
    connection.putheader('Content-Type','application/octet-stream')
    connection.putheader('Content-Length',str(row['bytes']))
    connection.endheaders()
    with path.open('rb') as source:
        while data:=source.read(64*1024): connection.send(data)
    response=connection.getresponse(); response.read()
    records.append({'path':row['path'],'status_code':response.status,'elapsed_seconds':round(time.monotonic()-started,3)})
    connection.close()
    assert response.status==201,records[-1]
print(json.dumps({'scope':'GUEST_OUTBOUND_FIXED_ARTIFACT_UPLOAD','status':'PASS','source_revision':manifest['source_revision'],'files':records}))
