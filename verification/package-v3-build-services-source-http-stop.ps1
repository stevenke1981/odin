$ErrorActionPreference = 'Stop'
$ownedHttp = Get-CimInstance Win32_Process -Filter 'ProcessId=29504'
if ($null -eq $ownedHttp) { throw 'Expected task-owned source HTTP helper is absent before stop' }
$expectedCommand = '"C:\Python314\python.exe" -m http.server 47652 --bind 127.0.0.1 --directory E:\odin\.odin\validation-tools\packaging-v3\transfer'
if ($ownedHttp.ExecutablePath -ne 'C:\Python314\python.exe' -or $ownedHttp.CommandLine -ne $expectedCommand) {
    throw 'Source HTTP PID does not match the task-owned executable and exact command'
}
Stop-Process -Id 29504 -ErrorAction Stop
[pscustomobject]@{ Scope='TASK_ONLY_READ_ONLY_SOURCE_HTTP_STOP'; ProcessId=29504; ExecutablePath=$ownedHttp.ExecutablePath; CommandLine=$ownedHttp.CommandLine; StopSucceeded=$true } | ConvertTo-Json -Depth 4
