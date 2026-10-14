# Dice la dirección IP de este equipo en tu red.
$ip = (Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -notlike "127.*" -and $_.IPAddress -notlike "169.254.*" } | Select-Object -First 1).IPAddress
if ($ip) { "Tu IP en la red es $ip." } else { "No encontré ninguna red conectada." }
