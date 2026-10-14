# Piensa un número del 1 al 20 y te deja adivinarlo, conversando.
$secreto = Leer "secreto" 0
$intentos = Leer "intentos" 0
if (-not $Cocuyo.Respuesta) {
    Guardar "secreto" (Get-Random -Minimum 1 -Maximum 21)
    Guardar "intentos" 0
    Preguntar "Pensé un número del 1 al 20. ¿Cuál es?"
    exit
}
$dicho = [regex]::Match($Cocuyo.Respuesta, "\d+").Value
$palabras = @{ uno=1; dos=2; tres=3; cuatro=4; cinco=5; seis=6; siete=7; ocho=8; nueve=9; diez=10; once=11; doce=12; trece=13; catorce=14; quince=15; dieciseis=16; diecisiete=17; dieciocho=18; diecinueve=19; veinte=20 }
if (-not $dicho) {
    $limpio = $Cocuyo.Respuesta.ToLower().Normalize([Text.NormalizationForm]::FormD) -replace "\p{Mn}", ""
    foreach ($p in $palabras.Keys) { if ($limpio -match "\b$p\b") { $dicho = $palabras[$p] } }
}
if (-not $dicho) { Preguntar "Eso no es un número. ¿Cuál dices?"; exit }
$intentos = [int]$intentos + 1
Guardar "intentos" $intentos
if ([int]$dicho -eq [int]$secreto) {
    Tarjeta -Titulo "¡Le diste!" -Texto "Era el $secreto. Lo sacaste en $intentos $(if ($intentos -eq 1) { 'intento. Brujería.' } else { 'intentos.' })" -Icono "🎯" -Color "#5ef2c2"
    Hablar "¡Le diste! Era el $secreto."
} elseif ($intentos -ge 5) {
    Decir "Se acabaron los intentos. Era el $secreto. Otra vez será."
} elseif ([int]$dicho -lt [int]$secreto) {
    Preguntar "Más alto. ¿Cuál?"
} else {
    Preguntar "Más bajo. ¿Cuál?"
}
