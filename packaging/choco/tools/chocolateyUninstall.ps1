$ErrorActionPreference = 'Stop'

$appId = '{8C6B9A2E-52C1-4E63-9B7A-7C1F4A2B9D33}_is1'
$silentArgs = '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART'

$regPaths = @(
    "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$appId",
    "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$appId",
    "HKLM:\Software\Wow6432Node\Microsoft\Windows\CurrentVersion\Uninstall\$appId"
)

$uninstaller = $null
foreach ($path in $regPaths) {
    if (Test-Path $path) {
        $props = Get-ItemProperty -Path $path -ErrorAction SilentlyContinue
        if ($props -and $props.UninstallString) {
            $raw = $props.UninstallString.Trim()
            if ($raw.StartsWith('"')) {
                $closingQuote = $raw.IndexOf('"', 1)
                if ($closingQuote -gt 1) {
                    $uninstaller = $raw.Substring(1, $closingQuote - 1)
                } else {
                    $uninstaller = $raw.Trim('"')
                }
            } else {
                $uninstaller = ($raw -split '\s+')[0]
            }
            break
        }
    }
}

if (-not $uninstaller) {
    $fallback = Join-Path $env:LOCALAPPDATA "Programs\RightType\unins000.exe"
    if (Test-Path $fallback) { $uninstaller = $fallback }
}

if ($uninstaller -and (Test-Path $uninstaller)) {
    $proc = Start-Process -FilePath $uninstaller -ArgumentList $silentArgs -Wait -PassThru
    if ($proc.ExitCode -ne 0) {
        Write-Warning "Uninstaller exited with code $($proc.ExitCode)"
    }
} else {
    Write-Warning "Uninstall string for $appId not found in registry."
}
