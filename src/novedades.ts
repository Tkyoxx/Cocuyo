// lo que trae cada version, para contarlo una vez despues de actualizar. la compilacion falla si falta la de la version

export const NOVEDADES: Record<string, [string, string][]> = {
  "0.4.3": [
    ["Las letras salen también jugando", "Con un juego a pantalla completa ya se ve lo que dices y lo que responde Cocuyo."],
    ["El visto de verdad", "Tus mensajes muestran enviado, entregado y leído en azul, y al abrir un chat les llega tu visto."],
    ["La IA entiende la conversación", "Si se lo pides, lee los últimos 20 mensajes de un chat y te propone una ficha de esa persona; solo se guarda si dices que sí."],
    ["Una barra a la medida", "El chat tiene su propia barra para subir y bajar, discreta, y ya no aparece la de lado."],
  ],
  "0.4.2": [
    ["La grabadora oye el PC en cualquier equipo", "Sigue al parlante de Windows aunque cambies a audífonos o Bluetooth, y en Ajustes → Capturas puedes probar qué llega al video."],
    ["Stickers, fotos y notas de voz en la burbuja", "Salen dentro del chat y las notas se escuchan ahí mismo. Enter envía y cada chat se cierra con su ×."],
    ["Pregúntale «¿alguien me escribió?»", "Te dice quién y qué, sin abrir el navegador."],
    ["Actualizarse ya no interrumpe", "Llega bajada y verificada: la pones ahora o se pone sola al reiniciar."],
  ],
  "0.4.1": [
    ["WhatsApp en una burbuja", "Llega una burbuja verde que flota; tócala para leer y responder ahí mismo, arrástrala de lado o lánzala fuera para descartarla."],
    ["Sonidos hechos para Cocuyo", "Una kalimba suave para los mensajes y su familia para abrir, enviar y cerrar. Bajitos de fábrica."],
    ["WhatsApp tiene su sección", "En Ajustes → WhatsApp eliges si sale el mensaje al lado, si se va sola, el sonido y el volumen."],
    ["Como lo pediste", "La rayita de estado se prende o se apaga en Ajustes → Isla, y las capturas ya no te preguntan qué hacer."],
  ],
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
