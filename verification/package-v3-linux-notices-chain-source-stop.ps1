$ErrorActionPreference = 'Stop'
$ownedHttp = Get-CimInstance Win32_Process -Filter 'ProcessId=40740'
if ($null -eq $ownedHttp) { throw 'Expected task-owned notice source HTTP helper is absent before stop' }
$expectedCommand = 'C:\Python314\python.exe -m http.server 47652 --bind 127.0.0.1 --directory E:\odin\.odin\validation-tools\packaging-v3\transfer'
if ($ownedHttp.ExecutablePath -ne 'C:\Python314\python.exe' -or $ownedHttp.CommandLine -ne $expectedCommand) { throw 'Notice source HTTP PID no longer matches the task-owned exact command' }
Stop-Process -Id 40740 -ErrorAction Stop
[pscustomobject]@{ Scope='TASK_ONLY_NOTICE_SOURCE_HTTP_STOP'; ProcessId=40740; ExecutablePath=$ownedHttp.ExecutablePath; CommandLine=$ownedHttp.CommandLine; StopSucceeded=$true } | ConvertTo-Json -Depth 4
