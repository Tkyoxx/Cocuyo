# Dice hace cuánto está prendido el PC.
$t = (Get-Date) - (Get-CimInstance Win32_OperatingSystem).LastBootUpTime
if ($t.Days -ge 2) { "Lleva $($t.Days) días y $($t.Hours) horas prendido. Ya dale un descanso al pobre." }
elseif ($t.Days -eq 1) { "Lleva un día y $($t.Hours) horas prendido." }
elseif ($t.Hours -ge 1) { "Lleva $($t.Hours) horas y $($t.Minutes) minutos prendido." }
else { "Lleva $($t.Minutes) minutos prendido. Recién levantado." }
