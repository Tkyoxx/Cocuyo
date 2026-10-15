// el cerebro. puras tablas de palabras y como suenan, cero IA aqui

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Change {
    By(i8),
    Set(u8),
    Mute,
    Unmute,
}

#[derive(Debug, PartialEq)]
pub enum Intent {
    OpenApp(String),
    OpenUrl(String),
    YouTube(String),
    Spotify(String),
    Google(String),
    Volume { change: Change, system: bool },
    Pause,
    Resume,
    Random,
    Mood(String),
    Playlist(String),
    Next,
    Previous,
    Restart,
    Screenshot,
    Lock,
    Settings,
    Record(bool),
    SaveReplay,
    Unknown,
}

const RECORD: &[&str] = &["graba", "grabar", "grabame", "grabes", "grabando", "grabacion", "grabe",
    "record", "recording", "grava", "gravar", "gravacao", "gravando", "grave",
];
const HALT: &[&str] = &["para", "parar", "pare", "deja", "dejar", "deten", "detener", "detenga", "termina", "terminar", "acaba", "acabar",
    "corta", "cortar", "apaga", "apagar", "frena", "finaliza", "finalizar", "stop", "end", "finish", "quit", "detem", "encerra", "encerrar",
];
const KEEP: &[&str] = &["guarda", "guardar", "guardame", "salva", "salvar", "save", "clip", "clipea", "clipear", "clipealo", "clipa"];
const LAST: &[&str] = &["ultimo", "ultimos", "ultima", "ultimas", "eso", "esto", "repeticion", "jugada", "jugadon", "clip", "last", "that", "replay", "isso", "jogada"];
const NOT_SCREEN: &[&str] = &["audio", "nota", "voz", "mensaje", "cancion", "note", "voice", "message", "mensagem"];

fn recording(tokens: &[&str]) -> Option<Intent> {
    let any = |family: &[&str]| tokens.iter().any(|t| family.contains(t));
    if any(KEEP) && (any(LAST) || tokens.len() == 1) && !any(NOT_SCREEN) {
        return Some(Intent::SaveReplay);
    }
    if !any(RECORD) || any(NOT_SCREEN) {
        return None;
    }
    Some(Intent::Record(!any(HALT)))
}

// Solo hace falta la forma base de cada verbo: los pronombres pegados ("ponme",

const PLAY: &[&str] = &[
    "pon", "pone", "poner", "ponga", "coloca", "colocar", "reproduce", "reproducir", "toca", "tocar", "escucha", "escuchar",
    "oir", "suena", "sonar", "dale", "echa", "tira", "manda", "play",
    "pun", "pond", "pawn", "put",
    "listen", "hear", "toque", "bota", "botar", "poe", "ouvir", "escutar", "escuta", "reproduz", "reproduzir",
];
const WATCH: &[&str] = &["ver", "mira", "mirar", "veamos",
    "watch", "assistir", "assiste",
];
const SEARCH: &[&str] = &["busca", "buscar", "busque", "investiga", "googlea", "consulta", "averigua",
    "search", "find", "look", "procura", "procurar", "pesquisa", "pesquisar",
];
const OPEN: &[&str] = &[
    "abre", "abrir", "abra", "abri", "inicia", "iniciar", "lanza", "lanzar", "ejecuta", "ejecutar", "arranca", "entra",
    "entrar", "mete", "prende", "corre",
    "open", "launch", "start", "run", "executa", "executar",
];
const UP: &[&str] = &["sube", "subir", "suba", "subi", "aumenta", "aumentar", "incrementa", "alza", "alzar",
    "up", "raise", "increase", "louder", "aumente",
];
const DOWN: &[&str] = &["baja", "bajar", "baje", "disminuye", "reduce", "reducir", "merma",
    "down", "lower", "decrease", "quieter", "abaixa", "abaixar", "abaixe", "diminui", "diminuir", "baixa", "baixar",
];
const MUTE: &[&str] = &["silencia", "silenciar", "silencio", "mutea", "mutear", "mute", "calla", "callar", "enmudece",
    "silence", "shut", "muta", "mutar", "cala",
];
const UNMUTE: &[&str] = &["desmutea", "desmutear", "desilencia", "desilenciar",
    "unmute", "desmuta", "desmutar",
];
const PAUSE: &[&str] = &["pausa", "pausar", "pause", "para", "parar", "pare", "deten", "detener", "detenga", "stop", "frena",
    "halt",
];
const RESUME: &[&str] = &["continua", "continuar", "continue", "reanuda", "reanudar", "sigue", "seguir", "siga", "resume", "despausa", "despausar",
    "unpause", "retoma", "retomar", "segue",
];
const NEXT: &[&str] = &[
    "siguiente", "proxima", "proximo", "salta", "saltar", "salte", "skip", "adelanta", "adelantar", "cambia", "cambiar",
    "cambie", "pasa", "pasar", "pase", "otra", "viene", "next",
    "forward", "seguinte", "pula", "pular", "avanca", "avancar", "passa", "passar", "outra", "another",
];
const PREVIOUS: &[&str] = &["anterior", "atras", "regresa", "regresar", "devuelve", "devolver", "vuelve", "volver", "previa", "previo", "retrocede",
    "previous", "back", "last", "rewind", "volta", "voltar", "volte",
];
const REPEAT: &[&str] = &["repite", "repetir", "repita", "reinicia", "reiniciar",
    "repeat", "restart", "replay", "again", "repete", "denovo",
];
const PLAYLIST: &[&str] = &["lista", "listas", "playlist", "playlists"];
const SHOT: &[&str] = &["captura", "capturar", "pantallazo", "screenshot", "foto",
    "capture", "snapshot", "print",
];
const LOCK: &[&str] = &["bloquea", "bloquear", "bloqueo",
    "lock", "bloqueia", "trava", "travar",
];
const SETTINGS: &[&str] = &["ajustes", "configuracion", "opciones", "preferencias",
    "settings", "preferences", "options", "configuracoes", "configuracao", "opcoes",
];
const QUIT: &[&str] = &["quita", "quitar", "quite", "apaga", "apagar",
    "desliga", "desligar",
];
const DEGREE: &[&str] = &["mas", "menos",
    "more", "less", "mais",
];
const VOLUME: &[&str] = &["volumen", "sonido", "audio",
    "volume", "sound", "som",
];
const MUSIC: &[&str] = &["musica", "cancion", "canciones", "tema", "temas", "rola", "rolas",
    "music", "song", "songs", "track", "tracks", "musicas", "cancao", "cancoes", "faixa", "faixas",
];
const RANDOM: &[&str] = &["random", "aleatoria", "aleatorio", "aleatorias", "azar", "cualquiera", "cualquier", "sorprendeme", "sorpresa",
    "shuffle", "whatever", "anything", "something", "surprise", "qualquer", "surpreenda",
];

