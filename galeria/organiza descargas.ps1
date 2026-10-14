# Ordena tu carpeta de Descargas en carpetas por tipo: imágenes, videos, documentos, instaladores y demás.
$descargas = (New-Object -ComObject Shell.Application).NameSpace("shell:Downloads").Self.Path
$tipos = [ordered]@{
    "Imágenes"     = "jpg", "jpeg", "png", "gif", "webp", "bmp", "svg", "heic", "ico"
    "Videos"       = "mp4", "mkv", "mov", "avi", "webm", "wmv", "flv"
    "Música"       = "mp3", "wav", "flac", "ogg", "m4a", "aac"
    "Documentos"   = "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv", "odt", "epub"
    "Comprimidos"  = "zip", "rar", "7z", "tar", "gz", "bz2"
    "Instaladores" = "exe", "msi", "msix", "appx", "iso"
}
$movidos = 0
foreach ($archivo in Get-ChildItem -LiteralPath $descargas -File) {
    $ext = $archivo.Extension.TrimStart(".").ToLower()
    # Lo que se bajó hace nada puede seguir a medias: se deja quieto.
    if ($archivo.LastWriteTime -gt (Get-Date).AddMinutes(-2)) { continue }
    $carpeta = ($tipos.GetEnumerator() | Where-Object { $_.Value -contains $ext } | Select-Object -First 1).Key
    if (-not $carpeta) { continue }
    $destino = Join-Path $descargas $carpeta
    New-Item -ItemType Directory -Path $destino -Force | Out-Null
    $final = Join-Path $destino $archivo.Name
    $n = 1
    while (Test-Path -LiteralPath $final) {
        $final = Join-Path $destino ("{0} ({1}){2}" -f $archivo.BaseName, $n, $archivo.Extension)
        $n++
    }
    try { Move-Item -LiteralPath $archivo.FullName -Destination $final -ErrorAction Stop; $movidos++ } catch {}
}
if ($movidos -eq 0) { "Tus descargas ya estaban ordenadas. Milagro." }
elseif ($movidos -eq 1) { "Listo: un archivo a su carpeta." }
else { "Listo: $movidos archivos, cada uno en su carpeta." }
