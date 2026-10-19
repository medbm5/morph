# S19 end-to-end: recipes hot-reload without restart, and a watch folder runs new
# files through a recipe. Uses portable mode next to the debug build, so the user's
# own settings and recipes are untouched. File-system only: no input is simulated.
#
#   pwsh app/tests/e2e/watch-e2e.ps1 [-Exe target\debug\metamorphosis.exe]
param([string]$Exe = "$PSScriptRoot\..\..\..\target\debug\metamorphosis.exe")
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$dir = Split-Path $Exe
$marker = Join-Path $dir 'portable'
if (Get-Process metamorphosis -ErrorAction SilentlyContinue) { throw 'Metamorphosis is already running; quit it first.' }
$work = Join-Path ([IO.Path]::GetTempPath()) ("morph-watch-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
$in = Join-Path $work 'in'
New-Item -ItemType Directory -Force $in | Out-Null
$fail = 0
function Check($ok, $msg) { if ($ok) { Write-Host "OK   $msg" } else { Write-Host "FAIL $msg"; $script:fail++ } }

Set-Content $marker 'e2e'
$config = Join-Path $dir 'config'
New-Item -ItemType Directory -Force (Join-Path $config 'recipes') | Out-Null
@"
autostart = false

[[watch]]
folder = '$in'
op = "recipe.e2e-duplicate"
out = "Done"
"@ | Set-Content -Encoding utf8 (Join-Path $config 'settings.toml')

$p = Start-Process $Exe -ArgumentList '--background' -PassThru
try {
  # Wait for the app to start (a cold start on CI can take a while).
  $log = Join-Path $dir 'data\morph.log'
  $deadline = (Get-Date).AddSeconds(90)
  # Ready once the watch folders are set up (the last step of start-up).
  do { Start-Sleep -Milliseconds 500; $ready = (Test-Path $log) -and (Select-String -Path $log -Pattern 'watching .+ recipe\.e2e-duplicate' -Quiet) }
  until ($ready -or (Get-Date) -gt $deadline)
  $m0 = (Select-String -Path $log -Pattern '(\d+) operations registered').Matches
  if (-not $m0) { throw "the app didn't start within 90 s" }
  Start-Sleep 2
  $before = $m0[-1].Groups[1].Value

  # 1. Drop a recipe in: it registers without a restart.
  @'
name = "E2E duplicate"
ring = "tools"
output = "{stem} Copy.{ext}"
run = ["cmd", "/c", "copy \"{input}\" \"{output}\" >NUL"]
'@ | Set-Content -Encoding utf8 (Join-Path $config 'recipes\e2e.toml')
  $deadline = (Get-Date).AddSeconds(10)
  do { Start-Sleep -Milliseconds 300; $m = (Select-String -Path $log -Pattern '(\d+) operations registered').Matches }
  until ($m.Count -ge 2 -or (Get-Date) -gt $deadline)
  $after = $m[-1].Groups[1].Value
  Check ($m.Count -ge 2 -and [int]$after -eq [int]$before + 1) "recipe hot-reloaded ($before -> $after operations)"

  # 2. A new file in the watch folder runs through the recipe.
  Set-Content (Join-Path $in 'notes.txt') 'hello'
  $out = Join-Path $in 'Done\notes Copy.txt'
  $deadline = (Get-Date).AddSeconds(15)
  while (-not (Test-Path $out) -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 300 }
  Check (Test-Path $out) 'watch folder produced Done\notes Copy.txt'
  if (Test-Path $out) { Check ((Get-Content $out) -eq 'hello') 'output content matches' }
  Start-Sleep 2
  Check (-not (Test-Path (Join-Path $in 'Done\notes Copy Copy.txt'))) 'outputs are not re-processed'

  # 3. Removing the recipe unregisters it.
  Remove-Item (Join-Path $config 'recipes\e2e.toml')
  $deadline = (Get-Date).AddSeconds(10)
  do { Start-Sleep -Milliseconds 300; $m2 = (Select-String -Path $log -Pattern '(\d+) operations registered').Matches }
  until ($m2.Count -gt $m.Count -or (Get-Date) -gt $deadline)
  Check ([int]$m2[-1].Groups[1].Value -eq [int]$before) 'removed recipe disappears'
}
finally {
  if ($fail -and (Test-Path $log)) { Write-Host '--- morph.log'; Get-Content $log | Select-Object -Last 25 }
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  Start-Sleep 1
  Remove-Item $marker -ErrorAction SilentlyContinue
  Remove-Item -Recurse -Force $config, (Join-Path $dir 'data'), $work -ErrorAction SilentlyContinue
}
if ($fail) { exit 1 }