const LOUDER: &[&str] = &["duro", "alto", "alta", "fuerte", "recio", "arriba",
    "up", "loud", "louder", "forte",
];
const QUIETER: &[&str] = &["bajo", "baja", "bajito", "bajita", "suave", "pasito", "quedito", "despacio", "abajo",
    "down", "quiet", "quieter", "soft", "softer", "baixo", "baixinho",
];
const A_LITTLE: &[&str] = &["poco", "poquito", "poquitico", "tantito", "pelo", "pelin", "rayita", "apenas",
    "little", "bit", "slightly", "pouco", "pouquinho",
];
const A_LOT: &[&str] = &["mucho", "bastante", "harto", "burda", "monton",
    "lot", "much", "way", "muito",
];
const SYSTEM: &[&str] = &[
    "pc", "pece", "computadora", "computador", "compu", "equipo", "sistema", "windows", "general", "laptop", "maquina",
    "ordenador",
    "computer", "system", "computador", "notebook",
];
const FILLER: &[&str] = &[
    "cancion", "canciones", "musica", "tema", "temas", "rola", "rolas", "cosa", "algo", "una", "un", "la", "el", "de", "al",
    "del", "las", "los", "mi", "mis", "me", "alguna", "algun", "a", "ya", "ahora", "eso", "esa", "ese", "esto", "esta",
    "este", "lo", "le", "que", "sea", "en", "con", "spotify", "play",
    "the", "an", "some", "song", "songs", "music", "track", "on", "in", "to", "it", "this", "that", "my", "of", "o", "os", "as", "um", "uma", "musicas", "do", "da", "dos", "das", "no", "na", "isso", "essa", "esse", "minha", "meu", "coisa", "alguma", "algum",
];
const GLUE: &[&str] = &["hasta", "por", "ciento", "porciento", "muy", "bien", "y", "nivel", "puntos", "rayitas", "tan",
    "to", "at", "percent", "para", "cento", "until", "ate", "em",
];

const SERVICES: &[(&str, &[&str])] = &[
    ("spotify", &["spotify", "spotifai", "spoti", "espoti"]),
    ("youtube", &["youtube", "yutub", "iutub", "yutu", "llutub"]),
    ("google", &["google", "gugel", "gugle", "gogle"]),
];

const GENRES: &[&str] = &[
    "rock", "pop", "salsa", "merengue", "bachata", "reggaeton", "regueton", "regeton", "regaeton", "trap", "rap", "hiphop", "jazz",
    "blues", "metal", "punk", "reggae", "cumbia", "vallenato", "rancheras", "corridos", "electronica", "techno", "house",
    "clasica", "instrumental", "lofi", "baladas", "boleros", "tango", "flamenco", "kpop", "country", "funk", "soul", "indie",
    "gaitas", "joropo", "llanera", "dembow", "champeta", "gospel", "cristiana", "anime", "disco", "dancehall", "phonk",
    "mariachi", "norteñas", "tropical", "urbano", "urbana", "relajante", "chill",
    "samba", "pagode", "sertanejo", "forro", "mpb", "bossa", "classical", "electronic", "latin",
];
const TRAITS: &[&str] = &[
    "suave", "tranquila", "tranquilo", "movida", "movido", "alegre", "triste", "tristes", "romantica", "romantico",
    "romanticas", "bailable", "vieja", "viejo", "viejita", "viejito", "nueva", "nuevo", "clasico", "clasicos", "clasicas",
    "latina", "latino", "español", "ingles", "venezolana", "venezolano", "colombiana", "mexicana", "argentino", "infantil",
    "navideña", "ochentas", "noventas", "ochenta", "noventa", "variada", "variado",
    "relaxing", "sad", "happy", "old", "new", "classic", "english", "spanish", "romantic", "soft", "upbeat", "calma", "antiga", "nova", "brasileira", "portugues",
];
const SMALL: &[&str] = &["en", "de", "del", "los", "las", "la", "el", "y", "mix", "exitos", "años", "lo", "mejor", "mas", "un", "poco", "hip", "hop", "lo", "fi", "k"];

const CONNECTIVES: &[&[&str]] = &[
    &["la", "que", "dice"], &["que", "dice"], &["que", "se", "llama"], &["llamada"], &["llamado"], &["del", "artista"],
    &["de", "la", "banda"], &["del", "grupo"], &["del", "cantante"], &["de", "la", "cantante"],
];

const SITES: &[(&[&str], &str)] = &[
    (&["youtube"], "https://www.youtube.com"),
    (&["gmail", "correo", "yimeil"], "https://mail.google.com"),
    (&["google"], "https://www.google.com"),
    (&["whatsapp", "wasap", "guasap", "watsap", "wasa"], "https://web.whatsapp.com"),
    (&["github", "guijab", "gitjab"], "https://github.com"),
    (&["netflix"], "https://www.netflix.com"),
    (&["twitter"], "https://x.com"),
    (&["facebook", "feisbuk", "face"], "https://www.facebook.com"),
    (&["instagram", "insta"], "https://www.instagram.com"),
    (&["twitch"], "https://www.twitch.tv"),
];

const LEAD_INS: &[&str] = &[
    "por favor", "porfa", "otra vez", "de nuevo", "la cancion de", "la cancion", "el tema de", "el tema", "la rola de", "la rola", "una cancion de",
    "una cancion", "un tema de", "una rola de", "otra cancion de", "otra de", "algo de", "musica de", "canciones de",
    "temas de", "lo mejor de", "lo ultimo de", "una de", "la de",
    "the song", "a song by", "songs by", "some music by", "music by", "something by", "some", "to", "the", "a musica", "uma musica de", "uma musica do", "uma musica da", "musicas de", "musicas do", "musicas da", "algo do", "algo da",
];

const ENCLITICS: &[&str] = &[
    "melos", "melas", "selos", "selas", "melo", "mela", "mele", "selo", "sela", "sele", "telo", "tela", "nos", "los", "las",
    "les", "me", "te", "le", "lo", "la", "se", "n",
];

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Play,
    Watch,
    Search,
    Open,
    Up,
    Down,
    Mute,
    Unmute,
    Pause,
    Resume,
    Next,
    Previous,
    Repeat,
    Shot,
    Lock,
    Settings,
    Quit,
    Degree,
    Volume,
    Music,
    Random,
}

const FAMILIES: &[(Kind, &[&str])] = &[
    (Kind::Play, PLAY),
    (Kind::Watch, WATCH),
    (Kind::Search, SEARCH),
    (Kind::Open, OPEN),
    (Kind::Up, UP),
    (Kind::Down, DOWN),
    (Kind::Mute, MUTE),
    (Kind::Unmute, UNMUTE),
    (Kind::Pause, PAUSE),
    (Kind::Resume, RESUME),
    (Kind::Next, NEXT),
    (Kind::Previous, PREVIOUS),
    (Kind::Repeat, REPEAT),
    (Kind::Shot, SHOT),
    (Kind::Lock, LOCK),
    (Kind::Settings, SETTINGS),
    (Kind::Quit, QUIT),
    (Kind::Degree, DEGREE),
    (Kind::Volume, VOLUME),
    (Kind::Music, MUSIC),
    (Kind::Random, RANDOM),
];

pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ç' => 'c',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        if c != ' ' || !out.ends_with(' ') {
            out.push(c);
        }
    }
    out.trim().to_string()
}

