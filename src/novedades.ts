// lo que trae cada version, para contarlo una vez despues de actualizar. la compilacion falla si falta la de la version

export const NOVEDADES: Record<string, [string, string][]> = {
  "0.4.0": [
    ["Pregúntale por cualquier archivo", "Suéltalo y toca «¿De qué trata?»: PDF, Word, Excel, PowerPoint, imágenes o texto, y sigues preguntando ahí mismo."],
    ["WhatsApp sin abrir WhatsApp", "Te avisa de los mensajes, te los lee y le respondes con la voz. Se conecta en Ajustes → Agente."],
    ["La voz, tres veces más rápida", "El modelo carga mientras hablas: una frase corta sale en unos 0,3 segundos."],
    ["Como te gustaba", "Vuelven las barras blancas al hablar, la isla del ancho de antes y sin la línea de cuenta regresiva."],
  ],
  "0.3.0": [
    ["Dictado a tu manera", "Manos libres con dos toques, tus palabras siempre bien escritas, atajos de texto y un estilo para cada app."],
    ["La isla te cuenta en qué anda", "Una rayita de color para escuchando, pensando, esperando, error y listo, y nueve barras que siguen tu voz."],
    ["Suelta archivos en la isla", "Sácale el texto, pregúntale por él o ábrelo. Y las capturas se anotan ahí mismo con flechas, recuadros y difuminado."],
    ["Escríbele y usa tu Claude", "Un lápiz en el menú para escribirle, y tu suscripción de Claude como cerebro, sin clave."],
  ],
};
