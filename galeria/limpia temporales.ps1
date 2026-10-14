# Borra los archivos temporales de más de un día y dice cuánto espacio liberó.
$temp = $env:TEMP
# Solo la carpeta temporal de verdad: si la variable viene rara, no se toca nada.
if (-not $temp -or -not (Test-Path -LiteralPath $temp -PathType Container) -or $temp.TrimEnd("\").Length -lt 12 -or $temp.TrimEnd("\") -eq $env:USERPROFILE) {
    "No encontré la carpeta temporal. No toqué nada."; exit
}
$limite = (Get-Date).AddDays(-1)
$liberado = 0; $borrados = 0
Get-ChildItem -LiteralPath $temp -Recurse -File -Force -ErrorAction SilentlyContinue | Where-Object { $_.LastWriteTime -lt $limite } | ForEach-Object {
    $peso = $_.Length
    # Lo que esté en uso no se deja borrar, y está bien así.
    try { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction Stop; $liberado += $peso; $borrados++ } catch {}
}
Get-ChildItem -LiteralPath $temp -Recurse -Directory -Force -ErrorAction SilentlyContinue | Sort-Object { $_.FullName.Length } -Descending | ForEach-Object {
    if (-not (Get-ChildItem -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue)) { Remove-Item -LiteralPath $_.FullName -Force -ErrorAction SilentlyContinue }
}
if ($borrados -eq 0) { "No había basura vieja que sacar." }
elseif ($liberado -ge 1GB) { "Saqué $borrados archivos: {0:N1} GB libres de más." -f ($liberado / 1GB) }
else { "Saqué $borrados archivos: {0:N0} MB libres de más." -f ($liberado / 1MB) }
