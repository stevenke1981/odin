{ Fixed Node worker, responsive progress, cooperative stop and owned process tree.
  SetupArchitecture=x86 is explicit in odin.iss: Win32 record layouts below are x86.
  Deadlines/cancel are enforced by setup-lsps.mjs before its short CAS commit.
  A job owns descendants on installer failure/exit; normal timeout never kills a commit. }
type
  TLspStartupInfo = record
    Size, Reserved, Desktop, Title: LongWord;
    X, Y, XSize, YSize, XCountChars, YCountChars, FillAttribute, Flags: LongWord;
    ShowWindow, ReservedSize: Word;
    ReservedBytes, StdInput, StdOutput, StdError: LongWord;
  end;
  TLspProcessInfo = record
    ProcessHandle, ThreadHandle, ProcessId, ThreadId: LongWord;
  end;
  TLspJobLimits = record
    Prefix: array[0..15] of Byte;
    LimitFlags: LongWord;
    Remainder: array[0..91] of Byte;
  end;
  TLspNonce = array[0..15] of Byte;

function LspCreateProcess(ApplicationName, CommandLine: String;
  ProcessAttributes, ThreadAttributes: LongWord; InheritHandles: Boolean;
  CreationFlags: LongWord; Environment, CurrentDirectory: String;
  var StartupInfo: TLspStartupInfo; var ProcessInfo: TLspProcessInfo): Boolean;
  external 'CreateProcessW@kernel32.dll stdcall';
function LspCreateJob(Attributes, Name: LongWord): LongWord;
  external 'CreateJobObjectW@kernel32.dll stdcall';
function LspSetJobLimits(Job: LongWord; InfoClass: Integer;
  var Limits: TLspJobLimits; InfoSize: LongWord): Boolean;
  external 'SetInformationJobObject@kernel32.dll stdcall';
function LspAssignJob(Job, ProcessHandle: LongWord): Boolean;
  external 'AssignProcessToJobObject@kernel32.dll stdcall';
function LspResumeThread(ThreadHandle: LongWord): LongWord;
  external 'ResumeThread@kernel32.dll stdcall';
function LspTerminateProcess(ProcessHandle, ExitCode: LongWord): Boolean;
  external 'TerminateProcess@kernel32.dll stdcall';
function LspWait(ProcessHandle, Milliseconds: LongWord): LongWord;
  external 'WaitForSingleObject@kernel32.dll stdcall';
function LspProcessExitCode(ProcessHandle: LongWord; var ExitCode: LongWord): Boolean;
  external 'GetExitCodeProcess@kernel32.dll stdcall';
function LspCloseHandle(Handle: LongWord): Boolean;
  external 'CloseHandle@kernel32.dll stdcall';
function LspTickCount: Int64;
  external 'GetTickCount64@kernel32.dll stdcall';
function LspRandom(Algorithm: LongWord; var Buffer: TLspNonce; Size, Flags: LongWord): Integer;
  external 'BCryptGenRandom@bcrypt.dll stdcall';
function LspOpenStatus(Filename: String; Access, Sharing, Security, Disposition,
  Attributes, Template: LongWord): LongWord;
  external 'CreateFileW@kernel32.dll stdcall';
function LspReadStatus(Handle: LongWord; Buffer: AnsiString; Maximum: LongWord;
  var ReadCount: LongWord; Overlapped: LongWord): Boolean;
  external 'ReadFile@kernel32.dll stdcall';

var
  LspProgressPage: TOutputProgressWizardPage;
  LspStopButton: TNewButton;
  LspProgressPath, LspCancelPath, LspRunId, LspPhase, LspComponent: String;
  LspStatusMessage, LspFailureMessage, LspLastLog: String;
  LspCompleted, LspTotal, LspReportedExit: Integer;
  LspCancelRequested, LspSawDone, LspActive, LspDeadlineRequested: Boolean;
  LspStarted: Int64;

function LspSetupTimeout: Integer;
begin
  Result := StrToIntDef(ExpandConstant('{param:ODIN_SETUP_TIMEOUT_SECONDS|900}'), 0);
end;

function LspAscii(Value: String; Maximum: Integer): Boolean;
var I: Integer;
begin
  Result := False;
  if Length(Value) > Maximum then exit;
  for I := 1 to Length(Value) do
    if (Ord(Value[I]) < 32) or (Ord(Value[I]) > 126) then exit;
  Result := True;
end;

