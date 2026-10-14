# Inventa una clave fuerte y la deja copiada. Si le dices un número, ese es el largo.
param($largo)
$n = 16
if ($largo -match "^\d+$" -and [int]$largo -ge 8 -and [int]$largo -le 64) { $n = [int]$largo }
$letras = "abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789!#%*+-=?@"
$bytes = New-Object byte[] $n
[Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
$clave = -join ($bytes | ForEach-Object { $letras[$_ % $letras.Length] })
Set-Clipboard $clave
"Clave de $n caracteres copiada. No la digo en voz alta, que nunca se sabe quién escucha."