fn phonetic(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let vowel = |c: Option<&char>| matches!(c, Some('a' | 'e' | 'i' | 'o' | 'u'));
    let mut out = String::with_capacity(word.len());
    let mut i = usize::from(chars.len() > 3 && chars[0] == 'e' && chars[1] == 's' && !vowel(chars.get(2)));
    while i < chars.len() {
        let next = chars.get(i + 1).copied();
        let soft = matches!(next, Some('e' | 'i'));
        let mut skip = 1;
        let sound = match chars[i] {
            'h' => {
                i += 1;
                continue;
            }
            'c' if next == Some('h') => {
                skip = 2;
                'C'
            }
            'q' if next == Some('u') => {
                skip = 2;
                'k'
            }
            'l' if next == Some('l') => {
                skip = 2;
                'y'
            }
            'g' if next == Some('u') && matches!(chars.get(i + 2), Some('e' | 'i')) => {
                skip = 2;
                'g'
            }
            'c' if soft => 's',
            'c' | 'q' => 'k',
            'g' if soft => 'j',
            'z' => 's',
            'v' | 'w' => 'b',
            'y' if !vowel(chars.get(i + 1)) => 'i',
            'm' if !vowel(chars.get(i + 1)) => 'n',
            other => other,
        };
        if !out.ends_with(sound) {
            out.push(sound);
        }
        i += skip;
    }
    out
}

fn levenshtein(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut previous = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let current = row[j + 1];
            row[j + 1] = (previous + usize::from(ca != *cb)).min(row[j] + 1).min(current + 1);
            previous = current;
        }
    }
    row[b.len()]
}

fn alike(heard: &str, word: &str) -> bool {
    if heard == word {
        return true;
    }
    let slack = match heard.chars().count().min(word.chars().count()) {
        0..=4 => return false,
        5..=7 => 1,
        _ => 2,
    };
    levenshtein(heard, word) <= slack
}

pub(crate) fn close(heard: &str, word: &str) -> bool {
    alike(&phonetic(heard), &phonetic(word))
}

fn within(token: &str, family: &[&str]) -> bool {
    family.contains(&token) || {
        let heard = phonetic(token);
        family.iter().any(|word| alike(&heard, &phonetic(word)))
    }
}

fn has(tokens: &[&str], family: &[&str]) -> bool {
    tokens.iter().any(|token| within(token, family))
}

fn stems(token: &str) -> impl Iterator<Item = &str> {
    fn strip(word: &str) -> impl Iterator<Item = &str> {
        ENCLITICS.iter().filter_map(move |suffix| word.strip_suffix(suffix).filter(|stem| stem.len() >= 3))
    }
    strip(token).flat_map(|stem| std::iter::once(stem).chain(strip(stem)))
}

fn kind_of(token: &str) -> Option<Kind> {
    let exact = |word: &str| FAMILIES.iter().find(|(_, family)| family.contains(&word)).map(|(kind, _)| *kind);
    let sounds = |heard: &str| FAMILIES.iter().find(|(_, family)| family.iter().any(|word| alike(heard, &phonetic(word)))).map(|(kind, _)| *kind);
    exact(token)
        .or_else(|| sounds(&phonetic(token)))
        .or_else(|| stems(token).find_map(exact))
        .or_else(|| stems(token).find_map(|stem| sounds(&phonetic(stem))))
}

fn kind_of_clear(token: &str) -> Option<Kind> {
    let exact = |word: &str| FAMILIES.iter().find(|(_, family)| family.contains(&word)).map(|(kind, _)| *kind);
    exact(token).or_else(|| stems(token).find_map(exact))
}

fn first_action(tokens: &[&str]) -> Option<(usize, Kind)> {
    tokens.iter().enumerate().find_map(|(i, token)| {
        if *token == "ver" && i > 0 && tokens[i - 1] == "a" {
            return None;
        }
        Some((i, kind_of(token)?))
    })
}

fn service_of(token: &str) -> Option<&'static str> {
    SERVICES.iter().find(|(_, spellings)| within(token, spellings)).map(|(name, _)| *name)
}

fn site_of(token: &str) -> Option<&'static str> {
    SITES.iter().find(|(names, _)| within(token, names)).map(|(_, url)| *url)
}

fn words(text: &str) -> Vec<&str> {
    let mut tokens: Vec<&str> = Vec::new();
    const HESITATION: &[&str] = &["eh", "ehh", "em", "mm", "mmm", "hmm", "um", "uh"];
    for token in text.split(' ').filter(|t| !t.is_empty() && !HESITATION.contains(t)) {
        if token == "tube" && tokens.last() == Some(&"you") {
            tokens.pop();
            tokens.push("youtube");
        } else {
            tokens.push(service_of(token).unwrap_or(token));
        }
    }
    loop {
        match tokens.as_slice() {
            [.., "por", "favor"] => tokens.truncate(tokens.len() - 2),
            [.., "porfa" | "porfis" | "porfavor" | "gracias" | "favor" | "please" | "thanks" | "obrigado" | "obrigada"] => {
                tokens.truncate(tokens.len() - 1)
            }
            _ => return tokens,
        }
    }
}

fn number_in(tokens: &[&str]) -> Option<(u8, bool)> {
    const WORDS: &[(&str, u8)] = &[
        ("cero", 0), ("dos", 2), ("tres", 3), ("cuatro", 4), ("cinco", 5), ("seis", 6), ("siete", 7), ("ocho", 8),
        ("nueve", 9), ("diez", 10), ("quince", 15), ("veinte", 20), ("veinticinco", 25), ("treinta", 30), ("cuarenta", 40),
        ("cincuenta", 50), ("sesenta", 60), ("setenta", 70), ("ochenta", 80), ("noventa", 90), ("cien", 100),
        ("zero", 0), ("two", 2), ("three", 3), ("four", 4), ("five", 5), ("six", 6), ("seven", 7), ("eight", 8), ("nine", 9),
        ("ten", 10), ("fifteen", 15), ("twenty", 20), ("thirty", 30), ("forty", 40), ("fifty", 50), ("sixty", 60),
        ("seventy", 70), ("eighty", 80), ("ninety", 90), ("hundred", 100),
        ("dois", 2), ("quatro", 4), ("sete", 7), ("oito", 8), ("dez", 10), ("vinte", 20), ("trinta", 30), ("quarenta", 40),
        ("cinquenta", 50), ("sessenta", 60), ("oitenta", 80), ("cem", 100),
    ];
    const LEVELS: &[(&str, u8)] = &[
        ("mitad", 50), ("medio", 50), ("maximo", 100), ("tope", 100), ("full", 100), ("minimo", 5), ("half", 50), ("max", 100),
        ("maximum", 100), ("minimum", 5), ("metade", 50),
    ];
    let word = |token: &str| WORDS.iter().find(|(word, _)| *word == token).map(|(_, n)| *n);

    tokens.iter().enumerate().find_map(|(i, token)| {
        if let Some((_, level)) = LEVELS.iter().find(|(word, _)| word == token) {
            return Some((*level, true));
        }
        let mut number = token.parse::<u32>().ok().map(|n| n.min(100) as u8).or_else(|| word(token))?;
        let mut end = i + 1;
        if number >= 20 && number % 10 == 0 {
            let joined = matches!(tokens.get(i + 1), Some(&("y" | "e")));
            let at = i + 1 + usize::from(joined);
            if let Some(unit) = tokens.get(at).and_then(|t| word(t)).filter(|unit| *unit > 0 && *unit < 10) {
                number += unit;
                end = at + 1;
            }
        }
        let level = (i > 0 && matches!(tokens[i - 1], "a" | "al" | "en" | "hasta" | "to" | "at" | "para" | "pra" | "em" | "ate"))
            || matches!(tokens.get(end), Some(&("porciento" | "percent" | "porcento")))
            || matches!((tokens.get(end), tokens.get(end + 1)), (Some(&"por"), Some(&("ciento" | "cento"))));
        if !level && matches!(tokens.get(end), Some(&("rayita" | "rayitas" | "barrita" | "barritas" | "lineas" | "linea"))) {
            number = (number * 5).min(100);
        }
        Some((number, level))
    })
}

