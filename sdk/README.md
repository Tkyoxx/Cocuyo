# SDK de Cocuyo

Cocuyo se deja modificar de punta a punta, como Spicetify con Spotify. Hay tres formas de meterle mano:

| | Qué es | Para qué | Dónde va |
|---|---|---|---|
| **Script** | Un archivo `.ps1`, `.py`, `.js`, `.bat`, `.cmd` o `.sh` | Órdenes de voz nuevas que hacen cosas en tu PC y responden con tarjetas, progreso o preguntas | `%APPDATA%\io.github.tkyoxx.cocuyo\scripts` |
| **Tema** | Un `estilo.css` | Cambiar cómo se ve la isla: colores, bordes, letras, animaciones | `%APPDATA%\io.github.tkyoxx.cocuyo\extensiones\<id>` |
| **Extensión** | Un `index.js` | Pestañas nuevas en el menú, efectos alrededor de la isla, widgets en el escritorio, órdenes de voz propias | `%APPDATA%\io.github.tkyoxx.cocuyo\extensiones\<id>` |

Las dos carpetas se abren desde la isla: **Ajustes → IA → Abrir carpeta** y **Ajustes → Extensiones → Abrir carpeta**.

> Un script o una extensión puede hacer lo mismo que tú en tu PC. Instala solo lo que entiendas. Si una extensión te rompe la isla, el icono de Cocuyo junto al reloj tiene **Desactivar extensiones**.

---

## Scripts

### Lo básico

```powershell
# Dice qué hora es.
"Son las " + (Get-Date -Format "h:mm") + "."
```

- El nombre del archivo es la orden: `di la hora.ps1` se ejecuta con «ejecuta di la hora». Con un modelo de IA conectado basta algo parecido a la descripción.
- La primera línea de comentario es la descripción. Se ve en la galería y la usa la IA para encontrarlo.
- Lo último que el script escribe es lo que Cocuyo responde.
- Lo que dices después del nombre llega como primer argumento y en `COCUYO_DATO`: «ejecuta elige pizza o sushi» → `pizza o sushi`.

### Lo que el script recibe

| Variable | Qué trae |
|---|---|
| `COCUYO_DATO` | Lo que dijiste después del nombre |
| `COCUYO_FRASE` | La frase entera |
| `COCUYO_RESPUESTA` | Lo que contestaste, si el script te había hecho una pregunta (ver `preguntar`) |
| `COCUYO_IDIOMA` | `es`, `en` o `pt` |
| `COCUYO_NOMBRE` / `COCUYO_CIUDAD` | Lo que pusiste en Ajustes → Tú |
| `COCUYO_CARPETA` | Una carpeta solo para este script, para guardar lo que quieras entre una vez y otra |
| `COCUYO_CONVERSACION` | Lo último que hablaron, por si tu script quiere contexto |
| `COCUYO_VERSION` | La versión de Cocuyo |
| `COCUYO_SDK` | `2` |

### Responder con algo más que texto

Cualquier línea que empiece por `@cocuyo ` seguida de un JSON es un mensaje para Cocuyo. Se pueden mandar varios, en cualquier momento mientras el script corre, y se pueden combinar claves en un mismo mensaje.

| Clave | Qué hace |
|---|---|
| `decir` | El texto de la respuesta (subtítulo). Con `"hablar": true` también lo dice en voz alta |
| `tarjeta` | Muestra una tarjeta bajo la isla (ver abajo) |
| `progreso` | Una barra de progreso de `0` a `1`, o `null` si no sabes cuánto falta. `texto` va debajo |
| `preguntar` | Cocuyo te hace la pregunta y lo próximo que digas vuelve a ejecutar **este** script con `COCUYO_RESPUESTA` |
| `copiar` | Deja un texto en el portapapeles |
| `abrir` | Abre una web, un archivo o una carpeta |
| `escribir` | Escribe un texto donde tengas el cursor |
| `recordar` | Guarda un dato en la memoria de Cocuyo |
| `programar` | `{"cuando": "en 10 minutos", "orden": "pon música"}` programa una orden |

**La tarjeta:**

| Campo | |
|---|---|
| `titulo` | Obligatorio |
| `texto` | Un párrafo |
| `icono` | Un emoji |
| `imagen` | Una ruta local (`png`, `jpg`, `gif`, `webp`, `svg`), una URL `https://` o un `data:image/...` |
| `color` | `#rrggbb`; tiñe el borde, las barras y el primer botón |
| `filas` | Hasta 8: `{"texto": "Euro", "valor": "4.300", "barra": 0.4}` (la barra es opcional, de 0 a 1) |
| `botones` | Hasta 4: `{"texto": "Ver", "abrir": "https://…"}`, `{"texto": "Copiar", "copiar": "…"}`, `{"texto": "Otra", "ejecutar": "nombre de un script", "dato": "…"}` |
| `segundos` | Cuánto se queda (de 2 a 120; 10 si no dices nada). Si pasas el mouse por encima, espera |

