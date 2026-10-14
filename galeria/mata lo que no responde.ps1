# Cierra a la fuerza los programas que se quedaron colgados, los que Windows marca como «No responde».
$colgados = Get-Process | Where-Object { $_.MainWindowHandle -ne 0 -and -not $_.Responding }
if (-not $colgados) { "No hay nada colgado. Todo responde."; exit }
$nombres = ($colgados | ForEach-Object { $_.ProcessName } | Sort-Object -Unique) -join ", "
$colgados | Stop-Process -Force -ErrorAction SilentlyContinue
"Cerré lo que no respondía: $nombres."
