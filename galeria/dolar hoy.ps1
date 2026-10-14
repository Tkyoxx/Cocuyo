# Dice a cómo está el dólar en la moneda de tu país.
[Net.ServicePointManager]::SecurityProtocol = "Tls12"
$moneda = [Globalization.RegionInfo]::CurrentRegion.ISOCurrencySymbol
if ($moneda -eq "USD") { "Tu moneda ya es el dólar. Está a un dólar, garantizado."; exit }
try {
    $hoy = Invoke-RestMethod "https://open.er-api.com/v6/latest/USD" -TimeoutSec 8
    $valor = $hoy.rates.$moneda
    if ($valor) { "El dólar está a {0:N2} {1}." -f $valor, $moneda } else { "No encontré el cambio a $moneda." }
} catch { "No pude mirar el dólar. Revisa el internet." }