fn volume(sign: i8, tokens: &[&str], level: bool) -> Intent {
    let step = if has(tokens, A_LITTLE) {
        5
    } else if has(tokens, A_LOT) {
        25
    } else {
        10
    };
    let change = match number_in(tokens) {
        Some((number, named)) if named || level => Change::Set(number),
        Some((number, _)) => Change::By(sign * number as i8),
        None => Change::By(sign * step),
    };
    Intent::Volume { change, system: has(tokens, SYSTEM) }
}

fn volume_change(change: Change, tokens: &[&str]) -> Intent {
    Intent::Volume { change, system: has(tokens, SYSTEM) }
}

fn sign_of(tokens: &[&str]) -> i8 {
    let quiet = tokens.iter().any(|t| QUIETER.contains(t) || within(t, DOWN));
    if quiet != tokens.iter().any(|t| matches!(*t, "menos" | "less")) {
        -1
    } else {
        1
    }
}

fn volume_talk(rest: &[&str], phrase: &[&str]) -> Option<Intent> {
    let loudness = |t: &&str| LOUDER.contains(t) || QUIETER.contains(t);
    let talk = |t: &&str| {
        FILLER.contains(t)
            || GLUE.contains(t)
            || DEGREE.contains(t)
            || loudness(t)
            || number_in(&[*t]).is_some()
            || [VOLUME, MUTE, SYSTEM, A_LITTLE, A_LOT].iter().any(|family| within(t, family))
    };
    if rest.is_empty() || !rest.iter().all(talk) {
        return None;
    }
    if has(rest, MUTE) {
        return Some(volume_change(Change::Mute, phrase));
    }
    let about = has(rest, VOLUME);
    let degree = rest.iter().any(|t| DEGREE.contains(t)) && rest.iter().any(loudness);
    let bare = rest.iter().any(loudness) && !rest.iter().any(|t| matches!(*t, "algo" | "cosa" | "una" | "un" | "alguna" | "algun"));
    (about || degree || bare).then(|| volume(sign_of(rest), phrase, about))
}

fn take_service(tokens: &mut Vec<&str>) -> Option<&'static str> {
    let index = tokens.iter().rposition(|token| SERVICES.iter().any(|(name, _)| name == token))?;
    let name = service_of(tokens.remove(index))?;
    if index > 0 && matches!(tokens[index - 1], "en" | "por" | "desde" | "con" | "de" | "del" | "al" | "on" | "in" | "from" | "no" | "na" | "pelo") {
        tokens.remove(index - 1);
    }
    Some(name)
}

fn strip_lead_in(tokens: &[&str]) -> String {
    let mut kept: Vec<&str> = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        match CONNECTIVES.iter().find(|words| tokens[i..].starts_with(words)) {
            Some(words) => i += words.len(),
            None => {
                kept.push(tokens[i]);
                i += 1;
            }
        }
    }
    let text = kept.join(" ");
    let mut rest = text.as_str();
    while let Some(shorter) = LEAD_INS.iter().find_map(|lead| rest.strip_prefix(lead).filter(|rest| rest.is_empty() || rest.starts_with(' '))) {
        rest = shorter.trim_start();
    }
    rest.strip_prefix("de ").or_else(|| rest.strip_prefix("a ")).unwrap_or(rest).to_string()
}

fn mood(tokens: &[&str]) -> bool {
    let start = tokens
        .iter()
        .position(|t| !matches!(*t, "musica" | "canciones" | "temas" | "rolas" | "algo" | "un" | "poco" | "de" | "music" | "songs" | "some" | "something" | "musicas" | "um"))
        .unwrap_or(tokens.len());
    let rest = &tokens[start..];
    let kind = |t: &&str| GENRES.contains(t) || TRAITS.contains(t) || within(t, GENRES);
    let plain = |t: &&str| kind(t) || SMALL.contains(t) || t.parse::<u32>().is_ok();
    match rest.iter().position(|t| matches!(*t, "para" | "for" | "pra")) {
        Some(at) => at + 1 < rest.len() && (at > 0 || start > 0) && rest[..at].iter().all(plain),
        None => (rest.iter().any(kind) || rest.windows(2).any(|pair| GENRES.contains(&pair.concat().as_str()))) && rest.iter().all(plain),
    }
}

fn again(tokens: &[&str]) -> bool {
    tokens.iter().any(|t| matches!(*t, "again" | "denovo")) || tokens.windows(2).any(|pair| {
        matches!(pair, ["otra", "vez"] | ["de", "nuevo"] | ["de", "novo"] | ["once", "more"] | [_, "principio" | "inicio" | "comienzo" | "beginning" | "comeco"])
    })
}

fn play(mut rest: Vec<&str>, phrase: &[&str]) -> Intent {
    if rest.first().is_some_and(|first| *first != "play" && (PLAY.contains(first) || stems(first).any(|stem| PLAY.contains(&stem)))) {
        return play(rest[1..].to_vec(), phrase);
    }
    let meaning = |t: &&str| !FILLER.contains(t);
    if again(&rest) && rest.iter().all(|t| !meaning(t) || matches!(*t, "otra" | "vez" | "nuevo" | "novo" | "desde" | "from" | "principio" | "inicio" | "comienzo" | "beginning" | "comeco" | "once" | "more" | "again" | "denovo")) {
        return Intent::Restart;
    }
    if rest.iter().any(|t| close(t, "pausa") || *t == "stop") && rest.iter().filter(|t| meaning(t)).count() == 1 {
        return Intent::Pause;
    }
    if let Some(volume) = volume_talk(&rest, phrase) {
        return volume;
    }
    if rest.iter().any(|t| SHOT.contains(t)) && rest.iter().filter(|t| meaning(t)).count() == 1 {
        return Intent::Screenshot;
    }
    let service = take_service(&mut rest);
    let is = |t: &&str, family: &[&str]| family.contains(t);
    if rest.iter().all(|t| !meaning(t) || within(t, RANDOM) || is(t, NEXT) || is(t, PREVIOUS)) {
        if has(&rest, RANDOM) || matches!(rest.as_slice(), ["algo"] | ["lo", "que", "sea"] | ["alguma", "coisa"] | ["something"]) {
            return Intent::Random;
        }
        if rest.iter().any(|t| is(t, NEXT)) {
            return Intent::Next;
        }
        if rest.iter().any(|t| is(t, PREVIOUS)) {
            return Intent::Previous;
        }
        if service == Some("youtube") {
            return Intent::OpenUrl("https://www.youtube.com".into());
        }
        return Intent::Resume;
    }
    if let Some(at) = rest.iter().position(|t| within(t, PLAYLIST)).filter(|_| service != Some("youtube")) {
        let name: Vec<&str> = rest[at + 1..].iter().copied().skip_while(|t| matches!(*t, "de" | "del" | "reproduccion" | "llamada" | "que" | "se" | "llama")).collect();
        return if name.is_empty() { Intent::Random } else { Intent::Playlist(name.join(" ")) };
    }
    let query = strip_lead_in(&rest);
    match service {
        _ if query.is_empty() => Intent::Resume,
        Some("youtube") => Intent::YouTube(query),
        Some("google") => Intent::Google(query),
        _ if mood(&rest) => Intent::Mood(rest.join(" ")),
        _ => Intent::Spotify(query),
    }
}

