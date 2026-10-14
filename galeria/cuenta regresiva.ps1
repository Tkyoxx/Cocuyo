# Una cuenta regresiva con barra de progreso. Dile los segundos: «ejecuta cuenta regresiva 30».
param($cuanto)
$total = 10
if ($cuanto -match "\d+") { $total = [Math]::Min(3600, [int]([regex]::Match($cuanto, "\d+").Value)) }
for ($i = 0; $i -le $total; $i++) {
    $falta = $total - $i
    Progreso ($i / $total) ("Faltan {0}:{1:00}" -f [Math]::Floor($falta / 60), ($falta % 60))
    if ($falta -gt 0) { Start-Sleep -Seconds 1 }
}
Tarjeta -Titulo "¡Tiempo!" -Texto "Se acabaron los $total segundos." -Icono "⏰" -Segundos 15
Hablar "¡Tiempo!"
