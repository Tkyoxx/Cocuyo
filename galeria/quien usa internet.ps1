# Dice qué programas tienen más conexiones a internet abiertas ahora mismo.
$conexiones = Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue | Where-Object { $_.RemoteAddress -notmatch "^(127\.|::1$|0\.0\.0\.0)" }
if (-not $conexiones) { "Ahora mismo nada está conectado a internet."; exit }
$nombres = @{}
Get-Process | ForEach-Object { $nombres[[int]$_.Id] = $_.ProcessName }
$top = $conexiones | Group-Object { $nombres[[int]$_.OwningProcess] } | Where-Object { $_.Name } | Sort-Object Count -Descending | Select-Object -First 4
"Los que más conexiones tienen: " + (($top | ForEach-Object { "{0} con {1}" -f $_.Name, $_.Count }) -join ", ") + "."
