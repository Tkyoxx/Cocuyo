# Cambia Windows entre modo oscuro y modo claro.
$clave = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"
$claro = (Get-ItemProperty $clave -ErrorAction SilentlyContinue).AppsUseLightTheme
$nuevo = if ($claro -eq 0) { 1 } else { 0 }
Set-ItemProperty $clave AppsUseLightTheme $nuevo -Type DWord
Set-ItemProperty $clave SystemUsesLightTheme $nuevo -Type DWord
if ($nuevo -eq 0) { "Modo oscuro. Tus ojos te lo agradecen." } else { "Modo claro. Ojo que encandila." }
