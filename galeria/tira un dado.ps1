# Tira un dado. Si le dices un número, ese es el de caras.
param($caras)
$n = 6
if ($caras -match "^\d+$" -and [int]$caras -gt 1) { $n = [int]$caras }
"Salió un " + (Get-Random -Minimum 1 -Maximum ($n + 1)) + "."
