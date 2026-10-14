# Elige por ti entre lo que le digas: «elige pizza o hamburguesa».
param($opciones)
$lista = "$opciones" -split "\s+o\s+|\s*,\s*" | Where-Object { $_.Trim() }
if ($lista.Count -lt 2) { "Dame al menos dos opciones, separadas por una o."; exit }
"Yo digo que " + ($lista | Get-Random).Trim() + ". Y no se discute."