fn search(mut rest: Vec<&str>, watching: bool) -> Intent {
    let service = take_service(&mut rest);
    let start = rest.iter().position(|t| !matches!(*t, "me" | "por" | "favor" | "porfa" | "for" | "up" | "please")).unwrap_or(rest.len());
    let rest = &rest[start..];
    if rest.is_empty() {
        return Intent::Unknown;
    }
    let song = matches!(rest, [first, ..] | ["la" | "una" | "el" | "un", first, ..] if MUSIC.contains(first));
    match service {
        Some("youtube") => Intent::YouTube(rest.join(" ")),
        Some("spotify") => Intent::Spotify(strip_lead_in(rest)),
        Some(_) => Intent::Google(rest.join(" ")),
        None if watching => Intent::YouTube(rest.join(" ")),
        None if song => Intent::Spotify(strip_lead_in(rest)),
        None => Intent::Google(rest.join(" ")),
    }
}

fn open(rest: &[&str]) -> Intent {
    const NOISE: &[&str] = &[
        "a", "al", "el", "la", "los", "las", "mi", "en", "app", "aplicacion", "programa", "juego", "pagina", "de", "me", "te",
        "the", "my", "application", "program", "game", "up", "o", "os", "as", "meu", "minha", "aplicativo", "jogo",
    ];
    let start = rest.iter().position(|t| !NOISE.contains(t)).unwrap_or(rest.len());
    match &rest[start..] {
        [] => Intent::Unknown,
        [one] if within(one, SETTINGS) => Intent::Settings,
        [one] if *one != "spotify" && site_of(one).is_some() => Intent::OpenUrl(site_of(one).unwrap_or_default().into()),
        target => Intent::OpenApp(target.join(" ")),
    }
}

pub fn parse(spoken: &str) -> Intent {
    let text = normalize(&spoken.replace('%', " porciento "));
    interpret(&words(&text))
}

/// Una queja también es una orden: "está muy duro" pide bajar y "no se escucha",
fn complaint(tokens: &[&str]) -> Option<i8> {
    let acts = |t: &&str| matches!(kind_of(t), Some(Kind::Play | Kind::Up | Kind::Down | Kind::Quit | Kind::Open | Kind::Search | Kind::Watch));
    let hears = |t: &&str| matches!(*t, "escucha" | "escucho" | "escuchan" | "oye" | "oyen" | "oigo");
    if let Some(no) = tokens.iter().position(|t| *t == "no") {
        if tokens[no + 1..].iter().take(3).any(hears) && !tokens.iter().any(|t| matches!(kind_of(t), Some(Kind::Up | Kind::Down))) {
            return Some(1);
        }
    }
    let states = |i: usize| matches!(tokens[i], "esta" | "estan" | "suena" | "quedo") || (i > 0 && tokens[i - 1] == "se" && hears(&tokens[i]));
    let verb = (0..tokens.len()).find(|i| states(*i))?;
    let how = verb + 1 + tokens[verb + 1..].iter().position(|t| LOUDER.contains(t) || QUIETER.contains(t))?;
    if tokens[..verb].iter().any(acts) || tokens[how + 1..].iter().any(acts) {
        return None;
    }
    Some(if LOUDER.contains(&tokens[how]) { -1 } else { 1 })
}

fn interpret(tokens: &[&str]) -> Intent {
    if let Some(recorder) = recording(tokens) {
        return recorder;
    }
    if let Some(sign) = complaint(tokens) {
        return volume(sign, tokens, false);
    }
    let Some((at, kind)) = first_action(tokens) else {
        return match tokens {
            [one] => site_of(one).map_or(Intent::Unknown, |url| Intent::OpenUrl(url.into())),
            _ if again(tokens) => Intent::Restart,
            _ => Intent::Unknown,
        };
    };
    let phrase = tokens;
    let verb = tokens[at];
    let mut rest: Vec<&str> = tokens[at + 1..].to_vec();
    let only = |tokens: &[&str], also: &[&str]| tokens.iter().all(|t| FILLER.contains(t) || also.contains(t));
    if matches!(kind, Kind::Play | Kind::Search | Kind::Watch | Kind::Music) {
        if let Some(service) = tokens[..at].iter().find(|t| SERVICES.iter().any(|(name, _)| name == *t)) {
            rest.extend(["en", *service]);
        }
    }

    match kind {
        Kind::Play
            if matches!(verb, "escucha" | "dale" | "oir")
                && matches!(
                    rest.first().and_then(|t| kind_of_clear(t)),
                    Some(Kind::Play | Kind::Up | Kind::Down | Kind::Mute | Kind::Unmute | Kind::Open | Kind::Search | Kind::Resume | Kind::Quit)
                ) =>
        {
            interpret(&tokens[at + 1..])
        }
        Kind::Play => play(rest, phrase),
        Kind::Music => play([&tokens[at..at + 1], &rest[..]].concat(), phrase),
        Kind::Watch => search(rest, true),
        Kind::Search => search(rest, false),
        Kind::Open => open(&rest),
        Kind::Up | Kind::Down if rest.contains(&"brillo") => Intent::Unknown,
        Kind::Up => volume(1, phrase, false),
        Kind::Down => volume(-1, phrase, false),
        Kind::Mute => volume_change(Change::Mute, phrase),
        Kind::Unmute => volume_change(Change::Unmute, phrase),
        Kind::Quit if has(&rest, MUTE) => volume_change(Change::Unmute, phrase),
        Kind::Quit if rest.contains(&"volumen") => volume(-1, phrase, false),
        Kind::Quit if has(&rest, VOLUME) => volume_change(Change::Mute, phrase),
        Kind::Quit if has(&rest, SYSTEM) => Intent::Unknown,
        Kind::Quit => Intent::Pause,
        Kind::Pause if verb == "para" && !only(&rest, PAUSE) => Intent::Unknown,
        Kind::Pause => Intent::Pause,
        Kind::Resume if at > 0 && tokens[at - 1] == "que" => Intent::Next,
        Kind::Resume => Intent::Resume,
        Kind::Next if verb == "otra" && rest.first() == Some(&"vez") => Intent::Restart,
        Kind::Next if matches!(verb, "otra" | "outra" | "another") && !only(&rest, &[]) => play([&["otra"], &rest[..]].concat(), phrase),
        Kind::Next if matches!(verb, "cambia" | "cambiar" | "cambie") && matches!(rest.first(), Some(&("a" | "por"))) => play(rest[1..].to_vec(), phrase),
        Kind::Next if verb == "pasa" && !only(&rest, NEXT) => Intent::Unknown,
        Kind::Next => Intent::Next,
        Kind::Previous => Intent::Previous,
        Kind::Repeat if has(&rest, SYSTEM) => Intent::Unknown,
        Kind::Repeat => Intent::Restart,
        Kind::Shot => Intent::Screenshot,
        Kind::Lock => Intent::Lock,
        Kind::Settings => Intent::Settings,
        Kind::Degree if has(&rest, VOLUME) || rest.iter().any(|t| LOUDER.contains(t) || QUIETER.contains(t)) => {
            volume(sign_of(&tokens[at..]), phrase, false)
        }
        Kind::Degree => Intent::Unknown,
        Kind::Volume if number_in(&rest).is_some() => volume(1, phrase, true),
        Kind::Volume if has(&rest, UP) || has(&rest, DOWN) || rest.iter().any(|t| DEGREE.contains(t) || LOUDER.contains(t) || QUIETER.contains(t)) => {
            volume(sign_of(&rest), phrase, true)
        }
        Kind::Volume => Intent::Unknown,
        Kind::Random => Intent::Random,
    }
}

