param([string]$DesktopPath = [Environment]::GetFolderPath('Desktop'))
$ErrorActionPreference = 'Stop'
$projectPath = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$pythonPath = Join-Path $projectPath '.venv\Scripts\pythonw.exe'
$launcherPath = Join-Path $projectPath 'launch.pyw'
$iconPath = Join-Path $projectPath 'ui\qml\assets\chesspush.ico'
foreach ($requiredPath in @($pythonPath, $launcherPath, $iconPath)) {
    if (-not (Test-Path -LiteralPath $requiredPath -PathType Leaf)) { throw "Missing: $requiredPath" }
}
$shortcutPath = Join-Path $DesktopPath 'ChessPush.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($shortcutPath)
if ((Test-Path -LiteralPath $shortcutPath) -and $shortcut.WorkingDirectory -ne $projectPath) {
    throw 'An unrelated ChessPush shortcut already exists.'
}
$shortcut.TargetPath = $pythonPath
$shortcut.Arguments = '"' + $launcherPath + '"'
$shortcut.WorkingDirectory = $projectPath
$shortcut.IconLocation = "$iconPath,0"
$shortcut.Description = 'ChessPush'
$shortcut.WindowStyle = 1
$shortcut.Save()
$verified = $shell.CreateShortcut($shortcutPath)
if ($verified.TargetPath -ne $pythonPath -or $verified.WorkingDirectory -ne $projectPath) { throw 'Shortcut verification failed' }
Write-Output $shortcutPath
