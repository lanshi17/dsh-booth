# Hosted Windows CI has no privately provisioned voice tool repository.
# Reuse the reviewed offline installer and the upstream's hash-pinned inputs.
$ErrorActionPreference = "Stop"
$Manifest = Get-Content -Raw (Join-Path $PSScriptRoot "voice-cygwin-snapshot.json") | ConvertFrom-Json
$Directory = Join-Path $env:RUNNER_TEMP "voice-cygwin-snapshot"
New-Item -ItemType Directory -Path $Directory | Out-Null
foreach ($Pin in @($Manifest.archive, $Manifest.sourceArchive)) {
    $Destination = Join-Path $Directory $Pin.name
    $Url = "https://github.com/openai/codex/releases/download/$($Manifest.archive.tag)/$($Pin.name)"
    & curl.exe --fail --location --retry 3 --output $Destination $Url
    if ($LASTEXITCODE -ne 0) { throw "Cannot obtain pinned public Cygwin input: $($Pin.name)" }
    if ((Get-Item $Destination).Length -ne $Pin.bytes -or
        (Get-FileHash $Destination -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Pin.sha256) {
        throw "Public Cygwin archive mismatch: $($Pin.name)"
    }
}
$HostArch = $env:PROCESSOR_ARCHITEW6432
if (-not $HostArch) { $HostArch = $env:PROCESSOR_ARCHITECTURE }
switch ($HostArch) {
    "AMD64" { $Target = "x86_64-pc-windows-msvc" }
    "ARM64" { $Target = "aarch64-pc-windows-msvc" }
    default { throw "Unsupported native Windows host architecture: $HostArch" }
}
if (-not $env:SystemRoot) { throw "Windows SystemRoot is required for native voice actions" }
& (Join-Path $PSScriptRoot "setup-voice-windows.ps1") -Target $Target -SnapshotArchive (Join-Path $Directory $Manifest.archive.name)
"VOICE_WINDOWS_SYSTEM_ROOT=$env:SystemRoot" | Out-File $env:GITHUB_ENV -Encoding utf8 -Append
"VOICE_WINDOWS_HOST_ARCH=$HostArch" | Out-File $env:GITHUB_ENV -Encoding utf8 -Append
