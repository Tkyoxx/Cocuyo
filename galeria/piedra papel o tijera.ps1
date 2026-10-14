# Juega una ronda: dile piedra, papel o tijera.
param($jugada)
$cosas = "piedra", "papel", "tijera"
$tuya = $cosas | Where-Object { "$jugada" -match $_ } | Select-Object -First 1
if (-not $tuya) { "Dime piedra, papel o tijera. Así no vale."; exit }
$mia = $cosas | Get-Random
if ($tuya -eq $mia) { "Yo también saqué $mia. Empate." }
elseif (($cosas.IndexOf($tuya) - $cosas.IndexOf($mia) + 3) % 3 -eq 1) { "Saqué $mia. Ganaste, pero fue suerte." }
else { "Saqué $mia. Gané yo. Otra cuando quieras." }
