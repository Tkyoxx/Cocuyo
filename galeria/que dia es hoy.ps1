# Dice la fecha de hoy.
"Hoy es " + (Get-Date).ToString("dddd d 'de' MMMM", [Globalization.CultureInfo]"es-ES") + "."
