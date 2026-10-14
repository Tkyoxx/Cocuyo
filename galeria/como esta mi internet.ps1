# Mide el ping, los paquetes perdidos y qué tan estable está la conexión. Para saber si el lag es tuyo.
$ping = New-Object Net.NetworkInformation.Ping
$tiempos = @(); $perdidos = 0
1..10 | ForEach-Object {
    try {
        $r = $ping.Send("1.1.1.1", 1000)
        if ($r.Status -eq "Success") { $tiempos += $r.RoundtripTime } else { $perdidos++ }
    } catch { $perdidos++ }
    Start-Sleep -Milliseconds 120
}
if (-not $tiempos) { "No hay internet, o algo está bloqueando el ping."; exit }
$medio = [math]::Round(($tiempos | Measure-Object -Average).Average)
$salto = ($tiempos | Measure-Object -Maximum).Maximum - ($tiempos | Measure-Object -Minimum).Minimum
$juicio = if ($perdidos -gt 1 -or $medio -gt 150) { "Está fea la cosa: el lag es tuyo." }
elseif ($medio -gt 70 -or $salto -gt 40) { "Regular: sirve, pero para jugar vas a sufrir." }
else { "Está fina. Si hay lag, no es por ti." }
"Ping de $medio ms, $perdidos de 10 paquetes perdidos y $salto ms de variación. $juicio"
