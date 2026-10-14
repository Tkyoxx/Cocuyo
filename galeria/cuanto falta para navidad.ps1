# Cuenta los días que faltan para Navidad.
$hoy = (Get-Date).Date
$navidad = Get-Date -Year $hoy.Year -Month 12 -Day 25 -Hour 0 -Minute 0 -Second 0
if ($hoy -gt $navidad.Date) { $navidad = $navidad.AddYears(1) }
$faltan = ($navidad.Date - $hoy).Days
if ($faltan -eq 0) { "Es hoy. Feliz Navidad." }
elseif ($faltan -eq 1) { "Falta un día. Ya huele a natilla." }
else { "Faltan $faltan días para Navidad." }