fn sounds_of(text: &str) -> Vec<String> {
    const EMPTY: &[&str] = &["de", "del", "la", "el", "los", "las", "y", "the", "of", "for", "by", "a", "o", "do", "da", "e", "and"];
    text.split(' ').filter(|w| !w.is_empty() && !EMPTY.contains(w)).map(phonetic).collect()
}

pub fn covers(spoken: &str, text: &str) -> f32 {
    let (said, there) = (sounds_of(&normalize(spoken)), sounds_of(&normalize(text)));
    if said.is_empty() {
        return 0.0;
    }
    let joined = |words: &[String], i: usize| words.get(i + 1).map(|next| format!("{}{next}", words[i]));
    let mut found = vec![false; said.len()];
    for i in 0..said.len() {
        if there.iter().any(|other| alike(&said[i], other)) {
            found[i] = true;
        }
        if joined(&said, i).is_some_and(|pair| there.iter().any(|other| alike(&pair, other))) {
            found[i] = true;
            found[i + 1] = true;
        }
        if (0..there.len()).any(|j| joined(&there, j).is_some_and(|pair| alike(&said[i], &pair))) {
            found[i] = true;
        }
    }
    found.iter().filter(|hit| **hit).count() as f32 / said.len() as f32
}

pub fn similarity(spoken: &str, name: &str) -> f32 {
    let (a, b) = (normalize(spoken), normalize(name));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }
    let (said, named) = (sounds_of(&a), sounds_of(&b));
    let inside = |part: &[String], whole: &[String]| !part.is_empty() && part.iter().all(|w| whole.iter().any(|n| alike(w, n)));
    if inside(&said, &named) {
        return 0.9 - 0.02 * named.len().saturating_sub(said.len()) as f32;
    }
    if inside(&named, &said) {
        return 0.85 - 0.03 * said.len().saturating_sub(named.len()) as f32;
    }
    let (pa, pb) = (phonetic(&a.replace(' ', "")), phonetic(&b.replace(' ', "")));
    1.0 - levenshtein(&pa, &pb) as f32 / pa.chars().count().max(pb.chars().count()) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spotify(change: Change) -> Intent {
        Intent::Volume { change, system: false }
    }

    fn song(query: &str) -> Intent {
        Intent::Spotify(query.into())
    }

    #[test]
    fn many_ways_to_pause_and_resume() {
        for phrase in [
            "Pausa", "pausa la música", "para la canción", "Detén eso", "ponle pausa", "quita la música", "stop", "páralo ya",
            "oye pausa por favor", "ponlo en pausa", "puedes pausar la música", "páusala", "deténgalo", "apaga la música",
        ] {
            assert_eq!(parse(phrase), Intent::Pause, "{phrase}");
        }
        for phrase in [
            "continúa", "sigue", "dale play", "pon música", "ponle", "reanuda la música", "música por favor", "despausa",
            "quiero escuchar música", "síguele", "dale",
        ] {
            assert_eq!(parse(phrase), Intent::Resume, "{phrase}");
        }
    }

    #[test]
    fn many_ways_to_skip() {
        for phrase in [
            "siguiente", "la siguiente por favor", "pasa la canción", "cámbiala", "otra canción", "salta esta", "pon la siguiente",
            "next", "adelanta", "pásala", "sáltatela", "cambia de canción", "pon otra", "pasa a la siguiente",
        ] {
            assert_eq!(parse(phrase), Intent::Next, "{phrase}");
        }
        for phrase in ["anterior", "la anterior", "regresa la canción", "vuelve atrás", "pon la anterior", "devuélvela", "canción anterior"] {
            assert_eq!(parse(phrase), Intent::Previous, "{phrase}");
        }
    }

    #[test]
    fn volume_goes_to_spotify_unless_the_pc_is_named() {
        for phrase in [
            "sube el volumen", "súbele", "más duro", "más volumen", "súbele a la música", "auméntale", "ponle más volumen",
            "volumen arriba", "súbanle", "subí el volumen", "ponlo más duro", "puedes subirle",
        ] {
            assert_eq!(parse(phrase), spotify(Change::By(10)), "{phrase}");
        }
        for phrase in [
            "baja el volumen", "bájale", "más bajito", "menos volumen", "más suave", "volumen abajo", "baja la música",
            "ponlo más bajito", "ponlo bajito", "quítale volumen", "bájamele",
        ] {
            assert_eq!(parse(phrase), spotify(Change::By(-10)), "{phrase}");
        }
        assert_eq!(parse("sube el volumen de la pc"), Intent::Volume { change: Change::By(10), system: true });
        assert_eq!(parse("bájale al volumen del equipo"), Intent::Volume { change: Change::By(-10), system: true });
    }

    #[test]
    fn how_much_volume() {
        assert_eq!(parse("bájale un poco"), spotify(Change::By(-5)));
        assert_eq!(parse("súbele un poquito"), spotify(Change::By(5)));
        assert_eq!(parse("súbele bastante"), spotify(Change::By(25)));
        assert_eq!(parse("un poco más bajo"), spotify(Change::By(-5)));
        assert_eq!(parse("súbele 20"), spotify(Change::By(20)));
        assert_eq!(parse("volumen al 50"), spotify(Change::Set(50)));
        assert_eq!(parse("pon el volumen en 30"), spotify(Change::Set(30)));
        assert_eq!(parse("sube el volumen a ochenta"), spotify(Change::Set(80)));
        assert_eq!(parse("bájale a 20"), spotify(Change::Set(20)));
        assert_eq!(parse("volumen a la mitad"), spotify(Change::Set(50)));
        assert_eq!(parse("sube el volumen al 75%"), spotify(Change::Set(75)));
        assert_eq!(parse("volumen setenta y cinco"), spotify(Change::Set(75)));
        assert_eq!(parse("súbele al máximo"), spotify(Change::Set(100)));
    }

    #[test]
    fn silence() {
        for phrase in ["silencia", "silencio", "mutea la música", "cállate", "ponlo en silencio", "quita el sonido"] {
            assert_eq!(parse(phrase), spotify(Change::Mute), "{phrase}");
        }
        assert_eq!(parse("quita el silencio"), spotify(Change::Unmute));
        assert_eq!(parse("desmutea"), spotify(Change::Unmute));
    }

    #[test]
    fn songs_however_they_are_asked() {
        for phrase in [
            "pon soda stereo", "ponme algo de Soda Stereo", "quiero escuchar música de soda stereo", "reproduce soda stereo en spotify",
            "oye, colócame canciones de soda stereo por favor", "música de soda stereo", "pónmela de soda stereo",
            "en spotify pon soda stereo", "otra de soda stereo", "cambia a soda stereo", "pongan soda stereo", "escucha, pon soda stereo",
            "pon por favor soda stereo", "tócate una de soda stereo",
        ] {
            assert_eq!(parse(phrase), song("soda stereo"), "{phrase}");
        }
        assert_eq!(parse("pon la canción de música ligera"), song("musica ligera"));
        assert_eq!(parse("Pon De Música Ligera"), song("musica ligera"));
        assert_eq!(parse("ponme la de bad bunny que dice tití me preguntó"), song("bad bunny titi me pregunto"));
        assert_eq!(parse("pon persiana americana del grupo soda stereo"), song("persiana americana soda stereo"));
        assert_eq!(parse("pon pop smoke"), song("pop smoke"));
        assert_eq!(parse("pon para ti"), song("para ti"));
        assert_eq!(parse("pon despacito en youtube"), Intent::YouTube("despacito".into()));
        assert_eq!(parse("en YouTube pon despacito"), Intent::YouTube("despacito".into()));
    }

    #[test]
    fn searching() {
        assert_eq!(parse("busca recetas de arepas en YouTube"), Intent::YouTube("recetas de arepas".into()));
        assert_eq!(parse("busca el clima de mañana"), Intent::Google("el clima de mañana".into()));
        assert_eq!(parse("búscame el clima de mañana por favor"), Intent::Google("el clima de mañana".into()));
        assert_eq!(parse("quiero ver videos de gatos"), Intent::YouTube("videos de gatos".into()));
        assert_eq!(parse("busca la canción de música ligera"), song("musica ligera"));
        assert_eq!(parse("a ver, pon música"), Intent::Resume);
    }

    #[test]
    fn something_random() {
        for phrase in [
            "pon una canción random", "pon algo", "sorpréndeme", "pon cualquier canción", "ponme una canción al azar",
            "pon música aleatoria", "pon cualquier cosa", "una canción cualquiera", "pon lo que sea", "algo random",
        ] {
            assert_eq!(parse(phrase), Intent::Random, "{phrase}");
        }
    }

    #[test]
    fn opening_things() {
        assert_eq!(parse("Abre el blog de notas."), Intent::OpenApp("blog de notas".into()));
        assert_eq!(parse("ábreme Spotify por favor"), Intent::OpenApp("spotify".into()));
        assert_eq!(parse("puedes abrir el espotifai"), Intent::OpenApp("spotify".into()));
        assert_eq!(parse("abre YouTube"), Intent::OpenUrl("https://www.youtube.com".into()));
        assert_eq!(parse("YouTube"), Intent::OpenUrl("https://www.youtube.com".into()));
        assert_eq!(parse("entra a whatsapp"), Intent::OpenUrl("https://web.whatsapp.com".into()));
        assert_eq!(parse("métete al guasap"), Intent::OpenUrl("https://web.whatsapp.com".into()));
        assert_eq!(parse("abre los ajustes"), Intent::Settings);
        assert_eq!(parse("inicia la aplicación de discord"), Intent::OpenApp("discord".into()));
        assert_eq!(parse("ábrete el google chrome"), Intent::OpenApp("google chrome".into()));
    }

    #[test]
    fn survives_what_the_transcriber_gets_wrong() {
        assert_eq!(parse("sube el bolumen"), spotify(Change::By(10)));
        assert_eq!(parse("Zube el volumen"), spotify(Change::By(10)));
        assert_eq!(parse("Pauza la musica"), Intent::Pause);
        assert_eq!(parse("ziguiente"), Intent::Next);
        assert_eq!(parse("la siguente"), Intent::Next);
        assert_eq!(parse("pon bad bunny en espotifai"), song("bad bunny"));
        assert_eq!(parse("pon bad bunny en Spotifi"), song("bad bunny"));
        assert_eq!(parse("busca gatos en you tube"), Intent::YouTube("gatos".into()));
        assert_eq!(parse("Reproduse soda stereo"), song("soda stereo"));
        assert_eq!(parse("Reproduze soda estereo"), song("soda estereo"));
        assert_eq!(parse("kontinua"), Intent::Resume);
        assert_eq!(parse("toma una captura de pantaya"), Intent::Screenshot);
        assert_eq!(parse("graba la pantalla"), Intent::Record(true));
        assert_eq!(parse("empieza a grabar"), Intent::Record(true));
        assert_eq!(parse("Para de grabar."), Intent::Record(false));
        assert_eq!(parse("deja de grabar la pantalla"), Intent::Record(false));
        assert_eq!(parse("detén la grabación"), Intent::Record(false));
        assert_eq!(parse("guarda lo último"), Intent::SaveReplay);
        assert_eq!(parse("guarda eso"), Intent::SaveReplay);
        assert_eq!(parse("clip"), Intent::SaveReplay);
        assert_eq!(parse("start recording"), Intent::Record(true));
        assert_eq!(parse("stop recording"), Intent::Record(false));
        assert_eq!(parse("clip that"), Intent::SaveReplay);
        assert_eq!(parse("pare de gravar"), Intent::Record(false));
        assert_eq!(parse("salva isso"), Intent::SaveReplay);
        assert_ne!(parse("graba una nota de voz"), Intent::Record(true));
        assert_eq!(parse("para"), Intent::Pause);
        assert_eq!(parse("Habre el bloc de notas"), Intent::OpenApp("bloc de notas".into()));
        assert_eq!(parse("Pun. Baby doll. The Dominic Fike."), song("baby doll the dominic fike"));
        assert_eq!(parse("Pun. Blinding lights. The weakened."), song("blinding lights the weakened"));
        assert_eq!(parse("Ponme. Shape of you. De Ed Sheeran."), song("shape of you de ed sheeran"));
    }

    #[test]
    fn complaints_are_orders_too() {
        for phrase in ["está muy duro", "la música está muy alta", "suena demasiado fuerte", "se escucha muy duro"] {
            assert_eq!(parse(phrase), spotify(Change::By(-10)), "{phrase}");
        }
        for phrase in ["no se escucha", "no oigo nada", "está muy bajito", "la música está muy baja", "no se escucha, súbele", "se oye bajito"] {
            assert_eq!(parse(phrase), spotify(Change::By(10)), "{phrase}");
        }
        assert_eq!(parse("está muy duro, bájale un poco"), spotify(Change::By(-5)));
        assert_eq!(parse("ponlo muy bajito"), spotify(Change::By(-10)));
        assert_eq!(parse("pon esta más duro"), spotify(Change::By(10)));
    }

    #[test]
    fn genres_and_moods_are_not_songs() {
        let mood = |query: &str| Intent::Mood(query.into());
        assert_eq!(parse("pon rock"), mood("rock"));
        assert_eq!(parse("ponme algo de salsa"), mood("algo de salsa"));
        assert_eq!(parse("pon rock en español"), mood("rock en español"));
        assert_eq!(parse("quiero escuchar reguetón viejito"), mood("regueton viejito"));
        assert_eq!(parse("pon música para dormir"), mood("musica para dormir"));
        assert_eq!(parse("música para estudiar"), mood("musica para estudiar"));
        assert_eq!(parse("pon algo suave"), mood("algo suave"));
        assert_eq!(parse("pon música romántica"), mood("musica romantica"));
        assert_eq!(parse("pon hip hop"), mood("hip hop"));
        assert_eq!(parse("pon rock en youtube"), Intent::YouTube("rock".into()));
    }

    #[test]
    fn titles_and_artists_cover_what_was_said() {
        assert!(covers("persiana americana soda stereo", "Persiana Americana Soda Stereo") > 0.99);
        assert!(covers("bad bunny titi me pregunto", "Tití Me Preguntó Bad Bunny") > 0.99);
        assert!(covers("baby doll dominic fike", "Babydoll Dominic Fike") > 0.99);
        assert!(covers("babydoll", "Baby Doll") > 0.99);
        assert!(covers("persiana americana", "De Música Ligera Soda Stereo") < 0.1);
        assert!(covers("titi me pregunto", "BAILE INoLVIDABLE Bad Bunny") < 0.1);
    }

    #[test]
    fn lists_and_repeats() {
        assert_eq!(parse("pon mi lista de rock"), Intent::Playlist("rock".into()));
        assert_eq!(parse("reproduce la playlist para entrenar"), Intent::Playlist("para entrenar".into()));
        assert_eq!(parse("pon la lista de reproducción Favoritas"), Intent::Playlist("favoritas".into()));
        assert_eq!(parse("ponme la pleilist de salsa en spotify"), Intent::Playlist("salsa".into()));
        assert_eq!(parse("pon mi playlist"), Intent::Random);
        for phrase in ["repite la canción", "otra vez", "ponla de nuevo", "ponla otra vez", "repítela", "desde el principio", "reinicia la canción"] {
            assert_eq!(parse(phrase), Intent::Restart, "{phrase}");
        }
        assert_eq!(parse("pon otra vez soda stereo"), song("soda stereo"));
    }

    #[test]
    fn one_order_inside_another() {
        assert_eq!(parse("dale, súbele"), spotify(Change::By(10)));
        assert_eq!(parse("escucha, abre spotify"), Intent::OpenApp("spotify".into()));
        assert_eq!(parse("la que sigue"), Intent::Next);
        assert_eq!(parse("la que viene"), Intent::Next);
        assert_eq!(parse("cambia esa vaina"), Intent::Next);
        assert_eq!(parse("para, para"), Intent::Pause);
        assert_eq!(parse("Pon, eh, soda stereo."), song("soda stereo"));
        assert_eq!(parse("quiero oír a Héctor Lavoe"), song("hector lavoe"));
        assert_eq!(parse("Pon una canción ramdon."), Intent::Random);
        assert_eq!(parse("bájale dos rayitas"), spotify(Change::By(-10)));
    }

    #[test]
    fn orders_in_english() {
        assert_eq!(parse("Play Blinding Lights by The Weeknd"), song("blinding lights by the weeknd"));
        assert_eq!(parse("play some music by Soda Stereo"), song("soda stereo"));
        assert_eq!(parse("listen to the song Despacito"), song("despacito"));
        assert_eq!(parse("play something"), Intent::Random);
        assert_eq!(parse("play some rock"), Intent::Mood("some rock".into()));
        assert_eq!(parse("pause the music please"), Intent::Pause);
        assert_eq!(parse("stop"), Intent::Pause);
        assert_eq!(parse("resume"), Intent::Resume);
        assert_eq!(parse("next song"), Intent::Next);
        assert_eq!(parse("skip this one"), Intent::Next);
        assert_eq!(parse("go back to the previous song"), Intent::Previous);
        assert_eq!(parse("play it again"), Intent::Restart);
        assert_eq!(parse("turn the volume up"), spotify(Change::By(10)));
        assert_eq!(parse("turn it down a bit"), spotify(Change::By(-5)));
        assert_eq!(parse("louder"), spotify(Change::By(10)));
        assert_eq!(parse("set the volume to fifty percent"), spotify(Change::Set(50)));
        assert_eq!(parse("volume to seventy five"), spotify(Change::Set(75)));
        assert_eq!(parse("mute"), spotify(Change::Mute));
        assert_eq!(parse("open Notepad"), Intent::OpenApp("notepad".into()));
        assert_eq!(parse("open the settings"), Intent::Settings);
        assert_eq!(parse("search for cat videos on YouTube"), Intent::YouTube("cat videos".into()));
        assert_eq!(parse("search the weather tomorrow"), Intent::Google("the weather tomorrow".into()));
        assert_eq!(parse("take a screenshot"), Intent::Screenshot);
        assert_eq!(parse("lock the computer"), Intent::Lock);
    }

    #[test]
    fn orders_in_portuguese() {
        assert_eq!(parse("Toca Garota de Ipanema"), song("garota de ipanema"));
        assert_eq!(parse("coloca uma música do Tim Maia"), song("tim maia"));
        assert_eq!(parse("toca alguma coisa"), Intent::Random);
        assert_eq!(parse("toca qualquer coisa"), Intent::Random);
        assert_eq!(parse("toca samba"), Intent::Mood("samba".into()));
        assert_eq!(parse("pausa a música"), Intent::Pause);
        assert_eq!(parse("continua"), Intent::Resume);
        assert_eq!(parse("próxima música"), Intent::Next);
        assert_eq!(parse("pula essa"), Intent::Next);
        assert_eq!(parse("volta a anterior"), Intent::Previous);
        assert_eq!(parse("aumenta o volume"), spotify(Change::By(10)));
        assert_eq!(parse("abaixa um pouco"), spotify(Change::By(-5)));
        assert_eq!(parse("volume em cinquenta"), spotify(Change::Set(50)));
        assert_eq!(parse("abre o bloco de notas"), Intent::OpenApp("bloco de notas".into()));
        assert_eq!(parse("pesquisa receitas no YouTube"), Intent::YouTube("receitas".into()));
        assert_eq!(parse("tira um print"), Intent::Screenshot);
    }

    #[test]
    fn nothing_is_guessed() {
        for phrase in [
            "mañana llueve", "qué hora es", "hola cómo estás", "", "el perro de mi vecino", "necesito algo para la tos",
            "qué pasa hoy", "apaga la computadora", "volumen", "sube el brillo", "reinicia la pc", "no pares", "cierra spotify",
        ] {
            assert_eq!(parse(phrase), Intent::Unknown, "{phrase}");
        }
    }

    #[test]
    fn words_that_look_alike_stay_apart() {
        assert_eq!(parse("pasa"), Intent::Next);
        assert_eq!(parse("para"), Intent::Pause);
        assert_eq!(parse("pausa"), Intent::Pause);
        assert_eq!(parse("sigue"), Intent::Resume);
        assert_eq!(parse("sube"), spotify(Change::By(10)));
        assert_eq!(parse("baja"), spotify(Change::By(-10)));
    }

    #[test]
    fn sounds_alike() {
        assert_eq!(phonetic("volumen"), phonetic("bolumen"));
        assert_eq!(phonetic("pausa"), phonetic("pauza"));
        assert_eq!(phonetic("espotifi"), phonetic("spotify"));
        assert_eq!(phonetic("hacer"), phonetic("aser"));
        assert_eq!(phonetic("queso"), phonetic("keso"));
        assert_eq!(phonetic("guitarra"), "gitara");
        assert_eq!(phonetic("gente"), phonetic("jente"));
        assert_ne!(phonetic("chico"), phonetic("cico"));
    }

    #[test]
    fn app_names_survive_transcription_slips() {
        assert!(similarity("blog de notas", "Bloc de notas") > 0.85);
        assert!(similarity("opera", "Navegador Opera GX") > 0.8);
        assert!(similarity("espotifai", "Spotify") > 0.8);
        assert!(similarity("el spotify ese", "Spotify") > 0.72);
        assert!(similarity("exel", "Excel") > 0.72);
        assert!(similarity("spotify", "Calculadora") < 0.4);
        assert!(similarity("google chrome", "Google Drive") < 0.72, "media coincidencia no basta");
    }
}
