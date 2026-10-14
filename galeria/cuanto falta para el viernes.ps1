# Cuenta los días que faltan para el viernes.
$faltan = (5 - [int](Get-Date).DayOfWeek + 7) % 7
if ($faltan -eq 0) { "Hoy es viernes. Se logró." }
elseif ($faltan -eq 1) { "Falta un día para el viernes. Aguanta que ya casi." }
else { "Faltan $faltan días para el viernes. Paciencia." }
