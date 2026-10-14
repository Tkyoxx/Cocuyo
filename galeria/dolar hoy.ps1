# Dice a cómo está el dólar en la moneda de tu país, con su tarjeta.
[Net.ServicePointManager]::SecurityProtocol = "Tls12"
$moneda = [Globalization.RegionInfo]::CurrentRegion.ISOCurrencySymbol
if ($moneda -eq "USD") { "Tu moneda ya es el dólar. Está a un dólar, garantizado."; exit }
try {
    $hoy = Invoke-RestMethod "https://open.er-api.com/v6/latest/USD" -TimeoutSec 8
    $valor = $hoy.rates.$moneda
    if (-not $valor) { "No encontré el cambio a $moneda."; exit }
    $euro = $valor / $hoy.rates.EUR
    Tarjeta -Titulo ("1 USD = {0:N2} {1}" -f $valor, $moneda) -Texto "Cambio de referencia de hoy. El de la casa de cambio siempre es peor, ya sabes." -Icono "💵" -Color "#5fd38d" -Filas @(
        (Fila "Dólar" ("{0:N2}" -f $valor)),
        (Fila "Euro" ("{0:N2}" -f $euro)),
        (Fila "100 dólares" ("{0:N0}" -f ($valor * 100)))
    ) -Botones @(
        (Boton -Texto "Copiar" -Copiar ("{0:N2}" -f $valor)),
        (Boton -Texto "Ver gráfica" -Abrir "https://www.google.com/finance/quote/USD-$moneda")
    )
    Decir ("El dólar está a {0:N2} {1}." -f $valor, $moneda)
} catch { "No pude mirar el dólar. Revisa el internet." }
