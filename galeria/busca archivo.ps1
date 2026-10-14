# Busca un archivo por nombre en tus carpetas y te lo señala en el Explorador.
param($nombre)
if (-not $nombre) { "Dime qué archivo busco."; exit }
$carpetas = @("Desktop", "MyDocuments", "MyPictures", "MyVideos", "MyMusic" | ForEach-Object { [Environment]::GetFolderPath($_) })
$carpetas += (New-Object -ComObject Shell.Application).NameSpace("shell:Downloads").Self.Path
$carpetas = $carpetas | Where-Object { $_ -and (Test-Path -LiteralPath $_) } | Select-Object -Unique
# Las palabritas sueltas (de, la, el) no suelen estar en el nombre del archivo.
$palabras = @("$nombre" -split "\s+" | Where-Object { $_.Length -gt 2 })
if (-not $palabras) { $palabras = @("$nombre".Trim()) }
$hallados = @(Get-ChildItem -LiteralPath $carpetas -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
    $archivo = $_.Name
    -not ($palabras | Where-Object { $archivo -notlike "*$_*" })
} | Sort-Object LastWriteTime -Descending | Select-Object -First 20)
if ($hallados.Count -eq 0) { "No encontré ningún archivo que se llame así."; exit }
Start-Process explorer.exe "/select,`"$($hallados[0].FullName)`""
if ($hallados.Count -eq 1) { "Ahí está: $($hallados[0].Name)." }
else { "Encontré $($hallados.Count) parecidos. Te señalo el más reciente: $($hallados[0].Name)." }
