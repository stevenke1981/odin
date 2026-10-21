param(
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$ExpectedBinarySha256,
    [Parameter(Mandatory)][string]$Output,
    [string[]]$Languages = @('rust', 'python', 'typescript', 'go', 'java', 'csharp'),
    [string]$Executor = 'C:/Users/steven/AppData/Local/Programs/DevTools/mxc/node_modules/@microsoft/mxc-sdk/bin/x64/wxc-exec.exe',
    [string]$ExecutorSha256 = '8a6d1db4dd1846981127d9014741be18b094d07be967765dff6754bda1d40259',
    [switch]$AllowUI,
    [switch]$TypeScriptCompatibility,
    [int]$Timeout = 60
)
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$binarySha = (Get-FileHash -LiteralPath $Binary -Algorithm SHA256).Hash.ToLowerInvariant()
if ($binarySha -ne $ExpectedBinarySha256.ToLowerInvariant()) { throw 'Candidate binary hash mismatch' }
if ((Get-FileHash -LiteralPath $Executor -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ExecutorSha256.ToLowerInvariant()) { throw 'Native executor hash mismatch' }
$reportPath = [IO.Path]::GetFullPath($Output)
$logPath = [IO.Path]::ChangeExtension($reportPath, '.txt')
$receiptPath = [IO.Path]::ChangeExtension($reportPath, '.command.json')
foreach ($path in @($reportPath, $logPath, $receiptPath, [IO.Path]::ChangeExtension($reportPath, '.requests.jsonl'), [IO.Path]::ChangeExtension($reportPath, '.normalization.json'))) {
    if (Test-Path -LiteralPath $path) { throw "Evidence already exists: $path" }
}
$python = (Get-Command python -CommandType Application | Select-Object -First 1).Source
$harnessSha = (Get-FileHash -LiteralPath 'tests/real_ide_backends.py' -Algorithm SHA256).Hash.ToLowerInvariant()
$driverArgs = @('-u', 'tests/real_ide_backends.py', '--binary', $Binary, '--languages') + $Languages + @('--mode', 'sandbox', '--sandbox-executor', $Executor, '--sandbox-sha256', $ExecutorSha256, '--timeout', $Timeout.ToString(), '--output', $Output)
if ($AllowUI) { $driverArgs += '--sandbox-allow-ui' }
if ($TypeScriptCompatibility) { $driverArgs += @('--sandbox-typescript-init-temp', '--sandbox-typescript-child-path-flags') }
$started = (Get-Date).ToUniversalTime().ToString('o')
& $python @driverArgs 2>&1 | Tee-Object -FilePath $logPath
$driverExit = $LASTEXITCODE
$finished = (Get-Date).ToUniversalTime().ToString('o')
$receipt = [ordered]@{
    command = @($python) + $driverArgs
    cwd = (Get-Location).Path
    started_utc = $started
    finished_utc = $finished
    exit_code = $driverExit
    binary_sha256 = $binarySha
    harness_sha256 = $harnessSha
    runner_source = $PSCommandPath
    runner_sha256 = (Get-FileHash -LiteralPath $PSCommandPath -Algorithm SHA256).Hash.ToLowerInvariant()
    sandbox_executor_sha256 = $ExecutorSha256
    raw_stdout_log = $logPath
}
$receiptJson = ($receipt | ConvertTo-Json -Depth 10).Replace("`r`n", "`n") + "`n"
[IO.File]::WriteAllText($receiptPath, $receiptJson, [Text.UTF8Encoding]::new($false))
$normalizer = @'
import hashlib, json, sys
from pathlib import Path
rp=Path(sys.argv[1]);cp=Path(sys.argv[2]);tp=rp.with_suffix('.requests.jsonl');lp=rp.with_suffix('.txt')
h=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
r=json.loads(rp.read_text(encoding='utf-8-sig'));c=json.loads(cp.read_text(encoding='utf-8-sig'))
checks=lambda v:hashlib.sha256(json.dumps([p['checks'] for p in v['projects']],ensure_ascii=False,sort_keys=True,separators=(',',':')).encode()).hexdigest()
before=checks(r)
for p in (tp,lp):p.write_bytes(p.read_bytes().replace(b'\r\n',b'\n'))
assert c['harness_sha256']==r['harness_sha256'] and c['binary_sha256']==r['binary_sha256']
r.update(transcript_sha256=h(tp),line_endings_normalized_before_hash_binding=True)
rp.write_bytes((json.dumps(r,ensure_ascii=False,indent=2)+'\n').encode())
c.update(log_sha256=h(lp),report_sha256=h(rp),line_endings_normalized_before_hash_binding=True)
cp.write_bytes((json.dumps(c,ensure_ascii=False,indent=2)+'\n').encode())
assert before==checks(json.loads(rp.read_text(encoding='utf-8')))
proof={'status':'PASS','scope':'LF normalization only; all original check results preserved','logical_checks_sha256_before':before,'logical_checks_sha256_after':checks(r),'actual_driver_exit_code':c['exit_code'],'binary_sha256':r['binary_sha256'],'harness_sha256':r['harness_sha256'],'artifacts':[{'path':p.as_posix(),'sha256':h(p)} for p in (rp,tp,lp,cp)]}
rp.with_suffix('.normalization.json').write_bytes((json.dumps(proof,ensure_ascii=False,indent=2)+'\n').encode())
print(json.dumps({'evidence_integrity':'PASS','actual_driver_exit_code':c['exit_code'],'counts':r['counts'],'report_sha256':h(rp),'transcript_sha256':h(tp)}))
'@
& $python -c $normalizer $reportPath $receiptPath
if ($LASTEXITCODE -ne 0) { throw 'Evidence normalization or hash binding failed' }
Write-Output "DRIVER_EXIT=$driverExit"
exit $driverExit
