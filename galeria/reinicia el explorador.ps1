# Reinicia el Explorador de Windows: para cuando la barra de tareas se pega o los íconos desaparecen.
Stop-Process -Name explorer -Force -ErrorAction SilentlyContinue
Start-Sleep -Seconds 2
if (-not (Get-Process -Name explorer -ErrorAction SilentlyContinue)) { Start-Process explorer.exe }
"Explorador reiniciado. La barra vuelve en un momento."
