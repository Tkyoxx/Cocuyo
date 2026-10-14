# Muestra qué programas tienen más conexiones a internet abiertas ahora mismo.
$conexiones = Get-NetTCPConnection -State Established -ErrorAction SilentlyContinue | Where-Object { $_.RemoteAddress -notmatch "^(127\.|::1$|0\.0\.0\.0)" }
if (-not $conexiones) { "Ahora mismo nada está conectado a internet."; exit }
$nombres = @{}
Get-Process | ForEach-Object { $nombres[[int]$_.Id] = $_.ProcessName }
$top = $conexiones | Group-Object { $nombres[[int]$_.OwningProcess] } | Where-Object { $_.Name } | Sort-Object Count -Descending | Select-Object -First 5
$mas = ($top | Select-Object -First 1).Count
Tarjeta -Titulo "Quién usa internet" -Icono "🌐" -Filas @($top | ForEach-Object { Fila $_.Name "$($_.Count)" ($_.Count / $mas) }) -Segundos 14
Decir ("Los que más conexiones tienen: " + (($top | Select-Object -First 3 | ForEach-Object { $_.Name }) -join ", ") + ".")
