# Dice cuánto espacio libre queda en cada disco.
$discos = Get-CimInstance Win32_LogicalDisk -Filter "DriveType=3" | ForEach-Object {
    "{0} {1} GB libres de {2}" -f $_.DeviceID, [math]::Round($_.FreeSpace / 1GB), [math]::Round($_.Size / 1GB)
}
if ($discos) { ($discos -join "; ") + "." } else { "No encontré ningún disco, y eso ya es raro." }
