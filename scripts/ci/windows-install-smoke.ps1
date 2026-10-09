#Requires -Version 7
# Silent-installs the NSIS bundle and proves the bundled uv can build a working
# engine from the shipped Windows CPU lock. No secrets, no model downloads.
param(
  [Parameter(Mandatory = $true)][string]$Installer
)

$ErrorActionPreference = 'Stop'

function Assert-Native([string]$What) {
  if ($LASTEXITCODE -ne 0) { throw "$What failed with exit code $LASTEXITCODE" }
}

$proc = Start-Process -FilePath $Installer -ArgumentList '/S' -Wait -PassThru
if ($proc.ExitCode -ne 0) { throw "installer exited with $($proc.ExitCode)" }

# Tauri's currentUser default is %LOCALAPPDATA%\Galpi, but that is unverified,
# so locate the install by its uv.exe.
$uvFile = Get-ChildItem -Path $env:LOCALAPPDATA -Filter uv.exe -Recurse -File -ErrorAction SilentlyContinue |
  Where-Object { $_.FullName -notmatch '\\(uv|Temp)\\' } |
  Select-Object -First 1
if (-not $uvFile) { throw "uv.exe not found under $env:LOCALAPPDATA" }
$install = $uvFile.DirectoryName
Write-Host "install dir: $install"

$uv = Join-Path $install 'uv.exe'
$worker = Join-Path $install 'resources\worker'
$lock = Join-Path $worker 'requirements-windows-cpu.lock'
foreach ($p in @($uv, (Join-Path $worker 'galpi_worker'), $lock)) {
  if (-not (Test-Path $p)) { throw "missing installed file: $p" }
}

$venv = Join-Path $env:RUNNER_TEMP 'galpi-smoke-venv'
if (-not $env:RUNNER_TEMP) { $venv = Join-Path $env:TEMP 'galpi-smoke-venv' }

& $uv --version; Assert-Native 'uv --version'
& $uv python install 3.12; Assert-Native 'uv python install'
& $uv venv --python 3.12 $venv; Assert-Native 'uv venv'
$python = Join-Path $venv 'Scripts\python.exe'
& $uv pip install --python $python -r $lock --require-hashes; Assert-Native 'uv pip install'

# CI-only copy of platform.rs `worker_environment(Os::Windows)`; keep in sync.
$sysRoot = $env:SystemRoot
$envKeys = @{}
foreach ($k in 'USERPROFILE', 'SYSTEMROOT', 'WINDIR', 'TEMP', 'TMP', 'LOCALAPPDATA', 'APPDATA', 'COMSPEC', 'PATHEXT', 'HOMEDRIVE', 'HOMEPATH') {
  $v = [Environment]::GetEnvironmentVariable($k)
  if ($v) { $envKeys[$k] = $v }
}
$envKeys['PATH'] = "$(Join-Path $venv 'Scripts');$sysRoot\System32;$sysRoot"
$cache = Join-Path $venv 'cache'
$envKeys['PYTHONUTF8'] = '1'
$envKeys['PYTHONSAFEPATH'] = '1'
$envKeys['PYTHONDONTWRITEBYTECODE'] = '1'
$envKeys['PYTHONPATH'] = $worker
$envKeys['HF_HOME'] = Join-Path $cache 'huggingface'
$envKeys['TORCH_HOME'] = Join-Path $cache 'torch'
$envKeys['HF_HUB_DISABLE_IMPLICIT_TOKEN'] = '1'
$envKeys['HF_HUB_DISABLE_TELEMETRY'] = '1'
$envKeys['PYANNOTE_METRICS_ENABLED'] = 'false'
$envKeys['DO_NOT_TRACK'] = '1'
$envKeys['UV_PYTHON_INSTALL_DIR'] = Join-Path $cache 'python'
$envKeys['UV_CACHE_DIR'] = Join-Path $cache 'uv'
$envKeys['UV_PYTHON_PREFERENCE'] = 'only-managed'
$envKeys['HF_HUB_DISABLE_SYMLINKS_WARNING'] = '1'

$psi = [System.Diagnostics.ProcessStartInfo]::new()
$psi.FileName = $python
$psi.UseShellExecute = $false
$psi.Environment.Clear()
foreach ($kv in $envKeys.GetEnumerator()) { $psi.Environment[$kv.Key] = $kv.Value }

function Invoke-Engine([string[]]$PyArgs) {
  $psi.ArgumentList.Clear()
  foreach ($a in $PyArgs) { $psi.ArgumentList.Add($a) }
  $p = [System.Diagnostics.Process]::Start($psi)
  $p.WaitForExit()
  if ($p.ExitCode -ne 0) { throw "python $($PyArgs -join ' ') failed with exit code $($p.ExitCode)" }
}

Invoke-Engine @('-c', 'import whisperx, torch, pyannote.audio')
Invoke-Engine @('-m', 'galpi_worker', '--help')
Write-Host 'windows install smoke OK'
