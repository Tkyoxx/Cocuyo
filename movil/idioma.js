// la pagina en ingles y portugues. lo que no este aqui se queda en español

const corto = (navigator.language || "es").slice(0, 2).toLowerCase();
export const idioma = corto === "en" || corto === "pt" ? corto : "es";

const EXACTO = {
  "Buscando tu PC…": ["Looking for your PC…", "Procurando seu PC…"],
  "Abierto en otra ventana": ["Open in another window", "Aberto em outra janela"],
  "Conecta tu celular a Cocuyo": ["Connect your phone to Cocuyo", "Conecte seu celular ao Cocuyo"],
  "Así le hablas a tu PC desde donde estés. Solo tu PC y este celular pueden hablarse: nadie más, ni siquiera el servidor que los junta.": [
    "Talk to your PC from wherever you are. Only your PC and this phone can talk to each other: nobody else, not even the server that brings them together.",
    "Assim você fala com seu PC de onde estiver. Só seu PC e este celular conseguem se falar: ninguém mais, nem o servidor que os junta.",
  ],
  "En tu PC, abre la isla y ve a": ["On your PC, open the island and go to", "No seu PC, abra a ilha e vá em"],
  "Ajustes → Remoto → Emparejar": ["Settings → Remote → Pair", "Ajustes → Remoto → Parear"],
  "Escanea con la cámara el QR que te muestra, o escribe aquí el código. Dura dos minutos.": ["Scan the QR it shows you with your camera, or type the code here. It lasts two minutes.", "Escaneie com a câmera o QR que ele mostra, ou digite o código aqui. Dura dois minutos."],
  "Escanea con la cámara del celular el QR que te muestra Cocuyo en tu PC. Así esta página sabe dónde encontrarlo.": [
    "Scan the QR that Cocuyo shows on your PC with your phone's camera. That's how this page knows where to find it.",
    "Escaneie com a câmera do celular o QR que o Cocuyo mostra no seu PC. Assim esta página sabe onde encontrá-lo.",
  ],
  Emparejar: ["Pair", "Parear"],
  "Tu celular crea su propia llave y nunca sale de aquí.": ["Your phone makes its own key and it never leaves it.", "Seu celular cria a própria chave e ela nunca sai daqui."],
  "Código de emparejamiento": ["Pairing code", "Código de pareamento"],
  Charla: ["Chat", "Conversa"],
  "Tu PC": ["Your PC", "Seu PC"],
  Ajustes: ["Settings", "Ajustes"],
  "Pídele lo que quieras a tu PC, o que haga algo aquí en el celular.": ["Ask your PC for anything, or have it do something here on your phone.", "Peça o que quiser ao seu PC, ou que faça algo aqui no celular."],
  "Pon algo de salsa": ["Play some salsa", "Toca uma salsa"],
  "Ponme una alarma a las 6:30": ["Set an alarm at 6:30", "Põe um alarme às 6:30"],
  "Busca vuelos a Cartagena": ["Search flights to Cartagena", "Pesquisa voos para Cartagena"],
  "¿Cómo está el clima?": ["What's the weather like?", "Como está o tempo?"],
  "Cómo llego al centro": ["Directions to downtown", "Como chego ao centro"],
  Conectado: ["Connected", "Conectado"],
  "Responder con voz": ["Answer out loud", "Responder com voz"],
  "Lee en voz alta lo que te responde.": ["Reads its answers out loud.", "Lê em voz alta o que responde."],
  "Instalar Cocuyo": ["Install Cocuyo", "Instalar o Cocuyo"],
  "Queda como una app más, abre sin internet y no se borra con el historial.": ["It stays like any other app, opens offline and isn't wiped with your history.", "Fica como mais um app, abre sem internet e não some com o histórico."],
  Instalar: ["Install", "Instalar"],
  "Instalar la app": ["Install the app", "Instalar o app"],
  "En iPhone: toca Compartir y luego «Agregar a inicio». Empareja desde la app instalada: guarda sus propias llaves.": [
    "On iPhone: tap Share, then “Add to Home Screen”. Pair from the installed app: it keeps its own keys.",
    "No iPhone: toque Compartilhar e depois «Adicionar à Tela de Início». Pareie pelo app instalado: ele guarda as próprias chaves.",
  ],
  "Olvidar este PC": ["Forget this PC", "Esquecer este PC"],
  "Borra la llave de este celular. Para volver, empareja otra vez.": ["Deletes this phone's key. To come back, pair again.", "Apaga a chave deste celular. Para voltar, pareie de novo."],
  Olvidar: ["Forget", "Esquecer"],
  "Todo viaja cifrado de punta a punta. El servidor que junta tu celular con tu PC solo ve paquetes que no puede abrir.": [
    "Everything travels end-to-end encrypted. The server that brings your phone and PC together only sees packets it can't open.",
    "Tudo viaja cifrado de ponta a ponta. O servidor que junta seu celular com seu PC só vê pacotes que não consegue abrir.",
  ],
  "Escríbele a Cocuyo": ["Write to Cocuyo", "Escreva para o Cocuyo"],
  Enviar: ["Send", "Enviar"],
  "Mantén para hablar": ["Hold to talk", "Segure para falar"],
  "Pensando…": ["Thinking…", "Pensando…"],
  "Te escucho…": ["Listening…", "Estou ouvindo…"],
  "Abriendo el micrófono…": ["Opening the mic…", "Abrindo o microfone…"],
  "No me dejaron usar el micrófono. Dale permiso en los ajustes del navegador.": ["I wasn't allowed to use the microphone. Give it permission in the browser settings.", "Não me deixaram usar o microfone. Dê permissão nos ajustes do navegador."],
  "Buenos días": ["Good morning", "Bom dia"],
  "Buenas tardes": ["Good afternoon", "Boa tarde"],
  "Buenas noches": ["Good evening", "Boa noite"],
  "Sin emparejar": ["Not paired", "Sem parear"],
  "Hablando con tu PC…": ["Talking to your PC…", "Falando com seu PC…"],
  "Apagado o sin Cocuyo": ["Off or without Cocuyo", "Desligado ou sem Cocuyo"],
  "Buscando…": ["Looking…", "Procurando…"],
  "por voz": ["by voice", "por voz"],
  "No llegué al punto de cita. Revisa tu conexión.": ["I couldn't reach the meeting point. Check your connection.", "Não cheguei ao ponto de encontro. Confira sua conexão."],
  "Tu PC no contestó a tiempo. Pide un código nuevo.": ["Your PC didn't answer in time. Ask for a new code.", "Seu PC não respondeu a tempo. Peça um código novo."],
  "Ese código no está esperando en ningún PC. Revisa las letras o pide uno nuevo.": ["No PC is waiting with that code. Check the letters or ask for a new one.", "Nenhum PC está esperando esse código. Confira as letras ou peça outro."],
  "Se cortó la conexión mientras emparejábamos.": ["The connection dropped while pairing.", "A conexão caiu enquanto pareávamos."],
  "El código tiene nueve letras y números, como K7Q-2XM-94P.": ["The code has nine letters and numbers, like K7Q-2XM-94P.", "O código tem nove letras e números, como K7Q-2XM-94P."],
  "Falta la dirección de tu punto de cita. Escanea con la cámara el QR que te muestra Cocuyo en tu PC.": [
    "Your meeting point address is missing. Scan the QR that Cocuyo shows on your PC with your camera.",
    "Falta o endereço do seu ponto de encontro. Escaneie com a câmera o QR que o Cocuyo mostra no seu PC.",
  ],
  "La respuesta no vino de tu PC. Empieza otra vez.": ["The answer didn't come from your PC. Start again.", "A resposta não veio do seu PC. Comece de novo."],
  "Eso es muy largo para mandarlo de una. Habla menos de un minuto.": ["That's too long to send at once. Talk for less than a minute.", "Isso é longo demais para mandar de uma vez. Fale menos de um minuto."],
  "Búsqueda en el celular": ["Search on your phone", "Pesquisa no celular"],
  Buscar: ["Search", "Pesquisar"],
  Abrir: ["Open", "Abrir"],
  "Cómo llegar": ["Directions", "Como chegar"],
  Ir: ["Go", "Ir"],
  Llamar: ["Call", "Ligar"],
  "Abrir WhatsApp": ["Open WhatsApp", "Abrir o WhatsApp"],
  "Elige el chat": ["Pick the chat", "Escolha a conversa"],
  "Alarma de Cocuyo": ["Cocuyo alarm", "Alarme do Cocuyo"],
  Recordatorio: ["Reminder", "Lembrete"],
  hoy: ["today", "hoje"],
  "Poner en el calendario": ["Add to calendar", "Pôr no calendário"],
  "Archivo .ics": [".ics file", "Arquivo .ics"],
  "Alarma (atajo)": ["Alarm (shortcut)", "Alarme (atalho)"],
  "Para pegar en el celular": ["To paste on your phone", "Para colar no celular"],
  Copiar: ["Copy", "Copiar"],
  Compartir: ["Share", "Compartilhar"],
};

