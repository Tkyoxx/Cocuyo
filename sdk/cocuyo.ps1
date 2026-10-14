# sdk de cocuyo para powershell. cocuyo lo carga solo antes de tu script, no hace falta importarlo

$Cocuyo = [pscustomobject]@{
    Dato         = $env:COCUYO_DATO
    Frase        = $env:COCUYO_FRASE
    Respuesta    = $env:COCUYO_RESPUESTA
    Idioma       = $env:COCUYO_IDIOMA
    Nombre       = $env:COCUYO_NOMBRE
    Ciudad       = $env:COCUYO_CIUDAD
    Carpeta      = $env:COCUYO_CARPETA
    Conversacion = $env:COCUYO_CONVERSACION
    Version      = $env:COCUYO_VERSION
}

function Enviar-Cocuyo($Datos) {
    "@cocuyo " + ($Datos | ConvertTo-Json -Compress -Depth 8)
}

function Decir([string]$Texto) { Enviar-Cocuyo @{ decir = $Texto } }

function Hablar([string]$Texto) { Enviar-Cocuyo @{ decir = $Texto; hablar = $true } }

function Preguntar([string]$Pregunta) { Enviar-Cocuyo @{ preguntar = $Pregunta } }

function Progreso([double]$Valor, [string]$Texto = "") { Enviar-Cocuyo @{ progreso = $Valor; texto = $Texto } }

function Copiar([string]$Texto) { Enviar-Cocuyo @{ copiar = $Texto } }

function Abrir([string]$Destino) { Enviar-Cocuyo @{ abrir = $Destino } }

function Escribir([string]$Texto) { Enviar-Cocuyo @{ escribir = $Texto } }

function Recordar([string]$Dato) { Enviar-Cocuyo @{ recordar = $Dato } }

function Programar([string]$Cuando, [string]$Orden) { Enviar-Cocuyo @{ programar = @{ cuando = $Cuando; orden = $Orden } } }

function Boton {
    param([string]$Texto, [string]$Abrir = "", [string]$Ejecutar = "", [string]$Dato = "", [string]$Copiar = "")
    $boton = @{ texto = $Texto }
    if ($Abrir) { $boton.abrir = $Abrir }
    if ($Ejecutar) { $boton.ejecutar = $Ejecutar }
    if ($Dato) { $boton.dato = $Dato }
    if ($Copiar) { $boton.copiar = $Copiar }
    $boton
}

function Tarjeta {
    param(
        [string]$Titulo,
        [string]$Texto = "",
        [string]$Imagen = "",
        [string]$Icono = "",
        [string]$Color = "",
        [object[]]$Botones = @(),
        [object[]]$Filas = @(),
        [int]$Segundos = 0
    )
    $tarjeta = @{ titulo = $Titulo }
    if ($Texto) { $tarjeta.texto = $Texto }
    if ($Imagen) { $tarjeta.imagen = $Imagen }
    if ($Icono) { $tarjeta.icono = $Icono }
    if ($Color) { $tarjeta.color = $Color }
    if ($Botones.Count) { $tarjeta.botones = @($Botones) }
    if ($Filas.Count) { $tarjeta.filas = @($Filas) }
    if ($Segundos) { $tarjeta.segundos = $Segundos }
    Enviar-Cocuyo @{ tarjeta = $tarjeta }
}

function Fila([string]$Texto, [string]$Valor = "", [double]$Barra = -1) {
    $fila = @{ texto = $Texto; valor = $Valor }
    if ($Barra -ge 0) { $fila.barra = $Barra }
    $fila
}

function Guardar([string]$Clave, $Valor) {
    $archivo = Join-Path $Cocuyo.Carpeta "datos.json"
    $todo = @{}
    if (Test-Path $archivo) { (Get-Content $archivo -Raw -Encoding UTF8 | ConvertFrom-Json).psobject.Properties | ForEach-Object { $todo[$_.Name] = $_.Value } }
    $todo[$Clave] = $Valor
    [IO.File]::WriteAllText($archivo, ($todo | ConvertTo-Json -Depth 8), [Text.UTF8Encoding]::new($false))
}

function Leer([string]$Clave, $PorDefecto = $null) {
    $archivo = Join-Path $Cocuyo.Carpeta "datos.json"
    if (-not (Test-Path $archivo)) { return $PorDefecto }
    $valor = (Get-Content $archivo -Raw -Encoding UTF8 | ConvertFrom-Json).$Clave
    if ($null -eq $valor) { $PorDefecto } else { $valor }
}
