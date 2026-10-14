# Mira si se cayó Discord, GitHub, Epic Games y compañía. Dile cuál, o mira todos.
param($cual)
[Net.ServicePointManager]::SecurityProtocol = "Tls12"
$servicios = [ordered]@{
    "Discord" = "https://discordstatus.com"
    "GitHub" = "https://www.githubstatus.com"
    "Epic Games" = "https://status.epicgames.com"
    "Reddit" = "https://www.redditstatus.com"
    "Cloudflare" = "https://www.cloudflarestatus.com"
    "OpenAI" = "https://status.openai.com"
    "Claude" = "https://status.anthropic.com"
}
$elegidos = $servicios.Keys
if ($cual) {
    $limpio = $cual.ToLower()
    $uno = $servicios.Keys | Where-Object { $limpio -match [regex]::Escape($_.ToLower().Split(" ")[0]) } | Select-Object -First 1
    if ($uno) { $elegidos = @($uno) }
}
$filas = @(); $caidos = @()
foreach ($nombre in $elegidos) {
    try {
        $estado = Invoke-RestMethod "$($servicios[$nombre])/api/v2/status.json" -TimeoutSec 6
        $nivel = $estado.status.indicator
        $texto = switch ($nivel) { "none" { "✅ Todo bien" } "minor" { "⚠️ Con fallas" } "major" { "🔥 Caído en parte" } "critical" { "💀 Caído" } default { "❓ $nivel" } }
        if ($nivel -ne "none") { $caidos += $nombre }
    } catch { $texto = "❓ No respondió" }
    $filas += Fila $nombre $texto
}
$titulo = if ($caidos) { "Hay problemas en " + ($caidos -join ", ") } else { "Todo funcionando" }
$botones = @()
if (@($elegidos).Count -eq 1) { $botones += Boton -Texto "Ver estado" -Abrir $servicios[@($elegidos)[0]] }
Tarjeta -Titulo $titulo -Icono $(if ($caidos) { "🚨" } else { "🟢" }) -Color $(if ($caidos) { "#ff6b5e" } else { "#5fd38d" }) -Filas $filas -Botones $botones -Segundos 15
if ($caidos) { Decir ("Sí, hay líos en " + ($caidos -join " y ") + ". No es tu internet.") }
elseif (@($elegidos).Count -eq 1) { Decir "$(@($elegidos)[0]) está funcionando. El problema es otro." }
else { Decir "Todo está funcionando. Si algo falla, es de tu lado." }