const PATRONES = [
  [/^Conectado a (.+)$/, "Connected to $1", "Conectado a $1"],
  [/^Buscando a (.+)…$/, "Looking for $1…", "Procurando $1…"],
  [/^(.+) está apagado o sin Cocuyo$/, "$1 is off or without Cocuyo", "$1 está desligado ou sem Cocuyo"],
  [/^(.+) no respondió\. Puede que siga trabajando; mira tu PC\.$/, "$1 didn't answer. It may still be working; check your PC.", "$1 não respondeu. Pode ainda estar trabalhando; olhe o PC."],
  [/^¿Olvidar (.+)\? Tendrás que emparejar otra vez\.$/, "Forget $1? You'll have to pair again.", "Esquecer $1? Vai ter que parear de novo."],
  [/^(.+) no está conectado ahora\. Revisa que esté prendido con Cocuyo abierto\.$/, "$1 isn't connected right now. Make sure it's on with Cocuyo open.", "$1 não está conectado agora. Confira se está ligado com o Cocuyo aberto."],
  [/^Para \+(\d+)$/, "To +$1", "Para +$1"],
];

export function t(texto) {
  if (idioma === "es" || !texto) return texto;
  const centro = String(texto).trim();
  const i = idioma === "en" ? 0 : 1;
  if (EXACTO[centro]) return String(texto).replace(centro, EXACTO[centro][i]);
  for (const [patron, en, pt] of PATRONES) if (patron.test(centro)) return String(texto).replace(centro, centro.replace(patron, i === 0 ? en : pt));
  return texto;
}

export function traducirPagina(raiz = document.body) {
  document.documentElement.lang = idioma;
  if (idioma === "es") return;
  const paseo = document.createTreeWalker(raiz, NodeFilter.SHOW_TEXT);
  for (let nodo = paseo.nextNode(); nodo; nodo = paseo.nextNode()) if (nodo.nodeValue.trim()) nodo.nodeValue = t(nodo.nodeValue);
  for (const el of raiz.querySelectorAll("[placeholder], [aria-label]")) {
    for (const attr of ["placeholder", "aria-label"]) if (el.hasAttribute(attr)) el.setAttribute(attr, t(el.getAttribute(attr)));
  }
}
