// lo que cocuyo responde, en ingles y portugues

const REPLIES: &[(&str, &str, &str)] = &[
    ("Texto copiado: {} palabras.", "Text copied: {} words.", "Texto copiado: {} palavras."),
    ("Ahora mismo nadie está forzando el procesador. {}", "Nothing is straining the processor right now. {}", "Agora mesmo nada está forçando o processador. {}"),
    ("Lo que más memoria ocupa es {}, con {} GB.", "What takes the most memory is {}, with {} GB.", "O que mais ocupa memória é {}, com {} GB."),
    ("Lo que más trabaja ahora es {}, con el {} % del procesador. {} Di «cierra {}» si quieres cerrarlo.", "What's working hardest now is {}, at {}% of the processor. {} Say “close {}” if you want to close it.", "O que mais trabalha agora é {}, com {} % do processador. {} Diga «fecha {}» se quiser fechar."),
    ("No encontré texto en esa imagen.", "I found no text in that image.", "Não encontrei texto nessa imagem."),
    ("No hay ninguna imagen reciente. Rodea algo con el lazo y vuelve a pedirlo.", "There's no recent image. Circle something with the lasso and ask again.", "Não há nenhuma imagem recente. Circule algo com o laço e peça de novo."),
    ("Windows no tiene instalado el lector de texto para tu idioma.", "Windows doesn't have the text reader installed for your language.", "O Windows não tem o leitor de texto instalado para o seu idioma."),
    ("Selecciona primero el texto que quieres cambiar.", "First select the text you want to change.", "Selecione primeiro o texto que quer mudar."),
    ("Para cambiar un texto hace falta un modelo de IA: conéctalo en Ajustes, sección IA.", "Changing text needs an AI model: connect one in Settings, AI section.", "Para mudar um texto é preciso um modelo de IA: conecte em Ajustes, seção IA."),
    ("Micrófono activado.", "Microphone on.", "Microfone ligado."),
    ("Micrófono silenciado.", "Microphone muted.", "Microfone silenciado."),
    ("Cerrando la ventana.", "Closing the window.", "Fechando a janela."),
    ("No hay ninguna ventana delante que cerrar.", "There's no window in front to close.", "Não há nenhuma janela na frente para fechar."),
    ("Cerrando {}.", "Closing {}.", "Fechando {}."),
    ("No veo ninguna app abierta llamada «{}».", "I don't see any open app called “{}”.", "Não vejo nenhum app aberto chamado «{}»."),
    ("Suena «{}» de {}.", "Playing “{}” by {}.", "Está tocando «{}» de {}."),
    ("Suena «{}».", "Playing “{}”.", "Está tocando «{}»."),
    ("En cola: «{}» de {}.", "Queued: “{}” by {}.", "Na fila: «{}» de {}."),
    ("Guardada en tus Me gusta: «{}».", "Saved to your Liked Songs: “{}”.", "Salva nas suas Curtidas: «{}»."),
    ("No encontré «{}» en Spotify.", "I couldn't find “{}” on Spotify.", "Não encontrei «{}» no Spotify."),
    ("No encontré ninguna ciudad llamada «{}».", "I couldn't find a city called “{}”.", "Não encontrei nenhuma cidade chamada «{}»."),
    ("En {} hay {} grados, {}. Hoy, máxima de {} y mínima de {}; {} % de probabilidad de lluvia.", "In {} it's {} degrees, {}. Today, a high of {} and a low of {}; {}% chance of rain.", "Em {} faz {} graus, {}. Hoje, máxima de {} e mínima de {}; {} % de chance de chuva."),
    ("No hay conexión con el servicio del clima.", "Can't reach the weather service.", "Sem conexão com o serviço do clima."),
    ("Escribe el nombre de tu ciudad.", "Type the name of your city.", "Escreva o nome da sua cidade."),
    ("Ahora mismo no suena nada.", "Nothing is playing right now.", "Não está tocando nada agora."),
    ("Aleatorio activado.", "Shuffle on.", "Aleatório ligado."),
    ("Aleatorio desactivado.", "Shuffle off.", "Aleatório desligado."),
    ("Ya no se repite.", "Repeat is off.", "Não repete mais."),
    ("Repitiendo la lista.", "Repeating the playlist.", "Repetindo a playlist."),
    ("Repitiendo esta canción.", "Repeating this song.", "Repetindo esta música."),
    ("Spotify no tiene nada sonando en ningún dispositivo.", "Spotify has nothing playing on any device.", "O Spotify não tem nada tocando em nenhum dispositivo."),
    ("A la sesión de Spotify le falta ese permiso. Desconecta y vuelve a conectar Spotify en Ajustes.", "The Spotify session lacks that permission. Disconnect and reconnect Spotify in Settings.", "A sessão do Spotify não tem essa permissão. Desconecte e conecte o Spotify de novo em Ajustes."),
    ("Lo recordaré: {}", "I'll remember: {}", "Vou lembrar: {}"),
    ("No tenía nada guardado sobre «{}».", "I had nothing saved about “{}”.", "Eu não tinha nada guardado sobre «{}»."),
    ("Tienes {}: {}", "You have {}: {}", "Você tem {}: {}"),
    ("Quitado: {}.", "Removed: {}.", "Removido: {}."),
    ("Quitados {}.", "Removed {}.", "Removidos {}."),
    ("No encontré nada programado con «{}».", "I found nothing scheduled with “{}”.", "Não encontrei nada programado com «{}»."),
    ("Memoria borrada.", "Memory cleared.", "Memória apagada."),
    ("Olvidado.", "Forgotten.", "Esquecido."),
    ("No tienes nada programado.", "You have nothing scheduled.", "Você não tem nada programado."),
    ("Dime cuál: «cancela el recordatorio de la pizza», o «borra todos los recordatorios».", "Tell me which: “cancel the pizza reminder”, or “delete all reminders”.", "Diga qual: «cancela o lembrete da pizza», ou «apaga todos os lembretes»."),
    ("No quedó imagen de lo que rodeaste.", "No image was left of what you circled.", "Não ficou imagem do que você circulou."),
    ("El modelo no respondió nada. Puede que no sepa mirar imágenes: prueba con otro.", "The model answered nothing. It may not be able to look at images: try another one.", "O modelo não respondeu nada. Talvez não saiba ver imagens: tente outro."),
    ("Programado en {} s: «{}».", "Scheduled in {} s: “{}”.", "Programado em {} s: «{}»."),
    ("Te aviso en {} s: {}", "I'll remind you in {} s: {}", "Eu te aviso em {} s: {}"),
    ("Programado en {} min: «{}».", "Scheduled in {} min: “{}”.", "Programado em {} min: «{}»."),
    ("Te aviso en {} min: {}", "I'll remind you in {} min: {}", "Eu te aviso em {} min: {}"),
    ("Programado mañana a las {}: «{}».", "Scheduled tomorrow at {}: “{}”.", "Programado amanhã às {}: «{}»."),
    ("Te aviso mañana a las {}: {}", "I'll remind you tomorrow at {}: {}", "Eu te aviso amanhã às {}: {}"),
    ("Programado cada día a las {}: «{}».", "Scheduled every day at {}: “{}”.", "Programado todo dia às {}: «{}»."),
    ("Te aviso cada día a las {}: {}", "I'll remind you every day at {}: {}", "Eu te aviso todo dia às {}: {}"),
    ("Programado de lunes a viernes a las {}: «{}».", "Scheduled Monday to Friday at {}: “{}”.", "Programado de segunda a sexta às {}: «{}»."),
    ("Te aviso de lunes a viernes a las {}: {}", "I'll remind you Monday to Friday at {}: {}", "Eu te aviso de segunda a sexta às {}: {}"),
    ("Programado los fines de semana a las {}: «{}».", "Scheduled on weekends at {}: “{}”.", "Programado nos fins de semana às {}: «{}»."),
    ("Te aviso los fines de semana a las {}: {}", "I'll remind you on weekends at {}: {}", "Eu te aviso nos fins de semana às {}: {}"),
    ("Programado a las {}: «{}».", "Scheduled at {}: “{}”.", "Programado às {}: «{}»."),
    ("Te aviso a las {}: {}", "I'll remind you at {}: {}", "Eu te aviso às {}: {}"),
    ("Programado el {} a las {}: «{}».", "Scheduled {} at {}: “{}”.", "Programado {} às {}: «{}»."),
    ("Te aviso el {} a las {}: {}", "I'll remind you {} at {}: {}", "Eu te aviso {} às {}: {}"),
    ("Programado al encender el equipo: «{}».", "Scheduled for when the PC starts: “{}”.", "Programado ao ligar o PC: «{}»."),
    ("Se pasó la hora: {}", "The time passed: {}", "Passou da hora: {}"),
    ("Se cumplió el tiempo.", "Time's up.", "O tempo acabou."),
    ("Ya es la hora.", "It's time.", "Está na hora."),
    ("«{}» sigue trabajando.", "“{}” is still working.", "«{}» continua trabalhando."),
    ("Hecho: {}.", "Done: {}.", "Feito: {}."),
    ("«{}» terminó con un error.", "“{}” ended with an error.", "«{}» terminou com um erro."),
    ("«{}» falló: {}", "“{}” failed: {}", "«{}» falhou: {}"),
    ("No tengo ningún script llamado «{}».", "I have no script called “{}”.", "Não tenho nenhum script chamado «{}»."),
    ("Para ejecutar «{}» falta instalar el programa que lo interpreta.", "To run “{}” you need to install the program that interprets it.", "Para executar «{}» falta instalar o programa que o interpreta."),
    ("No pude ejecutar «{}»: {}", "I couldn't run “{}”: {}", "Não consegui executar «{}»: {}"),
    ("No entendí «{}», y la IA no ayudó: {}", "I didn't understand “{}”, and the AI didn't help: {}", "Não entendi «{}», e a IA não ajudou: {}"),
    ("Sonando «{}» de {}.", "Playing “{}” by {}.", "Tocando «{}» de {}."),
    ("Sonando tu lista «{}».", "Playing your playlist “{}”.", "Tocando sua playlist «{}»."),
    ("Sonando la lista «{}».", "Playing the playlist “{}”.", "Tocando a playlist «{}»."),
    ("Sonando {}.", "Playing {}.", "Tocando {}."),
    ("Algo al azar de tu lista «{}».", "Something random from your playlist “{}”.", "Algo aleatório da sua playlist «{}»."),
    ("Puse el modo aleatorio y salté a otra canción.", "I turned shuffle on and skipped to another song.", "Liguei o aleatório e pulei para outra música."),
    ("Otra vez, desde el principio.", "Again, from the start.", "De novo, do começo."),
    ("En pausa.", "Paused.", "Em pausa."),
    ("Sonando.", "Playing.", "Tocando."),
    ("Siguiente.", "Next.", "Próxima."),
    ("Anterior.", "Previous.", "Anterior."),
    ("Spotify al {} %.", "Spotify at {}%.", "Spotify em {}%."),
    ("Spotify en silencio.", "Spotify muted.", "Spotify no mudo."),
    ("Volumen del equipo al {} %.", "System volume at {}%.", "Volume do sistema em {}%."),
    ("Volumen del equipo en silencio.", "System muted.", "Sistema no mudo."),
    ("Spotify no está sonando. {}", "Spotify isn't playing. {}", "O Spotify não está tocando. {}"),
    (
        "Buscando «{}» en Spotify. Conecta Spotify en Ajustes para que la ponga a sonar directamente.",
        "Searching “{}” on Spotify. Connect Spotify in Settings so I can play it directly.",
        "Buscando «{}» no Spotify. Conecte o Spotify em Ajustes para eu tocar direto.",
    ),
    (
        "Buscando «{}» en Spotify. Conecta Spotify en Ajustes para que lo ponga a sonar directamente.",
        "Searching “{}” on Spotify. Connect Spotify in Settings so I can play it directly.",
        "Buscando «{}» no Spotify. Conecte o Spotify em Ajustes para eu tocar direto.",
    ),
    ("Conecta Spotify en Ajustes.", "Connect Spotify in Settings.", "Conecte o Spotify em Ajustes."),
    ("Para poner tus listas, conecta Spotify en Ajustes.", "To play your playlists, connect Spotify in Settings.", "Para tocar suas playlists, conecte o Spotify em Ajustes."),
    ("Spotify no encontró nada para «{}».", "Spotify found nothing for “{}”.", "O Spotify não achou nada para «{}»."),
    ("Spotify no respondió a la búsqueda.", "Spotify didn't answer the search.", "O Spotify não respondeu à busca."),
    ("Spotify no devolvió tus listas.", "Spotify didn't return your playlists.", "O Spotify não devolveu suas playlists."),
    ("No tienes listas en Spotify de donde elegir.", "You have no Spotify playlists to pick from.", "Você não tem playlists no Spotify para escolher."),
    ("No encontré ninguna lista tuya llamada «{}».", "I found no playlist of yours called “{}”.", "Não achei nenhuma playlist sua chamada «{}»."),
    ("No hay nada sonando que pueda repetir.", "Nothing is playing that I can repeat.", "Não há nada tocando que eu possa repetir."),
    (
        "No hay nada sonando que pueda mezclar. Conecta Spotify en Ajustes para elegir de tus listas.",
        "Nothing is playing that I can shuffle. Connect Spotify in Settings to pick from your playlists.",
        "Não há nada tocando para embaralhar. Conecte o Spotify em Ajustes para escolher das suas playlists.",
    ),
    (
        "Spotify recibió la orden pero no empezó a sonar. Toca reproducir una vez en Spotify y vuelve a pedírmelo.",
        "Spotify got the order but didn't start playing. Press play once in Spotify and ask me again.",
        "O Spotify recebeu a ordem mas não começou a tocar. Aperte play uma vez no Spotify e peça de novo.",
    ),
    (
        "Abrí Spotify, pero no llegó a estar listo. Inténtalo otra vez en un momento.",
        "I opened Spotify, but it didn't get ready. Try again in a moment.",
        "Abri o Spotify, mas ele não ficou pronto. Tente de novo em um momento.",
    ),
    ("Spotify solo deja elegir la canción desde fuera con una cuenta Premium.", "Spotify only lets me pick the song with a Premium account.", "O Spotify só deixa escolher a música com uma conta Premium."),
    ("La sesión de Spotify venció. Vuelve a conectarla en Ajustes.", "The Spotify session expired. Connect it again in Settings.", "A sessão do Spotify expirou. Conecte de novo em Ajustes."),
    ("Windows no dejó cambiar el volumen.", "Windows didn't let me change the volume.", "O Windows não deixou mudar o volume."),
    ("Abriendo {}.", "Opening {}.", "Abrindo {}."),
    ("Abriendo la página.", "Opening the page.", "Abrindo a página."),
    ("Buscando «{}» en YouTube.", "Searching “{}” on YouTube.", "Buscando «{}» no YouTube."),
    ("Buscando «{}» en Google.", "Searching “{}” on Google.", "Buscando «{}» no Google."),
    ("Equipo bloqueado.", "Computer locked.", "Computador bloqueado."),
    (
        "No encontré ninguna app llamada «{}». Puedes añadirla en la pestaña Apps.",
        "I found no app called “{}”. You can add it in the Apps tab.",
        "Não achei nenhum app chamado «{}». Você pode adicioná-lo na aba Apps.",
    ),
    (
        "No entendí qué hacer con «{}». Prueba con «pon…», «abre…» o «busca…».",
        "I didn't get what to do with “{}”. Try “play…”, “open…” or “search…”.",
        "Não entendi o que fazer com «{}». Tente «toca…», «abre…» ou «busca…».",
    ),
    ("No te escuché. Mantén las teclas mientras hablas.", "I didn't hear you. Hold the keys while you speak.", "Não te ouvi. Segure as teclas enquanto fala."),
    (
        "Falta la voz local. Descárgala en la pestaña Ajustes del menú.",
        "The local voice is missing. Download it in the menu's Settings tab.",
        "Falta a voz local. Baixe-a na aba Ajustes do menu.",
    ),
    ("El motor de voz falló al transcribir.", "The voice engine failed to transcribe.", "O motor de voz falhou ao transcrever."),
    ("Mis Notas no aceptó la nota.", "Mis Notas didn't accept the note.", "O Mis Notas não aceitou a nota."),
    ("No encontré Mis Notas instalada en este equipo.", "I didn't find Mis Notas installed on this computer.", "Não achei o Mis Notas instalado neste computador."),
    (
        "Mis Notas se abrió, pero no respondió a tiempo. Inténtalo otra vez.",
        "Mis Notas opened, but didn't answer in time. Try again.",
        "O Mis Notas abriu, mas não respondeu a tempo. Tente de novo.",
    ),
    ("Abrí Mis Notas y guardé tu nota.", "I opened Mis Notas and saved your note.", "Abri o Mis Notas e salvei sua nota."),
    ("Anotado en Mis Notas.", "Saved to Mis Notas.", "Anotado no Mis Notas."),
    ("El micrófono no está listo.", "The microphone isn't ready.", "O microfone não está pronto."),
    ("El micrófono no respondió.", "The microphone didn't respond.", "O microfone não respondeu."),
    ("No hay ningún micrófono conectado.", "No microphone is connected.", "Não há nenhum microfone conectado."),
    ("No se estaba grabando.", "Nothing was being recorded.", "Não estava gravando."),
    ("Spotify no está abierto y no lo pude abrir.", "Spotify isn't open and I couldn't open it.", "O Spotify não está aberto e não consegui abrir."),
    ("No se pudo descomprimir el motor de voz.", "Couldn't unpack the voice engine.", "Não foi possível descompactar o motor de voz."),
    ("No se pudo usar el portapapeles.", "Couldn't use the clipboard.", "Não foi possível usar a área de transferência."),
    ("La tecla Windows se había quedado pegada: la solté.", "The Windows key was stuck: I released it.", "A tecla Windows tinha ficado presa: soltei."),
    ("Windows no tiene instalada ninguna voz en ese idioma", "Windows has no voice installed for that language", "O Windows não tem nenhuma voz instalada nesse idioma"),
    ("El micrófono no responde: {}", "The microphone isn't responding: {}", "O microfone não responde: {}"),
    ("No se pudo abrir el micrófono: {}", "Couldn't open the microphone: {}", "Não foi possível abrir o microfone: {}"),
    ("Formato de micrófono no soportado: {}", "Unsupported microphone format: {}", "Formato de microfone não suportado: {}"),
    ("El motor de voz no arrancó: {}", "The voice engine didn't start: {}", "O motor de voz não iniciou: {}"),
    ("Windows no pudo abrir «{}».", "Windows couldn't open “{}”.", "O Windows não conseguiu abrir «{}»."),
    ("Ver la guía otra vez", "Show the guide again", "Ver o guia de novo"),
    ("Ajustes…", "Settings…", "Ajustes…"),
    ("Soltar teclas atascadas", "Release stuck keys", "Soltar teclas presas"),
    ("Desactivar extensiones", "Turn off extensions", "Desativar extensões"),
    ("Salir de Cocuyo", "Quit Cocuyo", "Sair do Cocuyo"),
    ("Programas y accesos directos", "Programs and shortcuts", "Programas e atalhos"),
    ("Desinstalar Cocuyo", "Uninstall Cocuyo", "Desinstalar o Cocuyo"),
    ("Cocuyo se desinstaló.", "Cocuyo was uninstalled.", "O Cocuyo foi desinstalado."),
    ("¿Borrar también tus ajustes, tus tareas y la voz descargada (406 MB)?\n\nSi piensas volver a instalar Cocuyo, elige No.", "Also delete your settings, your tasks and the downloaded voice (406 MB)?\n\nIf you plan to install Cocuyo again, choose No.", "Apagar também seus ajustes, suas tarefas e a voz baixada (406 MB)?\n\nSe você pensa em instalar o Cocuyo de novo, escolha Não."),
    ("Tu clave de Gemini no trae el modelo de transcripción en vivo.", "Your Gemini key doesn't include the live transcription model.", "Sua chave do Gemini não inclui o modelo de transcrição ao vivo."),
    ("Para tomar apuntes hace falta tu clave de Gemini (sección IA).", "Taking notes needs your Gemini key (AI section).", "Para fazer anotações é preciso sua chave do Gemini (seção IA)."),
    ("Para tomar apuntes hace falta Gemini con tu clave (sección IA).", "Taking notes needs Gemini with your key (AI section).", "Para fazer anotações é preciso o Gemini com sua chave (seção IA)."),
    ("El modelo de transcripción no arrancó: {}", "The transcription model didn't start: {}", "O modelo de transcrição não iniciou: {}"),
    ("No pude escuchar ni el PC ni el micrófono.", "I couldn't hear either the PC or the microphone.", "Não consegui ouvir nem o PC nem o microfone."),
    ("Ya estoy tomando apuntes.", "I'm already taking notes.", "Já estou fazendo anotações."),
    ("Tomando apuntes de lo que suena en el PC y de tu micrófono. Di «termina los apuntes» cuando acabe, y avísales a los demás que los estás grabando.", "Taking notes of what plays on the PC and your microphone. Say “stop taking notes” when it's over, and let the others know you're recording them.", "Fazendo anotações do que toca no PC e do seu microfone. Diga «para as anotações» quando acabar, e avise os outros que você está gravando."),
    ("No estaba tomando apuntes.", "I wasn't taking notes.", "Eu não estava fazendo anotações."),
    ("La transcripción no respondió.", "The transcription didn't answer.", "A transcrição não respondeu."),
    ("Se cortaron los apuntes: {}", "The notes got cut off: {}", "As anotações foram interrompidas: {}"),
    ("No alcancé a oír nada que apuntar.", "I didn't hear anything worth writing down.", "Não cheguei a ouvir nada para anotar."),
    ("Listo, guardé {} de apuntes con su resumen en Documentos, Cocuyo, Apuntes.", "Done, I saved {} of notes with a summary in Documents, Cocuyo, Apuntes.", "Pronto, salvei {} de anotações com o resumo em Documentos, Cocuyo, Apuntes."),
    ("{}. ¿Lo hago?", "{}. Should I do it?", "{}. Faço?"),
    ("Enviar «{}» a {} por {}", "Send “{}” to {} on {}", "Enviar «{}» para {} pelo {}"),
    ("Ventana «{}»{}", "Window “{}”{}", "Janela «{}»{}"),
    ("Listo, le mandé a {}: «{}».", "Done, I sent {}: “{}”.", "Pronto, mandei para {}: «{}»."),
    ("Quedó sin enviar.", "It wasn't sent.", "Ficou sem enviar."),
    ("Listo, te devolví el mouse. Me detuve ahí.", "Done, the mouse is yours again. I stopped there.", "Pronto, devolvi o mouse. Parei aí."),
    ("Me enredé con tantos pasos. Pídemelo por partes.", "I got tangled up in so many steps. Ask me in parts.", "Me enrolei com tantos passos. Peça por partes."),
    ("No hay ninguna ventana abierta para mirar.", "There's no open window to look at.", "Não há nenhuma janela aberta para olhar."),
    ("«{}» está abierta, pero Windows no me dejó traerla al frente.", "“{}” is open, but Windows didn't let me bring it to the front.", "«{}» está aberta, mas o Windows não me deixou trazê-la para a frente."),
    ("No encontré ninguna app llamada «{}» abierta ni instalada.", "I couldn't find any app called “{}”, open or installed.", "Não encontrei nenhum app chamado «{}» aberto nem instalado."),
    ("Abrí {}, pero su ventana no apareció.", "I opened {}, but its window didn't show up.", "Abri o {}, mas a janela não apareceu."),
    ("no vi el buscador de chats", "I didn't see the chat search box", "não vi a busca de conversas"),
    ("no encontré a {} en {}", "I couldn't find {} in {}", "não encontrei {} no {}"),
    ("no vi dónde se escribe el mensaje", "I didn't see where to type the message", "não vi onde se escreve a mensagem"),
    ("el chat que se abrió no parece de {} ({})", "the chat that opened doesn't look like {}'s ({})", "a conversa que abriu não parece ser de {} ({})"),
    ("la ventana se cerró", "the window closed", "a janela fechou"),
    ("Moviste el mouse: te devolví el control.", "You moved the mouse: you're back in control.", "Você mexeu no mouse: devolvi o controle."),
    ("Falta la clave de Gemini.", "The Gemini key is missing.", "Falta a chave do Gemini."),
    ("Tu clave no tiene ningún modelo de voz en vivo.", "Your key doesn't have any live voice model.", "Sua chave não tem nenhum modelo de voz ao vivo."),
    ("No hay conexión con Google.", "Can't reach Google.", "Sem conexão com o Google."),
    ("No llegué a Google.", "I couldn't reach Google.", "Não consegui chegar ao Google."),
    ("No pude abrir la conexión.", "I couldn't open the connection.", "Não consegui abrir a conexão."),
    ("Google no completó la conexión en vivo.", "Google didn't complete the live connection.", "O Google não completou a conexão ao vivo."),
    ("Gemini Live no arrancó: {}", "Gemini Live didn't start: {}", "O Gemini Live não iniciou: {}"),
    ("Gemini se quedó callado.", "Gemini went quiet.", "O Gemini ficou calado."),
    ("No hay altavoces.", "There are no speakers.", "Não há alto-falantes."),
    ("Falta la voz local. Descárgala en la pestaña Ajustes del menú.", "The local voice is missing. Download it in the Settings tab of the menu.", "Falta a voz local. Baixe na aba Ajustes do menu."),
    ("No me dijiste qué hacer.", "You didn't tell me what to do.", "Você não me disse o que fazer."),
    ("No te escuché.", "I didn't hear you.", "Não te ouvi."),
    ("Eso tiene varios pasos: enciende el modo agente en Ajustes, sección Agente, y lo hago yo.", "That takes several steps: turn on agent mode in Settings, Agent section, and I'll do it.", "Isso tem vários passos: ligue o modo agente em Ajustes, seção Agente, e eu faço."),
    ("No entendí qué hacer con «{}». Prueba con «pon…», «abre…» o «busca…».", "I didn't get what to do with “{}”. Try “play…”, “open…” or “search…”.", "Não entendi o que fazer com «{}». Tente «toca…», «abre…» ou «busca…»."),
    ("Buscando «{}» en Spotify. Conecta Spotify en Ajustes para que la ponga a sonar directamente.", "Searching “{}” on Spotify. Connect Spotify in Settings so I can play it directly.", "Buscando «{}» no Spotify. Conecte o Spotify em Ajustes para eu tocar direto."),
    ("Buscando «{}» en Spotify. Conecta Spotify en Ajustes para que lo ponga a sonar directamente.", "Searching “{}” on Spotify. Connect Spotify in Settings so I can play it directly.", "Buscando «{}» no Spotify. Conecte o Spotify em Ajustes para eu tocar direto."),
    ("No hay nada sonando que pueda mezclar. Conecta Spotify en Ajustes para elegir de tus listas.", "Nothing is playing that I can shuffle. Connect Spotify in Settings to pick from your playlists.", "Não há nada tocando que eu possa misturar. Conecte o Spotify em Ajustes para escolher das suas playlists."),
    ("No encontré ninguna app llamada «{}». Puedes añadirla en la pestaña Apps.", "I couldn't find any app called “{}”. You can add it in the Apps tab.", "Não encontrei nenhum app chamado «{}». Você pode adicionar na aba Apps."),
    ("No hay respuesta en esa dirección. ¿Está encendido el servicio?", "No answer at that address. Is the service on?", "Sem resposta nesse endereço. O serviço está ligado?"),
    ("El servicio rechazó la clave.", "The service rejected the key.", "O serviço recusou a chave."),
    ("El servicio no conoce el modelo «{}» o esa dirección.", "The service doesn't know the model “{}” or that address.", "O serviço não conhece o modelo «{}» ou esse endereço."),
    ("«{}» llegó al límite de su plan. Lo dejo descansar.", "“{}” reached its plan's limit. I'm letting it rest.", "«{}» chegou ao limite do plano. Vou deixar descansar."),
    ("El servicio de «{}» está saturado ahora mismo. Prueba en un rato.", "The “{}” service is overloaded right now. Try again in a while.", "O serviço de «{}» está sobrecarregado agora. Tente daqui a pouco."),
    ("El servicio respondió con el error {}.", "The service answered with error {}.", "O serviço respondeu com o erro {}."),
    ("El servicio respondió: {}", "The service answered: {}", "O serviço respondeu: {}"),
    ("Ese servicio no escucha audio.", "That service doesn't listen to audio.", "Esse serviço não escuta áudio."),
    ("Hoy ya se usó el cupo de los modelos; hasta mañana sigo con lo que entiendo solo.", "Today's quota for the models is used up; until tomorrow I'll go on with what I understand by myself.", "A cota dos modelos de hoje acabou; até amanhã sigo com o que entendo sozinho."),
    ("El modo agente necesita un servicio con formato OpenAI: Gemini, Groq, Ollama, LM Studio, OpenAI o DeepSeek.", "Agent mode needs a service with the OpenAI format: Gemini, Groq, Ollama, LM Studio, OpenAI or DeepSeek.", "O modo agente precisa de um serviço com formato OpenAI: Gemini, Groq, Ollama, LM Studio, OpenAI ou DeepSeek."),
    ("El modelo respondió algo vacío.", "The model answered with nothing.", "O modelo respondeu algo vazio."),
    ("El modelo no oyó nada.", "The model didn't hear anything.", "O modelo não ouviu nada."),
    ("El modelo no devolvió nada.", "The model returned nothing.", "O modelo não devolveu nada."),
    ("La voz de Gemini necesita que el servicio de IA elegido sea Gemini.", "The Gemini voice needs Gemini as the chosen AI service.", "A voz do Gemini precisa que o serviço de IA escolhido seja o Gemini."),
    ("El servicio no devolvió audio.", "The service returned no audio.", "O serviço não devolveu áudio."),
    ("(No pude hacer el resumen: {})", "(I couldn't write the summary: {})", "(Não consegui fazer o resumo: {})"),
    ("Enciende Remoto primero.", "Turn on Remote first.", "Ligue o Remoto primeiro."),
    ("Falta el punto de cita: pon su dirección en Ajustes, sección Remoto.", "The meeting point is missing: put its address in Settings, Remote section.", "Falta o ponto de encontro: coloque o endereço em Ajustes, seção Remoto."),
    ("La dirección del punto de cita está mal escrita.", "The meeting point address is mistyped.", "O endereço do ponto de encontro está errado."),
    ("No llegué al punto de cita.", "I couldn't reach the meeting point.", "Não cheguei ao ponto de encontro."),
    ("Eso no parece una llave de Cloudflare. Cópiala completa desde la página donde la creaste.", "That doesn't look like a Cloudflare key. Copy all of it from the page where you made it.", "Isso não parece uma chave da Cloudflare. Copie ela inteira da página onde você criou."),
    ("La llave no ve ninguna cuenta. Créala con la plantilla «Edit Cloudflare Workers».", "The key can't see any account. Make it with the “Edit Cloudflare Workers” template.", "A chave não vê nenhuma conta. Crie com o modelo «Edit Cloudflare Workers»."),
    ("Cloudflare no aceptó la llave. Créala con la plantilla «Edit Cloudflare Workers» y pégala completa.", "Cloudflare didn't accept the key. Make it with the “Edit Cloudflare Workers” template and paste all of it.", "A Cloudflare não aceitou a chave. Crie com o modelo «Edit Cloudflare Workers» e cole ela inteira."),
    ("No hubo respuesta de Cloudflare. Revisa tu conexión.", "Cloudflare didn't answer. Check your connection.", "A Cloudflare não respondeu. Confira sua conexão."),
    ("Cloudflare respondió: {}", "Cloudflare answered: {}", "A Cloudflare respondeu: {}"),
    ("un error sin explicación", "an error with no explanation", "um erro sem explicação"),
    ("No pude abrir curl.exe.", "I couldn't open curl.exe.", "Não consegui abrir o curl.exe."),
    ("Grabando la pantalla. Pulsa el mismo atajo para parar.", "Recording the screen. Press the same shortcut to stop.", "Gravando a tela. Aperte o mesmo atalho para parar."),
    ("Grabación guardada: {}", "Recording saved: {}", "Gravação salva: {}"),
    ("Ya estoy grabando.", "I'm already recording.", "Já estou gravando."),
    ("No estaba grabando.", "I wasn't recording.", "Eu não estava gravando."),
    ("Grabando la pantalla.", "Recording the screen.", "Gravando a tela."),
    ("Grabación guardada: {}.", "Recording saved: {}.", "Gravação salva: {}."),
    ("Guardé lo último: {}", "Saved the last bit: {}", "Salvei o último: {}"),
    ("Guardé lo último: {}.", "Saved the last bit: {}.", "Salvei o último: {}."),
    ("Enciende la repetición instantánea en Ajustes → Capturas.", "Turn on instant replay in Settings → Captures.", "Ligue o replay instantâneo em Ajustes → Capturas."),
    ("Todavía no hay nada guardado en la repetición.", "There's nothing in the replay yet.", "Ainda não há nada no replay."),
    ("La grabadora se detuvo: {}", "The recorder stopped: {}", "O gravador parou: {}"),
    ("Ya hay cinco celulares. Quita uno antes de emparejar otro.", "There are already five phones. Remove one before pairing another.", "Já há cinco celulares. Remova um antes de parear outro."),
    ("Se acabó el tiempo para emparejar.", "Time to pair ran out.", "O tempo para parear acabou."),
    ("El celular mandó una llave rara.", "The phone sent a strange key.", "O celular mandou uma chave estranha."),
    ("El código no coincide. Empieza otra vez.", "The code doesn't match. Start again.", "O código não confere. Comece de novo."),
    ("Windows no pudo cifrar la llave.", "Windows couldn't encrypt the key.", "O Windows não conseguiu cifrar a chave."),
    ("Tu celular «{}» se conectó a Cocuyo.", "Your phone “{}” connected to Cocuyo.", "Seu celular «{}» se conectou ao Cocuyo."),
    ("Orden desde tu celular.", "Order from your phone.", "Ordem do seu celular."),
    ("No te escuché. Mantén el botón mientras hablas.", "I didn't hear you. Hold the button while you talk.", "Não te ouvi. Segure o botão enquanto fala."),
    ("Te dejo el camino en el celular.", "Here's the way, on your phone.", "Deixei o caminho no celular."),
    ("Te la dejo para abrir en el celular.", "Here it is to open on your phone.", "Deixei para abrir no celular."),
    ("Alarma {}: tócala para dejarla en el celular.", "Alarm {}: tap it to set it on your phone.", "Alarme {}: toque para deixar no celular."),
    ("Recordatorio {}: tócalo para dejarlo en el celular.", "Reminder {}: tap it to set it on your phone.", "Lembrete {}: toque para deixar no celular."),
    ("Hecho.", "Done.", "Feito."),
    ("Faltan datos para dejarlo en el celular.", "Some details are missing to leave it on your phone.", "Faltam dados para deixar no celular."),
    ("Tu celular no está conectado ahora. Abre Cocuyo en el celular y vuelve a pedirlo.", "Your phone isn't connected right now. Open Cocuyo on your phone and ask again.", "Seu celular não está conectado agora. Abra o Cocuyo no celular e peça de novo."),
    ("{}\n{}", "{}\n{}", "{}\n{}"),
];

