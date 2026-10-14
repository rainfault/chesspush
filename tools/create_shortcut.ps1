param([string]$DesktopPath = [Environment]::GetFolderPath('Desktop'))
$ErrorActionPreference = 'Stop'
$projectPath = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$executablePath = Join-Path $projectPath 'src-tauri\target\release\chesspush.exe'
$iconPath = Join-Path $projectPath 'src-tauri\icons\icon.ico'
foreach ($requiredPath in @($executablePath,$iconPath)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) { throw "Missing: $requiredPath" }
}
$shortcutPath = Join-Path $DesktopPath 'ChessPush.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
if ((Test-Path -LiteralPath $shortcutPath) -and $shortcut.WorkingDirectory -ne $projectPath) {
    throw 'An unrelated ChessPush shortcut already exists.'
}
$shortcut.TargetPath = $executablePath
$shortcut.Arguments = ''
$shortcut.WorkingDirectory = $projectPath
$shortcut.IconLocation = "$iconPath,0"
$shortcut.Description = 'ChessPush'
$shortcut.WindowStyle = 1
$shortcut.Save()
$verified = $shell.CreateShortcut($shortcutPath)
if ($verified.TargetPath -ne $executablePath -or $verified.WorkingDirectory -ne $projectPath) {
    throw 'Shortcut verification failed'
}
Write-Output $shortcutPath
