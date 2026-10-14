# Cierra lo que distrae: Discord, WhatsApp, Telegram, Steam y compañía. La lista está arriba del script, cámbiala a tu gusto.
$distracciones = "Discord", "WhatsApp", "Telegram", "Steam", "EpicGamesLauncher", "Messenger", "Slack"
$cerrados = @()
foreach ($nombre in $distracciones) {
    if (Get-Process -Name $nombre -ErrorAction SilentlyContinue) {
        Stop-Process -Name $nombre -Force -ErrorAction SilentlyContinue
        $cerrados += $nombre
    }
}
if ($cerrados) { "Cerré " + ($cerrados -join ", ") + ". A trabajar, que el día no se hace solo." }
else { "No había nada que te distrajera. Ya no tienes excusa." }