Si un script tarda más de 20 segundos, Cocuyo dice «sigue trabajando» y lo deja correr: lo que mande después (tarjetas, progreso, la respuesta final) igual aparece.

### Con ayuda en cada lenguaje

Cocuyo trae una librería para PowerShell, Python y Node que escribe esos mensajes por ti. Viene sola, sin instalar nada.

**PowerShell** (las funciones ya están cargadas):

```powershell
# Dice a cómo está el dólar, con su tarjeta.
$valor = (Invoke-RestMethod "https://open.er-api.com/v6/latest/USD").rates.COP
Tarjeta -Titulo ("1 USD = {0:N0} COP" -f $valor) -Icono "💵" -Color "#5fd38d" `
    -Filas @((Fila "Dólar" ("{0:N0}" -f $valor))) `
    -Botones @((Boton -Texto "Copiar" -Copiar ("{0:N0}" -f $valor)))
Decir ("El dólar está a {0:N0} pesos." -f $valor)
```

Funciones: `Decir`, `Hablar`, `Tarjeta`, `Fila`, `Boton`, `Progreso`, `Preguntar`, `Copiar`, `Abrir`, `Escribir`, `Recordar`, `Programar`, `Guardar`, `Leer`, y el objeto `$Cocuyo` (`Dato`, `Frase`, `Respuesta`, `Nombre`, `Carpeta`…).

**Python:**

```python
# Cuenta hasta diez con barra de progreso.
import time
import cocuyo

for i in range(11):
    cocuyo.progreso(i / 10, f"Van {i}")
    time.sleep(0.5)
cocuyo.tarjeta("¡Listo!", "Conté hasta diez sin saltarme ninguno.", icono="🔟")
cocuyo.hablar("Listo")
```

**Node:**

```js
// Saluda por el nombre.
const cocuyo = require("cocuyo");
cocuyo.tarjeta({ titulo: `Hola, ${cocuyo.nombre || "tú"}`, icono: "👋", texto: "Esto lo mandó un script de Node." });
cocuyo.decir("Hola");
```

Las tres tienen lo mismo: `decir`, `hablar`, `tarjeta`, `fila`, `boton`, `progreso`, `preguntar`, `copiar`, `abrir`, `escribir`, `recordar`, `programar`, `guardar`, `leer`.

### Conversar: `preguntar`

```powershell
# Piensa un número y te deja adivinarlo.
if (-not $Cocuyo.Respuesta) {
    Guardar "secreto" (Get-Random -Minimum 1 -Maximum 21)
    Preguntar "Pensé un número del 1 al 20. ¿Cuál es?"
    exit
}
$secreto = Leer "secreto"
if ($Cocuyo.Respuesta -match "\b$secreto\b") { Hablar "¡Le diste!" } else { Preguntar "No. ¿Otro?" }
```

La pregunta queda abierta dos minutos y medio. Lo que digas en ese rato va directo a tu script. El estado entre una vuelta y otra se guarda en `COCUYO_CARPETA` (`Guardar`/`Leer` lo hacen por ti).

---

## Temas

Una carpeta con dos archivos:

```
extensiones/
  mi-tema/
    manifest.json
    estilo.css
```

```json
{
  "nombre": "Mi tema",
  "descripcion": "Qué cambia, en una línea.",
  "autor": "tu-usuario-de-github",
  "version": "1.0",
  "tipo": "tema",
  "estilo": "estilo.css"
}
```

El CSS se carga después del de Cocuyo, así que gana. Lo más fácil es cambiar las variables:

| Variable | Qué pinta |
|---|---|
| `--black` | El cuerpo de la isla (y sus orejitas) |
| `--accent` / `--accent-ink` | El color de acento y el texto encima de él |
| `--ink` / `--dim` | Texto principal y secundario |
| `--card` / `--tile` | Fondos de tarjetas y botones del menú |
| `--font` | La letra |

Selectores útiles: `#island` (la isla; `data-face` dice si está en reposo, escuchando o con el menú), `#island .shell` (su cuerpo), `.tab` (las pestañas), `.page[data-page="hoy"]` (cada pestaña), `#caption .said` (los subtítulos), `#pill` (la píldora de voz), `.sc` (las tarjetas). La variable `--lv` (de 0 a 1) sigue el volumen de tu voz mientras hablas, para animar lo que quieras con CSS.