function LspTerminalPhase(Phase: String): Boolean;
begin
  Result := (Phase = 'done') or (Phase = 'failed') or
    (Phase = 'cancelled') or (Phase = 'timeout');
end;

function LspValidPhase(Phase: String): Boolean;
begin
  Result := LspTerminalPhase(Phase) or (Phase = 'starting') or
    (Phase = 'verify_before') or (Phase = 'probe') or
    (Phase = 'verify_after') or (Phase = 'applying');
end;

procedure LspReadProgress;
var
  Lines: TArrayOfString;
  Completed, Total, ReportedExit, I: Integer;
  Key: String;
  Handle, ReadCount: LongWord;
  Buffer: AnsiString;
  Text: TStringList;
begin
  { Bound every read and share delete so the worker can atomically replace status.
    No repository input, executable configuration or unbounded file load is used. }
  Handle := LspOpenStatus(LspProgressPath, $80000000, 7, 0, 3, $00200080, 0);
  if Handle = $FFFFFFFF then exit;
  try
    Buffer := StringOfChar(' ', 4097);
    if not LspReadStatus(Handle, Buffer, 4097, ReadCount, 0) then exit;
    if ReadCount > 4096 then exit;
    SetLength(Buffer, ReadCount);
  finally
    LspCloseHandle(Handle);
  end;
  Text := TStringList.Create;
  try
    Text.Text := String(Buffer);
    if Text.Count <> 8 then exit;
    SetArrayLength(Lines, 8);
    for I := 0 to 7 do Lines[I] := Text[I];
  finally
    Text.Free;
  end;
  if (Lines[0] <> 'ODIN_SETUP_PROGRESS_V1') or (Lines[1] <> LspRunId) then exit;
  if not LspValidPhase(Lines[2]) then exit;
  if (not LspAscii(Lines[5], 240)) or (not LspAscii(Lines[7], 240)) then exit;
  Completed := StrToIntDef(Lines[3], -1);
  Total := StrToIntDef(Lines[4], -1);
  ReportedExit := StrToIntDef(Lines[6], -1);
  if (Completed < 0) or (Total < 0) or (Completed > Total) or
    (Total > 100000000) or (ReportedExit < 0) or (ReportedExit > 2147483647) then exit;
  LspPhase := Lines[2];
  LspCompleted := Completed;
  LspTotal := Total;
  LspComponent := Lines[5];
  LspReportedExit := ReportedExit;
  LspStatusMessage := Lines[7];
  if (LspPhase = 'done') and (ReportedExit = 0) then LspSawDone := True;
  Key := LspPhase + '|' + LspComponent;
  if Key <> LspLastLog then begin
    Log('Odin LSP setup phase=' + LspPhase + ', component=' + LspComponent +
      ', elapsed_seconds=' + IntToStr((LspTickCount - LspStarted) div 1000));
    LspLastLog := Key;
  end;
end;

procedure LspRequestStop(Sender: TObject);
var Pending: String;
begin
  if (not LspActive) or LspCancelRequested or (LspPhase = 'applying') or
      LspTerminalPhase(LspPhase) then exit;
  Pending := LspCancelPath + '.request';
  { Rename exposes the complete nonce, never a partially written marker. }
  if (not FileExists(LspCancelPath)) and
      SaveStringToFile(Pending, LspRunId, False) and RenameFile(Pending, LspCancelPath) then begin
    LspCancelRequested := True;
    LspStopButton.Enabled := False;
    Log('Odin LSP stop requested; waiting for the current bounded probe/checkpoint.');
  end else begin
    DeleteFile(Pending);
    Log('Odin LSP stop marker could not be created.');
  end;
end;

