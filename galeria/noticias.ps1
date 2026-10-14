# Los titulares de hoy en BBC Mundo, en una tarjeta.
[Net.ServicePointManager]::SecurityProtocol = "Tls12"
try {
    [xml]$rss = (Invoke-WebRequest "https://feeds.bbci.co.uk/mundo/rss.xml" -UseBasicParsing -TimeoutSec 8).Content
    $items = @($rss.rss.channel.item | Select-Object -First 5)
    if (-not $items) { "No llegaron noticias."; exit }
    $titulo = { param($item) $item.SelectSingleNode("title").InnerText.Trim() }
    $filas = $items | ForEach-Object { Fila (& $titulo $_) "" }
    Tarjeta -Titulo "Titulares de hoy" -Texto "BBC Mundo" -Icono "📰" -Filas $filas -Segundos 30 -Botones @((Boton -Texto "Abrir la primera" -Abrir $items[0].SelectSingleNode("link").InnerText.Trim()), (Boton -Texto "BBC Mundo" -Abrir "https://www.bbc.com/mundo"))
    Decir ("La primera: " + (& $titulo $items[0]))
} catch { "No pude traer las noticias. Revisa el internet." }
