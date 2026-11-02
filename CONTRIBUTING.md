# Ayudar con Cocuyo

Gracias por pasarte. Antes de mandar un cambio, lee las reglas: son pocas pero no se negocian.

## Las reglas

1. **Que corra hasta en un PC viejo.** Nada se queda trabajando en reposo: ni temporizadores, ni animaciones eternas, ni consultas cada tantos segundos. Si algo tiene que mirar, mira solo mientras se ve.
2. **Todo propio.** Antes de meter un crate, un paquete de npm o un servicio, pregúntate si se puede hacer con lo que ya hay. Si hace falta, di cuánto pesa (binario, memoria, procesador).
3. **Nada sale del PC sin decirlo.** Si algo nuevo manda datos afuera, va en «Qué sale de tu PC» en Ajustes → IA.
4. **Nada se envía, se paga ni se borra sin un clic.** El agente pide permiso y una respuesta solo vale para la pregunta que la pidió.
5. **Nunca se reescribe un archivo ajeno.** Si Cocuyo tiene que tocar un archivo que no es suyo, hace copia con fecha, muestra el cambio y no escribe si el archivo cambió mientras tanto.
6. **El aspecto no se cambia de pasada.** La isla se ve como se ve; si quieres cambiarle la cara, abre primero un issue.

## Compilar

```
npm install
npm run tauri dev
```

Rust va en `src-tauri/`, la isla en TypeScript sin frameworks en `src/`.

## Probar

```
cd src-tauri && cargo test
npm test
npx tsc --noEmit
```

Lo que se ve en pantalla se prueba también en la app de verdad: el panel del navegador no corre las animaciones.

## Textos

Todo lo que se ve o se dice va en español y con su traducción al inglés y al portugués en `src/i18n.ts` (la isla) y `src-tauri/src/i18n.rs` (lo que responde).

## Versiones

Cada versión nueva lleva su sección en `CHANGELOG.md` y lo que trae en `src/novedades.ts`. Sin las dos, la compilación de la versión falla.