procedure LspUpdatePage;
var Heading, Detail: String;
begin
  if LspPhase = 'verify_before' then Heading := '1/4  Verifying bundled files'
  else if LspPhase = 'probe' then Heading := '2/4  Checking language server versions'
  else if LspPhase = 'verify_after' then Heading := '3/4  Verifying files after checks'
  else if LspPhase = 'applying' then Heading := '4/4  Saving host configuration'
  else if LspPhase = 'done' then Heading := 'Language server setup completed'
  else if LspPhase = 'starting' then Heading := 'Starting bundled language server setup'
  else Heading := 'Language server setup needs attention';
  if LspCancelRequested and (LspPhase <> 'applying') and
      (not LspTerminalPhase(LspPhase)) then begin
    Heading := 'Stopping language server checks';
    Detail := 'Waiting for the current check (up to 60 seconds). The request is checked before saving host configuration.';
  end else if LspPhase = 'applying' then
    Detail := 'Finishing the short configuration commit. Please wait.'
  else if LspDeadlineRequested and (not LspTerminalPhase(LspPhase)) then begin
    Heading := 'Time limit reached; waiting for setup worker';
    Detail := 'The worker enforces its own time limit before saving host configuration. A configuration commit is never interrupted.';
  end else begin
    Detail := LspStatusMessage;
    if LspComponent <> '' then Detail := LspComponent + ': ' + Detail;
  end;
  Detail := Detail + #13#10 + 'Elapsed: ' + IntToStr((LspTickCount - LspStarted) div 1000) +
    ' s. Setup limit: ' + IntToStr(LspSetupTimeout) + ' s.';
  LspStopButton.Enabled := LspActive and (not LspCancelRequested) and
    (LspPhase <> 'applying') and (not LspTerminalPhase(LspPhase));
  { Both methods pump Inno's UI messages; never wait on the child synchronously. }
  { SetText treats its second argument as a single path; keep wrapped details
    in the label directly so elapsed/cancellation text remains visible. }
  LspProgressPage.SetText(Heading, '');
  LspProgressPage.Msg2Label.Caption := Detail;
  LspProgressPage.SetProgress(LspCompleted, LspTotal);
end;

procedure InitializeLspProgress;
begin
  LspProgressPage := CreateOutputProgressPage('Odin language server setup',
    'Offline verification, version checks and host configuration');
  LspProgressPage.Msg2Label.Top := ScaleY(28);
  LspProgressPage.Msg2Label.Height := ScaleY(96);
  LspProgressPage.Msg2Label.WordWrap := True;
  LspProgressPage.ProgressBar.Top := ScaleY(136);
  LspStopButton := TNewButton.Create(LspProgressPage);
  LspStopButton.Parent := LspProgressPage.Surface;
  LspStopButton.Caption := 'Stop LSP checks';
  LspStopButton.Width := ScaleX(140);
  LspStopButton.Height := ScaleY(26);
  LspStopButton.Left := 0;
  LspStopButton.Top := LspProgressPage.SurfaceHeight - LspStopButton.Height;
  LspStopButton.OnClick := @LspRequestStop;
end;

function LspWorkerEnvironment: String;
var Names: TArrayOfString; I: Integer; Value: String;
begin
  { No NODE_OPTIONS, credentials, shell startup or arbitrary host variables. }
  Names := ['ALLUSERSPROFILE', 'APPDATA', 'CommonProgramFiles',
    'CommonProgramFiles(x86)', 'CommonProgramW6432', 'HOMEDRIVE', 'HOMEPATH',
    'LOCALAPPDATA', 'PATH', 'ProgramData', 'ProgramFiles', 'ProgramFiles(x86)',
    'ProgramW6432', 'SystemRoot', 'TEMP', 'TMP', 'USERPROFILE', 'WINDIR'];
  Result := '';
  for I := 0 to GetArrayLength(Names) - 1 do begin
    if Names[I] = 'PATH' then Value := ExpandConstant('{sys}') + ';' + ExpandConstant('{win}')
    else Value := GetEnv(Names[I]);
    if Value <> '' then Result := Result + Names[I] + '=' + Value + #0;
  end;
  Result := Result + #0;
end;

function RunLspWorker(Parameters: String): Integer;
var
  Startup: TLspStartupInfo;
  ProcessInfo: TLspProcessInfo;
  Limits: TLspJobLimits;
  Nonce: TLspNonce;
  Job, WaitResult, WorkerExit: LongWord;
  I: Integer;
  Node, Command, Environment, HexDigits: String;
begin
  Result := 1;
  LspFailureMessage := '';
  LspRunId := '';
  if LspRandom(0, Nonce, 16, 2) <> 0 then begin
    LspFailureMessage := 'Could not allocate a private setup run identifier.';
    exit;
  end;
  HexDigits := '0123456789abcdef';
  for I := 0 to 15 do LspRunId := LspRunId +
    HexDigits[Nonce[I] div 16 + 1] + HexDigits[Nonce[I] mod 16 + 1];
  LspProgressPath := ExpandConstant('{tmp}') + '\odin-lsp-' + LspRunId + '.progress';
  LspCancelPath := ExpandConstant('{tmp}') + '\odin-lsp-' + LspRunId + '.cancel';
  if FileExists(LspProgressPath) or FileExists(LspCancelPath) then begin
    LspFailureMessage := 'Private setup status paths already exist.';
    exit;
  end;
  Parameters := Parameters + ' --progress-file ' + QuoteWindowsArg(LspProgressPath) +
    ' --cancel-file ' + QuoteWindowsArg(LspCancelPath) +
    ' --progress-run-id ' + LspRunId +
    ' --setup-timeout-seconds ' + IntToStr(LspSetupTimeout);
  Job := LspCreateJob(0, 0);
  if Job = 0 then begin
    LspFailureMessage := 'Could not create the setup process job.';
    exit;
  end;
  try
    { Inno Pascal records are zero-initialized. x86 extended job limits are 112 bytes. }
    Limits.LimitFlags := $2000; { JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE }
    if not LspSetJobLimits(Job, 9, Limits, 112) then begin
      LspFailureMessage := 'Could not configure setup process cleanup.';
      exit;
    end;
    Startup.Size := 68;
    Startup.Flags := 1; { STARTF_USESHOWWINDOW }
    Startup.ShowWindow := SW_HIDE;
    Node := ExpandConstant('{app}\backends\node\node.exe');
    Command := QuoteWindowsArg(Node) + ' ' + Parameters;
    Environment := LspWorkerEnvironment;
    { CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT. }
    if not LspCreateProcess(Node, Command, 0, 0, False, $08000404,
        Environment, ExpandConstant('{app}'), Startup, ProcessInfo) then begin
      LspFailureMessage := 'Could not launch the fixed bundled Node setup worker.';
      exit;
    end;
    try
      if not LspAssignJob(Job, ProcessInfo.ProcessHandle) then begin
        LspTerminateProcess(ProcessInfo.ProcessHandle, 1);
        LspFailureMessage := 'Could not attach the setup worker to its cleanup job.';
        exit;
      end;
      if LspResumeThread(ProcessInfo.ThreadHandle) = $FFFFFFFF then begin
        LspFailureMessage := 'Could not start the setup worker thread.';
        exit;
      end;
      LspActive := True;
      LspCancelRequested := False;
      LspDeadlineRequested := False;
      LspSawDone := False;
      LspPhase := 'starting';
      LspStatusMessage := 'Preparing offline language server checks.';
      LspComponent := '';
      LspLastLog := '';
      LspCompleted := 0;
      LspTotal := 0;
      LspStarted := LspTickCount;
      LspProgressPage.Show;
      try
        repeat
          LspReadProgress;
          if (LspTickCount - LspStarted >= Int64(LspSetupTimeout) * 1000) and
              (not LspDeadlineRequested) and (LspPhase <> 'applying') and
              (not LspTerminalPhase(LspPhase)) then begin
            LspDeadlineRequested := True;
            { Worker time starts after process/module startup. A UI timer must not
              create a manual-cancel marker and misreport timeout as cancellation. }
            Log('Odin LSP UI time limit reached; waiting for the worker deadline.');
          end;
          LspUpdatePage;
          WaitResult := LspWait(ProcessInfo.ProcessHandle, 0);
          if WaitResult = 0 then break;
          if WaitResult <> $00000102 then begin
            LspFailureMessage := 'Could not inspect the setup process state.';
            exit;
          end;
          Sleep(50);
        until False;
        LspReadProgress;
        if not LspProcessExitCode(ProcessInfo.ProcessHandle, WorkerExit) then begin
          LspFailureMessage := 'Could not read the setup process exit code.';
          exit;
        end;
        if WorkerExit > $7FFFFFFF then Result := 1 else Result := WorkerExit;
        if (Result = 0) and ((not LspSawDone) or (LspPhase <> 'done') or (LspReportedExit <> 0)) then begin
          Result := 1;
          LspFailureMessage := 'The worker exited without a matching completed setup status.';
        end else if Result <> 0 then
          LspFailureMessage := 'Language server setup stopped in phase ' + LspPhase +
            ' (exit ' + IntToStr(Result) + '). ' + LspStatusMessage;
        Log('Odin LSP setup worker exit=' + IntToStr(Result) +
          ', elapsed_seconds=' + IntToStr((LspTickCount - LspStarted) div 1000));
      finally
        LspActive := False;
        LspProgressPage.Hide;
      end;
    finally
      LspCloseHandle(ProcessInfo.ThreadHandle);
      LspCloseHandle(ProcessInfo.ProcessHandle);
    end;
  finally
    { Normal completion also reaps any version-check descendants. Closing/crashing
      the installer closes this non-inheritable job handle and cleans the tree. }
    LspCloseHandle(Job);
  end;
end;