Alrededor de la isla hay 18 px donde puedes dibujar sin tapar nada. Mira [`aurora`](../galeria/extensiones/aurora/estilo.css), [`medianoche`](../galeria/extensiones/medianoche/estilo.css) y [`neon`](../galeria/extensiones/neon/estilo.css).

---

## Extensiones

Igual que un tema, pero con `"tipo": "extension"` y `"codigo": "index.js"` (puede llevar `estilo` también). El `index.js` es un módulo que exporta una función; Cocuyo la llama con la API:

```js
export default function (cocuyo) {
  cocuyo.orden(["hola cocuyo", "saluda"], () => cocuyo.decir("¡Quiubo!"));
}
```

### La API

| | |
|---|---|
| `cocuyo.decir(texto, ms?)` | Un subtítulo |
| `cocuyo.hablar(texto)` | Lo dice en voz alta, con la voz que elegiste |
| `cocuyo.tarjeta({...})` | Una tarjeta, igual que las de los scripts. Los botones pueden llevar `accion: () => {}` |
| `cocuyo.progreso(id, valor, texto)` | Una barra de progreso |
| `cocuyo.pestana({ id, titulo, ancho, alto, html, montar(el), mostrar(el), ocultar() })` | Una pestaña nueva en el menú, antes de Ajustes |
| `cocuyo.orden(frases, fn)` | Órdenes de voz propias. Si lo que dices empieza por una de las frases, la llama y Cocuyo no hace nada más |
| `cocuyo.al(evento, fn)` | Escuchar lo que pasa (ver abajo). Devuelve una función para dejar de escuchar |
| `cocuyo.flotante(el)` | Pone un elemento en el escritorio, fuera de la isla, y lo hace clicable. Devuelve una función para quitarlo |
| `cocuyo.isla` | El elemento de la isla, para pegarle cosas encima |
| `cocuyo.css(texto)` | Inyecta estilos |
| `cocuyo.ejecutar(script, dato?)` | Ejecuta uno de tus scripts y devuelve su respuesta |
| `cocuyo.invocar(comando, args?)` | Llama a cualquier comando del núcleo de Cocuyo (los que usa la propia interfaz) |
| `cocuyo.ajustes()` | Los ajustes actuales |
| `cocuyo.leer(clave, porDefecto)` / `cocuyo.guardar(clave, valor)` | Guardar cosas de tu extensión |
| `cocuyo.abrirMenu(pestaña?)` / `cocuyo.cerrarMenu()` | Abrir y cerrar el menú |

**Eventos para `cocuyo.al`:**

| Evento | Cuándo | Trae |
|---|---|---|
| `orden` | Empiezas o terminas de hablarle | `{ chord, phase: "start" \| "end", cancelled }` |
| `nivel` | Mientras hablas, unas 30 veces por segundo | El volumen de tu voz |
| `oido` | Entendió lo que dijiste | `{ chord, text, reply, error }` |
| `nota` | Cocuyo muestra un aviso | El texto |
| `tarjeta` / `progreso` | Un script mandó una tarjeta o progreso | Lo que mandó |
| `menu` | Se abre o se cierra el menú | `true` / `false` |
| `lugar` | La isla cambia de sitio | `"top"`, `"right"` o `"hidden"` |
| `listo` | Cargaron todas las extensiones | |

Ejemplos completos: [`foco`](../galeria/extensiones/foco/index.js) (una pestaña Pomodoro con órdenes de voz) y [`cocuyos`](../galeria/extensiones/cocuyos/index.js) (luciérnagas que siguen tu voz). Los tipos están en [`cocuyo.d.ts`](cocuyo.d.ts): pon `/** @param {import("./cocuyo").Cocuyo} cocuyo */` encima de tu función y el editor te autocompleta todo.

---

## Publicar

- **Scripts:** desde la isla, **Ajustes → IA → Galería → Subir el mío**. Se abre GitHub con todo escrito; sale a tu nombre cuando se aprueba.
- **Temas y extensiones:** un pull request que agregue tu carpeta en [`galeria/extensiones`](../galeria/extensiones) y una entrada en su `indice.json` (`id`, `nombre`, `descripcion`, `autor`, `tipo`, `archivos` y, si quieres, una `vista` en `.webp` de 16:9). Los nombres de archivo solo pueden llevar letras, números, `-` y `_`.

Lo que no entra: nada que borre cosas, mande datos de nadie a otro sitio o descargue y ejecute otra cosa sin decirlo clarísimo.