fn holes<'a>(template: &str, text: &'a str) -> Option<Vec<&'a str>> {
    let mut literals = template.split("{}");
    let mut rest = text.strip_prefix(literals.next()?)?;
    let literals: Vec<&str> = literals.collect();
    let mut found = Vec::with_capacity(literals.len());
    for (index, literal) in literals.iter().enumerate() {
        if index + 1 == literals.len() {
            found.push(rest.strip_suffix(literal)?);
            rest = "";
        } else {
            let at = rest.find(literal)?;
            found.push(&rest[..at]);
            rest = &rest[at + literal.len()..];
        }
    }
    rest.is_empty().then_some(found)
}

fn translate(text: &str, language: &str) -> String {
    let column = match language {
        "en" => 1,
        "pt" => 2,
        _ => return text.to_string(),
    };
    for (spanish, english, portuguese) in REPLIES {
        let Some(found) = holes(spanish, text) else { continue };
        let mut out = String::new();
        let mut pieces = [english, portuguese][column - 1].split("{}");
        out.push_str(pieces.next().unwrap_or_default());
        for (hole, piece) in found.iter().zip(pieces) {
            out.push_str(&translate(hole, language));
            out.push_str(piece);
        }
        return out;
    }
    text.to_string()
}

pub fn tr(text: &str) -> String {
    translate(text, crate::cue::language().0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_keep_their_names() {
        assert_eq!(translate("Sonando «Babydoll» de Dominic Fike.", "en"), "Playing “Babydoll” by Dominic Fike.");
        assert_eq!(translate("Sonando «Babydoll» de Dominic Fike.", "pt"), "Tocando «Babydoll» de Dominic Fike.");
        assert_eq!(translate("Sonando Soda Stereo.", "en"), "Playing Soda Stereo.");
        assert_eq!(translate("Sonando tu lista «Rock».", "en"), "Playing your playlist “Rock”.");
        assert_eq!(translate("Abriendo Bloc de notas.", "en"), "Opening Bloc de notas.");
        assert_eq!(translate("Spotify al 40 %.", "en"), "Spotify at 40%.");
    }

    #[test]
    fn a_phrase_inside_another_is_translated_too() {
        assert_eq!(
            translate("Spotify no está sonando. Volumen del equipo al 30 %.", "en"),
            "Spotify isn't playing. System volume at 30%."
        );
    }

    #[test]
    fn the_new_replies_too() {
        assert_eq!(translate("Ya estoy tomando apuntes.", "en"), "I'm already taking notes.");
        assert_eq!(translate("Listo, guardé 5 minutes de apuntes con su resumen en Documentos, Cocuyo, Apuntes.", "en"), "Done, I saved 5 minutes of notes with a summary in Documents, Cocuyo, Apuntes.");
        assert_eq!(translate("Listo, le mandé a Ana: «ya voy».", "pt"), "Pronto, mandei para Ana: «ya voy».");
        assert_eq!(translate("no encontré a Ana en WhatsApp", "en"), "I couldn't find Ana in WhatsApp");
        assert_eq!(
            translate("Enviar «ya voy» a Ana por WhatsApp
Ventana «Chats de prueba» · Ana. ¿Lo hago?", "en"),
            "Send “ya voy” to Ana on WhatsApp
Window “Chats de prueba” · Ana. Should I do it?"
        );
        assert_eq!(translate("Gemini se quedó callado.", "pt"), "O Gemini ficou calado.");
    }

    #[test]
    fn spanish_and_unknown_phrases_pass_through() {
        assert_eq!(translate("Sonando Soda Stereo.", "es"), "Sonando Soda Stereo.");
        assert_eq!(translate("Una frase que nadie tradujo.", "en"), "Una frase que nadie tradujo.");
        assert_eq!(translate("", "en"), "");
    }

    #[test]
    fn every_translation_has_the_same_holes() {
        for (spanish, english, portuguese) in REPLIES {
            let count = spanish.matches("{}").count();
            assert_eq!(english.matches("{}").count(), count, "{spanish}");
            assert_eq!(portuguese.matches("{}").count(), count, "{spanish}");
        }
    }
}
