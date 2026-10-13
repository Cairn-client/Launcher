# Starts a published master server (default: the Release publish of dotnet\Cairn.MasterServer) and exercises what it does, over its real sockets:
# the server list, a relay announcing itself, and two friends seeing each other and inviting. Needed because the published server is trimmed,
# and a trimmed ASP.NET app fails at run time (not at build time) when something it uses by reflection was cut out.
#   tools\Test-MasterServer.ps1 [-Dir <folder with CairnMasterServer.exe>]
param([string]$Dir = (Join-Path $PSScriptRoot '..\dotnet\Cairn.MasterServer\bin\Release\net8.0\publish'))

$ErrorActionPreference = 'Stop'
$exe = Join-Path $Dir 'CairnMasterServer.exe'
if (-not (Test-Path $exe)) { throw "no CairnMasterServer.exe in $Dir (publish it first: dotnet publish dotnet\Cairn.MasterServer -c Release -p:PublishProfile=FolderProfile)" }
if (Get-NetTCPConnection -LocalPort 5100 -State Listen -ErrorAction SilentlyContinue) { throw 'port 5100 is in use: close the launcher (it starts its own master server) and try again' }

$none = [Threading.CancellationToken]::None
function Connect($path) { $ws = New-Object Net.WebSockets.ClientWebSocket; $ws.ConnectAsync([uri]"ws://127.0.0.1:5100$path", $none).Wait(); $ws }
function Send($ws, $obj) { $b = [Text.Encoding]::UTF8.GetBytes(($obj | ConvertTo-Json -Depth 6 -Compress)); $ws.SendAsync([ArraySegment[byte]]$b, 'Text', $true, $none).Wait() }
function Receive($ws) {
    $buf = New-Object byte[] 8192; $cts = New-Object Threading.CancellationTokenSource 4000
    try { $r = $ws.ReceiveAsync([ArraySegment[byte]]$buf, $cts.Token).Result; [Text.Encoding]::UTF8.GetString($buf, 0, $r.Count) } catch { '' }
}
function Check($what, $ok) { if ($ok) { Write-Host "ok   $what" } else { Write-Host "FAIL $what"; $script:failed = $true } }

$failed = $false
$srv = Start-Process $exe -WorkingDirectory $Dir -PassThru -WindowStyle Hidden
try {
    Start-Sleep 4
    Check 'the server list answers' ((Invoke-WebRequest 'http://127.0.0.1:5100/api/v1/servers' -UseBasicParsing).Content -like '*"servers":*')
    Check 'the status answers' ((Invoke-WebRequest 'http://127.0.0.1:5100/api/v1/status' -UseBasicParsing).Content -like '*"players":*')

    $relay = Connect '/api/v1/servers/connect'
    Send $relay @{ type = 'announce'; apiVersion = 1; announce = @{ name = 'Smoke'; releaseVersion = '0.0.0'; protocolVersion = 1; port = 7778; infoPort = 7779; mapName = 'x'; players = 1; maxPlayers = 4; tags = @() } }
    Check 'an announced server is accepted' ((Receive $relay) -like '*"type":"accepted"*')
    Check 'and listed' ((Invoke-WebRequest 'http://127.0.0.1:5100/api/v1/servers' -UseBasicParsing).Content -like '*"name":"Smoke"*')

    $idA = '76561198000000001'; $idB = '76561198000000002'
    $a = Connect '/api/v1/presence/connect'; Send $a @{ type = 'hello'; apiVersion = 1; hello = @{ id = $idA; name = 'A'; friends = @($idB); status = @{ activity = 'online'; game = '' } } }
    Check 'presence: a hello is welcomed' ((Receive $a) -like '*"type":"welcome"*')
    $b = Connect '/api/v1/presence/connect'; Send $b @{ type = 'hello'; apiVersion = 1; hello = @{ id = $idB; name = 'B'; friends = @($idA); status = @{ activity = 'online'; game = '' } } }
    $null = Receive $b
    Check 'presence: a friend is seen' ((Receive $b) -like '*"online":true*')
    $null = Receive $a
    Send $a @{ type = 'invite'; apiVersion = 1; invite = @{ to = $idB; game = 'G'; code = 'abc' } }
    Check 'presence: an invitation arrives' ((Receive $b) -like '*"type":"invited"*')
}
finally { Stop-Process -Id $srv.Id -Force -ErrorAction SilentlyContinue }
if ($failed) { exit 1 }
Write-Host 'the master server works'
