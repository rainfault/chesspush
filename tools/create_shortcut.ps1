param([string]$DesktopPath = [Environment]::GetFolderPath('Desktop'))
$ErrorActionPreference = 'Stop'
$projectPath = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$executablePath = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'ChessPush\chesspush.exe'
$installationPath = Split-Path -Parent $executablePath
foreach ($requiredPath in @($executablePath)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) { throw "Missing: $requiredPath" }
}
$shortcutPath = Join-Path $DesktopPath 'ChessPush.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
if (Test-Path -LiteralPath $shortcutPath) {
    $existingTarget = [IO.Path]::GetFullPath($shortcut.TargetPath)
    $installedTarget = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'ChessPush\chesspush.exe'
    $projectTarget = $existingTarget.StartsWith($projectPath + '\',[StringComparison]::OrdinalIgnoreCase)
    $installedChessPush = [string]::Equals($existingTarget,$installedTarget,[StringComparison]::OrdinalIgnoreCase)
    if (-not $projectTarget -and -not $installedChessPush) {
        throw 'An unrelated ChessPush shortcut already exists.'
    }
}
$shortcut.TargetPath = $executablePath
$shortcut.Arguments = ''
$shortcut.WorkingDirectory = $installationPath
$shortcut.IconLocation = "$executablePath,0"
$shortcut.Description = 'ChessPush'
$shortcut.WindowStyle = 1
$shortcut.Save()
$verified = $shell.CreateShortcut($shortcutPath)
if ($verified.TargetPath -ne $executablePath -or $verified.WorkingDirectory -ne $installationPath) {
    throw 'Shortcut verification failed'
}
Write-Output $shortcutPath
