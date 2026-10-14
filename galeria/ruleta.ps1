# Gira una ruleta entre lo que le digas: «ejecuta ruleta pizza, sushi o tacos».
param($opciones)
$lista = @("$opciones" -split "\s+o\s+|\s*,\s*|\s+y\s+" | ForEach-Object { $_.Trim() } | Where-Object { $_ })
if ($lista.Count -lt 2) { Preguntar "¿Entre qué cosas giro la ruleta?"; exit }
if ($Cocuyo.Respuesta) { $lista = @($Cocuyo.Respuesta -split "\s+o\s+|\s*,\s*|\s+y\s+" | Where-Object { $_.Trim() }) }
$ganador = $lista | Get-Random
$ganador = $ganador.Substring(0, 1).ToUpper() + $ganador.Substring(1)
$vueltas = 18
for ($i = 1; $i -le $vueltas; $i++) {
    Progreso ($i / $vueltas) ("🎡 " + $lista[$i % $lista.Count])
    Start-Sleep -Milliseconds (40 + $i * $i * 2)
}
Tarjeta -Titulo $ganador -Texto ("Entre " + ($lista -join ", ") + ". La ruleta habló.") -Icono "🎉" -Color "#f2b84b" -Segundos 12 -Botones @((Boton -Texto "Otra vez" -Ejecutar "ruleta" -Dato ($lista -join ", ")))
Hablar "¡$ganador! Y no se discute."
