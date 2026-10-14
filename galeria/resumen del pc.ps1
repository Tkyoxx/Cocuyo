# Resume tu PC en una frase: procesador, gráfica, memoria, disco y Windows.
$cpu = ((Get-CimInstance Win32_Processor | Select-Object -First 1).Name -replace "\(R\)|\(TM\)|CPU|@.*", "" -replace "\s+", " ").Trim()
$gpu = (Get-CimInstance Win32_VideoController | Sort-Object AdapterRAM -Descending | Select-Object -First 1).Name
$so = Get-CimInstance Win32_OperatingSystem
$ram = [math]::Round($so.TotalVisibleMemorySize / 1MB)
$libre = [math]::Round($so.FreePhysicalMemory / 1MB, 1)
$disco = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='$($env:SystemDrive)'"
"{0}, {1}, {2} GB de RAM con {3} libres, {4} GB libres en {5} y {6}." -f $cpu, $gpu, $ram, $libre, [math]::Round($disco.FreeSpace / 1GB), $env:SystemDrive, ($so.Caption -replace "Microsoft ", "")
