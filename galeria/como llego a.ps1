# Abre Google Maps con la ruta al sitio que le digas.
param($sitio)
if (-not $sitio) { "Dime a dónde vamos."; exit }
Start-Process ("https://www.google.com/maps/dir/?api=1&destination=" + [uri]::EscapeDataString($sitio))
"Ahí te va la ruta a $sitio."
