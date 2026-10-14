# Busca algo en Wikipedia y te lo resume con foto: «ejecuta wikipedia agujero negro».
param($tema)
if (-not $tema) { Preguntar "¿Qué busco en Wikipedia?"; exit }
if ($Cocuyo.Respuesta) { $tema = $Cocuyo.Respuesta }
[Net.ServicePointManager]::SecurityProtocol = "Tls12"
$idioma = if ($Cocuyo.Idioma) { $Cocuyo.Idioma } else { "es" }
try {
    $busqueda = Invoke-RestMethod ("https://$idioma.wikipedia.org/w/api.php?action=opensearch&limit=1&format=json&search=" + [uri]::EscapeDataString($tema)) -TimeoutSec 8 -Headers @{ "User-Agent" = "Cocuyo/1.0" }
    $titulo = $busqueda[1][0]
    if (-not $titulo) { "No encontré nada sobre $tema en Wikipedia."; exit }
    $resumen = Invoke-RestMethod ("https://$idioma.wikipedia.org/api/rest_v1/page/summary/" + [uri]::EscapeDataString($titulo)) -TimeoutSec 8 -Headers @{ "User-Agent" = "Cocuyo/1.0" }
    $texto = $resumen.extract
    if ($texto.Length -gt 420) { $texto = $texto.Substring(0, $texto.LastIndexOf(" ", 420)) + "…" }
    $imagen = if ($resumen.thumbnail.source -like "https://*") { $resumen.thumbnail.source } else { "" }
    Tarjeta -Titulo $resumen.title -Texto $texto -Imagen $imagen -Icono "📚" -Segundos 30 -Botones @((Boton -Texto "Leer más" -Abrir $resumen.content_urls.desktop.page), (Boton -Texto "Copiar" -Copiar $resumen.extract))
    $primera = ($resumen.extract -split "(?<=\.)\s")[0]
    if ($primera.Length -gt 200) { $primera = $primera.Substring(0, $primera.LastIndexOf(" ", 200)) + "…" }
    Hablar $primera
} catch { "Wikipedia no respondió. Revisa el internet." }
