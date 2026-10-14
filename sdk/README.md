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

## El modo agente

Con **Ajustes → Agente → Modo agente**, Cocuyo decide solo cómo atender cada cosa que le dices:

1. **Lo que entiende sin ayuda** («pon Bad Bunny», «sube el volumen», «recuérdame en 10 minutos…») va al instante, sin gastar consultas.
2. **Lo que claramente tiene pasos** («mándale a Angela por WhatsApp que llego tarde», «abre el bloc de notas y luego…», «haz clic en aceptar») va directo al agente, sin pasar por el intérprete.
3. **Lo dudoso** pasa por el intérprete de siempre, que en la misma consulta decide si es una orden, una respuesta o una tarea con pasos («agente …»). Sus decisiones se recuerdan, así que la misma frase no vuelve a gastar.

El agente **piensa en pasos**: elige herramientas, las usa, mira el resultado y sigue hasta terminar (máximo 12). Si el primer paso es una sola orden de Cocuyo, termina ahí mismo sin otra consulta. Para obligarlo a pensar, empieza con «Encárgate de…».

> «Encárgate de decirme qué hora es en Tokio y ponme un pomodoro de 40 minutos» → usa la herramienta del Reloj mundial, la de Foco, arma una tarjeta y te responde. Probado con Gemini, 14 segundos.

Usa el modelo de la sección IA. Hace falta uno con formato OpenAI y que sepa usar herramientas: Gemini (viene listo), Groq, OpenAI, DeepSeek, Ollama o LM Studio con un modelo que las soporte. Cada paso gasta una consulta.

### Las manos: mouse y teclado

Con **Usar el mouse y el teclado** encendido, el agente maneja cualquier app como lo harías tú. No adivina píxeles: lee la ventana con la accesibilidad de Windows (UI Automation) y recibe cada botón, campo y texto con un número; hace clic sobre esos números, así que da igual la resolución, la escala o en qué monitor esté la ventana. Si una app no deja leer sus botones, usa el texto de la pantalla, y si tampoco alcanza, pide una captura.

> «Mándale a Angela por WhatsApp que llego en 10 minutos» → abre WhatsApp, busca a Angela, entra al chat, comprueba que arriba dice Angela, escribe y te pregunta «¿Lo hago?» antes de pulsar Enter. Unas 3 consultas.

Cuidados que trae de fábrica:

- **Permiso antes de lo irreversible.** Enviar, publicar, comprar o borrar muestra una tarjeta con a dónde va (la ventana y el chat abierto) y espera tu «Hazlo». Se apaga en **Pedir permiso antes de enviar**, pero mejor no.
- **Comprueba el destino.** Antes de enviar mira la cabecera del chat; si no es la persona que dijiste, no envía.
- **No usa números viejos.** Si un clic o una tecla cambia la ventana, vuelve a mirarla antes de seguir; si el botón ya no está, se detiene.
- **Tú mandas.** Si mueves el mouse mientras trabaja, suelta todo y te avisa.
- **Contraseñas no.** No escribe en campos de contraseña ni pulsa Ctrl+Alt+Supr o Win+L.

### Las herramientas que trae

| Herramienta | Qué hace |
|---|---|
| `cocuyo_orden` | Cualquier orden de Cocuyo: música, apps, volumen, recordatorios, clima, notas… |
| `cocuyo_script` | Tus scripts, con su descripción para que el agente sepa cuál usar |
| `cocuyo_tarjeta` | Muestra el resultado en una tarjeta |
| `cocuyo_portapapeles` | Lee lo que copiaste |
| `cocuyo_pantalla` | Lee el texto que se ve en pantalla |
| `cocuyo_recordar` | Guarda algo en la memoria |
| `cocuyo_ventana` | Abre una app (o trae la que ya está abierta) y devuelve sus elementos numerados |
| `cocuyo_mirar` | Vuelve a mirar la ventana; con `imagen` también manda una captura |
| `cocuyo_hacer` | Clics, escribir, teclas, esperar y rueda, en tanda, con `confirmar` y `destino` para lo irreversible |

### Servidores MCP

Cocuyo habla MCP (Model Context Protocol), así que puede usar los mismos servidores que Claude Desktop. Se configuran en `%APPDATA%\io.github.tkyoxx.cocuyo\mcp.json` (botón **Editar mcp.json** en Ajustes → Agente), con el mismo formato:

```json
{
  "mcpServers": {
    "archivos": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "C:\\Users\\yo\\Documents"] },
    "navegador": { "command": "npx", "args": ["-y", "@playwright/mcp@latest"] },
    "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"], "env": { "GITHUB_PERSONAL_ACCESS_TOKEN": "..." } }
  }
}
```

- Con `"disabled": true` un servidor se queda apagado sin borrarlo.
- Los servidores arrancan la primera vez que se usan y se cierran al salir de Cocuyo. Los que fallan se reintentan al minuto.
- En Ajustes → Agente se ve cuáles conectaron y cuántas herramientas trae cada uno.
- Las claves que pongas en `env` quedan en ese archivo, en tu PC. No se mandan a ningún lado, salvo al propio servidor que las usa.

Hay servidores MCP para casi todo: archivos, GitHub, el navegador, bases de datos, Notion, Slack, Home Assistant, Spotify, Obsidian… Un buen sitio para buscarlos es el [registro oficial](https://github.com/modelcontextprotocol/servers).

### Herramientas desde una extensión

Una extensión puede darle herramientas propias al agente:

```js
export default function (cocuyo) {
  cocuyo.herramienta({
    nombre: "hora_en",
    descripcion: "Dice la hora actual en una ciudad.",
    parametros: { type: "object", properties: { ciudad: { type: "string" } }, required: ["ciudad"] },
    hacer: ({ ciudad }) => `En ${ciudad} son las …`,
  });
}
```

`parametros` es un JSON Schema; `hacer` puede ser `async` y lo que devuelva es lo que lee el agente. Así una extensión se vuelve parte del cerebro de Cocuyo: [`foco`](../galeria/extensiones/foco/index.js) y [`reloj-mundial`](../galeria/extensiones/reloj-mundial/index.js) lo hacen.

### Armar tu propio Jarvis

Las piezas se juntan:

- **Scripts** para lo que toca tu PC.
- **Extensiones** para la interfaz y las herramientas en tiempo real.
- **MCP** para el mundo de afuera.
- **Las manos** para las apps que no tienen API.
- **El agente** para encadenarlo todo por voz.

Por ejemplo, «Encárgate de leer el error que tengo en pantalla, buscarlo en GitHub y dejarme la solución en una nota»: `cocuyo_pantalla`, luego el servidor de GitHub y luego `cocuyo_orden` («anota …»). O «pásale a Juan por Discord el enlace que copié»: `cocuyo_portapapeles`, `cocuyo_ventana` y `cocuyo_hacer`.

---

## Publicar

- **Scripts:** desde la isla, **Ajustes → IA → Galería → Subir el mío**. Se abre GitHub con todo escrito; sale a tu nombre cuando se aprueba.
- **Temas y extensiones:** un pull request que agregue tu carpeta en [`galeria/extensiones`](../galeria/extensiones) y una entrada en su `indice.json` (`id`, `nombre`, `descripcion`, `autor`, `tipo`, `archivos` y, si quieres, una `vista` en `.webp` de 16:9). Los nombres de archivo solo pueden llevar letras, números, `-` y `_`.

Lo que no entra: nada que borre cosas, mande datos de nadie a otro sitio o descargue y ejecute otra cosa sin decirlo clarísimo.
