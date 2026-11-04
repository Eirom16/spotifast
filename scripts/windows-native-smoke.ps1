param([Parameter(Mandatory=$true)][string]$Binary,[Parameter(Mandatory=$true)][string]$Output)
$ErrorActionPreference='Stop'
$exe=(Resolve-Path $Binary).Path
New-Item -ItemType Directory -Force $Output | Out-Null
$smokeOutput=(Resolve-Path $Output).Path
$report=@{source_commit=$env:GITHUB_SHA;sha256=(Get-FileHash $exe -Algorithm SHA256).Hash.ToLower();physical_windows_hardware=$false;gui_observed=$false;audio_probe_succeeded=$false}
# Own generated profile; never the runner user's default Doremi data directory.
$smokeProfile=Join-Path ([IO.Path]::GetTempPath()) ('doremi-smoke-'+[Guid]::NewGuid().ToString('N'))
$previousDataDir=$env:DOREMI_DATA_DIR
$env:DOREMI_DATA_DIR=$smokeProfile
try {
    $audio=Start-Process -FilePath $exe -ArgumentList '--audio-probe' -PassThru -RedirectStandardOutput (Join-Path $smokeOutput 'audio.json') -RedirectStandardError (Join-Path $smokeOutput 'audio-stderr.txt')
    if ($audio.WaitForExit(60000)) {
        $report.audio_exit_code=$audio.ExitCode
        $report.audio_probe_succeeded=($audio.ExitCode -eq 0)
    } else {$audio.Kill();$audio.WaitForExit();$report.audio_timeout=$true}
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class DoremiSmokeWindow {
    [StructLayout(LayoutKind.Sequential)] public struct Bounds {public int Left,Top,Right,Bottom;}
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd,out Bounds bounds);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd,uint message,IntPtr wParam,IntPtr lParam);
}
'@
    $gui=Start-Process -FilePath $exe -PassThru -RedirectStandardOutput (Join-Path $smokeOutput 'gui-stdout.txt') -RedirectStandardError (Join-Path $smokeOutput 'gui-stderr.txt')
    try {
        $deadline=[DateTime]::UtcNow.AddSeconds(20)
        while ([DateTime]::UtcNow -lt $deadline -and !$gui.HasExited) {
            $gui.Refresh()
            if ($gui.MainWindowHandle -ne 0 -and (Test-Path (Join-Path $smokeProfile 'qa-state.json'))) {break}
            Start-Sleep -Milliseconds 200
        }
        $gui.Refresh()
        if (!$gui.HasExited -and $gui.MainWindowHandle -ne 0 -and (Test-Path (Join-Path $smokeProfile 'qa-state.json'))) {
            Start-Sleep -Milliseconds 500
            $report.gui_observed=$true
            Copy-Item (Join-Path $smokeProfile 'qa-state.json') (Join-Path $smokeOutput 'gui-state.json')
            try {
                Add-Type -AssemblyName System.Drawing
                $bounds=New-Object DoremiSmokeWindow+Bounds
                if (![DoremiSmokeWindow]::GetWindowRect($gui.MainWindowHandle,[ref]$bounds)) {throw 'Own window bounds unavailable'}
                $width=$bounds.Right-$bounds.Left;$height=$bounds.Bottom-$bounds.Top
                if ($width -le 0 -or $height -le 0 -or $width -gt 8192 -or $height -gt 8192) {throw 'Own window bounds invalid'}
                $bitmap=New-Object System.Drawing.Bitmap($width,$height)
                try {
                    $graphics=[System.Drawing.Graphics]::FromImage($bitmap)
                    try {$graphics.CopyFromScreen($bounds.Left,$bounds.Top,0,0,$bitmap.Size)} finally {$graphics.Dispose()}
                    $bitmap.Save((Join-Path $smokeOutput 'gui.png'),[System.Drawing.Imaging.ImageFormat]::Png)
                    $report.gui_capture=$true
                } finally {$bitmap.Dispose()}
            } catch {$report.gui_capture=$false;$report.capture_error=$_.Exception.Message}
            [void][DoremiSmokeWindow]::PostMessage($gui.MainWindowHandle,0x0010,[IntPtr]::Zero,[IntPtr]::Zero)
            if (!$gui.WaitForExit(5000)) {$gui.Kill();$gui.WaitForExit();$report.gui_forced_shutdown=$true}
        } elseif ($gui.HasExited) {$report.gui_exit_code=$gui.ExitCode}
    } finally {if (!$gui.HasExited) {$gui.Kill();$gui.WaitForExit()}}
} finally {
    $env:DOREMI_DATA_DIR=$previousDataDir
    $report | ConvertTo-Json -Depth 8 | Set-Content -Encoding utf8 (Join-Path $smokeOutput 'report.json')
}
# Build identity, native GUI observation and native callback audio are distinct
# facts. A missing device/display is recorded, never reported as passed playback.
