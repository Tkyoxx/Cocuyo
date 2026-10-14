# Revisa qué le sobra al PC: memoria, temporales y papelera, y te deja limpiar con un botón.
$so = Get-CimInstance Win32_OperatingSystem
$usada = ($so.TotalVisibleMemorySize - $so.FreePhysicalMemory) / $so.TotalVisibleMemorySize
$top = Get-Process | Group-Object ProcessName | ForEach-Object { [pscustomobject]@{ Nombre = $_.Name; Bytes = ($_.Group | Measure-Object WorkingSet64 -Sum).Sum } } | Sort-Object Bytes -Descending | Select-Object -First 3
$temporales = (Get-ChildItem -LiteralPath $env:TEMP -Recurse -File -Force -ErrorAction SilentlyContinue | Measure-Object Length -Sum).Sum
$papelera = 0
try { $shell = New-Object -ComObject Shell.Application; $papelera = ($shell.NameSpace(10).Items() | ForEach-Object { $_.Size } | Measure-Object -Sum).Sum } catch {}
$mas = [double]($top | Select-Object -First 1).Bytes
$filas = @(Fila "Memoria en uso" ("{0:P0}" -f $usada) $usada)
$filas += $top | ForEach-Object { Fila $_.Nombre ("{0:N1} GB" -f ($_.Bytes / 1GB)) ($_.Bytes / $mas) }
$filas += Fila "Temporales" ("{0:N0} MB" -f ($temporales / 1MB))
$filas += Fila "Papelera" ("{0:N0} MB" -f ($papelera / 1MB))
$botones = @((Boton -Texto "Limpiar temporales" -Ejecutar "limpia temporales"), (Boton -Texto "Administrador" -Abrir "taskmgr.exe"))
$veredicto = if ($usada -gt 0.85) { "Tu PC está a punto de pedir auxilio." } elseif ($usada -gt 0.65) { "Va cargadito, pero aguanta." } else { "Está en forma." }
Tarjeta -Titulo "Tu PC a dieta" -Texto $veredicto -Icono "🥗" -Color "#5fd38d" -Filas $filas -Botones $botones -Segundos 25
Decir ("{0} Lo que más come es {1}." -f $veredicto, ($top | Select-Object -First 1).Nombre)
