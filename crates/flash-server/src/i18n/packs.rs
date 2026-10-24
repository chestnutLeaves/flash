//! Style packs: what a language's messages are held to beyond what every
//! language is (`lint`), as data, one pack per message folder. A language
//! is checked against the packs of every folder of its chain together:
//! Spain's Spanish against `es` and `es-ES`, Latin America's against `es`
//! and `es-419`. A folder with no pack has only the checks every language
//! has.

/// A word the messages use for the product's own object where readers
/// outside the product search for another (Spanish: a "tarjeta" inside,
/// "flashcards" where people look for Flash).
pub struct ProductWord {
    /// The start of the word kept for readers outside ("flashcard").
    pub outside: &'static str,
    /// The word inside, named in the problem.
    pub inside: &'static str,
}

/// What a deployment's own messages add to every pack. The open core's
/// add nothing (`Extras::NONE`).
#[derive(Clone, Copy)]
pub struct Extras {
    /// Ids that speak to readers outside the product (a public page, a
    /// page's title for search), where a product word's `outside` is
    /// right.
    pub outside_ids: &'static [&'static str],
    /// Names its messages share with their English in every language,
    /// beside each pack's `same_as_english`.
    pub same_as_english: &'static [&'static str],
    /// Products its messages name, which a translation names exactly as
    /// English does, beside the core's.
    pub names: &'static [&'static str],
    /// Starts of the ids of messages whose words come from a standard
    /// (CLDR's names): no translator answers for them, so they may read
    /// as English does and their length is their own.
    pub standard_ids: &'static [&'static str],
    /// Starts of the ids of a client's messages: its scripts word them,
    /// and it passes each one `$platform`, which a translation may use
    /// though English does not.
    pub client_ids: &'static [&'static str],
}

impl Extras {
    pub const NONE: Extras = Extras {
        outside_ids: &[],
        same_as_english: &[],
        names: &[],
        standard_ids: &[],
        client_ids: &[],
    };

    pub fn is_outside(&self, id: &str) -> bool {
        starts_with_any(self.outside_ids, id)
    }

    pub fn is_standard(&self, id: &str) -> bool {
        starts_with_any(self.standard_ids, id)
    }

    pub fn is_client(&self, id: &str) -> bool {
        starts_with_any(self.client_ids, id)
    }
}

fn starts_with_any(prefixes: &[&str], id: &str) -> bool {
    prefixes.iter().any(|p| id.starts_with(p))
}

/// What one message folder's text is held to.
pub struct Pack {
    pub folder: &'static str,
    /// Words its reader would not write: a whole word, or a word's start
    /// (`añad*` for añadir, añade, añadido). Messages quoting another
    /// product's own label (`ui-*`) are left alone.
    pub banned: &'static [&'static str],
    /// Phrases its reader would not write.
    pub banned_phrases: &'static [&'static str],
    /// Forms that write one gender over another ("o/a"), reworded for
    /// everyone instead; `at_gender` also refuses a letter on each side
    /// of an @ ("tod@s").
    pub gendered: &'static [&'static str],
    pub at_gender: bool,
    /// Questions and exclamations open with ¿ and ¡.
    pub inverted_marks: bool,
    /// An error never exclaims.
    pub calm_errors: bool,
    /// How a failure (`*-failed`) starts.
    pub failure_start: Option<&'static str>,
    pub product_word: Option<ProductWord>,
    /// Words (as `banned` writes them) right only where the product word's
    /// outside word is, in its `outside_ids`, each with the word the
    /// product says instead: the market's names for what Flash makes
    /// (French "fiches de révision", France's store name, for "cartes").
    pub outside_only: &'static [(&'static str, &'static str)],
    /// Words a message may share with its English: names, and words the
    /// language writes the same way.
    pub same_as_english: &'static [&'static str],
    /// English terms and the word this language's platforms ship for
    /// each ("appearance", "aspecto"): a message whose English has the
    /// term and whose translation lacks the word raises a warning.
    pub glossary: &'static [(&'static str, &'static str)],
    /// Characters its reader never gets, each with what to write instead
    /// (French: the straight apostrophe, for ’).
    pub banned_chars: &'static [(char, &'static str)],
    /// Marks a no-break space (U+00A0) always comes before, never a plain
    /// space or none (France's "?", "!", ";" and ":"). A mark followed by
    /// a letter, a digit, `/` or `=` is part of an address or a time, not
    /// punctuation.
    pub nbsp_before: &'static [char],
    /// Marks no space comes before (Quebec's "?", "!" and ";"). A variant's
    /// pack holds the base's messages to it too, so a base message the
    /// variant reads with a space there needs the variant's own wording.
    pub tight_before: &'static [char],
    /// « and » hold a no-break space inside.
    pub nbsp_in_guillemets: bool,
}

impl Pack {
    const fn empty(folder: &'static str) -> Pack {
        Pack {
            folder,
            banned: &[],
            banned_phrases: &[],
            gendered: &[],
            at_gender: false,
            inverted_marks: false,
            calm_errors: false,
            failure_start: None,
            product_word: None,
            outside_only: &[],
            same_as_english: &[],
            glossary: &[],
            banned_chars: &[],
            nbsp_before: &[],
            tight_before: &[],
            nbsp_in_guillemets: false,
        }
    }
}

/// Every pack. A folder named twice, or one no language reads, fails a
/// test.
pub const PACKS: &[Pack] = &[
    // Spanish, both variants: the reader is tú, a card is a tarjeta, a
    // deck a mazo, due cards are pendientes.
    // ("revisar" stays allowed: checking cards over is revisar;
    // studying them is repasar, which no rule can tell apart.)
    Pack {
        banned: &[
            "usted",
            "ficha",
            "fichas",
            "baraja",
            "barajas",
            "carta",
            "cartas",
            "programadas",
            // Apple and Google both ship "Aspecto".
            "apariencia",
            // The device is a teléfono on both sides of the Atlantic, as
            // both platforms write it.
            "móvil",
            "móviles",
        ],
        banned_phrases: &[
            "por favor",
            "repaso espaciado",
            // The device, as phrases: the adjective is fine ("ciclo
            // celular").
            "tu celular",
            "tus celulares",
            "el celular",
            "un celular",
            "del celular",
            "al celular",
            "los celulares",
            // Neither platform's error, in either variant.
            "algo salió mal",
            // Apple renamed it "Cuenta de Apple" in 2024.
            "apple id",
        ],
        gendered: &[
            "o/a",
            "os/as",
            "as/os",
            "a/o",
            "tú mismo",
            "tú misma",
            "ti mismo",
            "ti misma",
        ],
        at_gender: true,
        inverted_marks: true,
        calm_errors: true,
        // Lower case where the message ends a sentence the page starts.
        failure_start: Some("no se"),
        product_word: Some(ProductWord {
            outside: "flashcard",
            inside: "tarjeta",
        }),
        same_as_english: &[
            // Keys as Latin American keyboards label them (Spain's say
            // Intro and Mayús, in its own file).
            "enter", "shift", "ctrl", "esc", "play", "store", "https", "http", "www", "video",
            "videos", "plan", "admin", "local", "general", "manual", "panel", "flash", "chatgpt",
            "claude", "grok", "chat", "gemini", "mistral", "anki", "quizlet", "google", "apple",
            "github", "youtube", "fsrs", "pdf", "mcp", "api", "url", "html", "json", "csv", "apkg",
            "total", "normal", "audio", "web", "app", "plus", "code", "email", "markdown", "latex",
            "cloze", "oauth", "android", "iphone", "ipad", "ios", "windows", "mac", "linux",
            "docker", "codex", "cursor", "openai", "gmail", "outlook", "safari", "chrome",
            "firefox", "edge", "brave", "error",
            // The path of a YouTube address (youtube.com/watch?v=), which an
            // example link keeps as it is.
            "watch",
        ],
        glossary: &[
            ("appearance", "aspecto"),
            ("apple account", "cuenta de apple"),
        ],
        ..Pack::empty("es")
    },
    // Spain's Spanish: Latin America's words.
    Pack {
        banned: &[
            "agreg*",
            "computador*",
            "video",
            "videos",
            "costo",
            "costos",
            "presion*",
            // "Enter" a code or a password; "ingresos" (revenue) is
            // Spain's too.
            "ingresa",
            "ingresar",
            "ingresá",
            "ustedes",
        ],
        // Spain's errors are in the perfect, as Apple's and Android's
        // Spain strings are.
        banned_phrases: &["no se pudo", "no se pudieron", "se produjo un error"],
        ..Pack::empty("es-ES")
    },
    // Latin America's Spanish: Spain's words.
    Pack {
        banned: &[
            "añad*",
            "ordenador*",
            "vídeo*",
            "coste",
            "costes",
            "puls*",
            "vosotros",
            "vuestro*",
            "vuestra*",
            "coger",
            "coge",
            "cogí",
            "ajustes",
            "gestionar",
            "gestiona",
            "introduce",
            "introducir",
            "comprobar",
            "comprueba",
        ],
        // Latin America's errors are in the simple past, and it tries
        // again with "Vuelve a intentarlo", as Android's es-419 does.
        banned_phrases: &[
            "no se ha podido",
            "no se han podido",
            "se ha producido un error",
            "inténtalo de nuevo",
        ],
        ..Pack::empty("es-419")
    },
    // Portuguese, both variants: `pt` holds Brazil's words and Portugal
    // overrides where it words a message otherwise. A card is a cartão in
    // both, a deck a baralho; the reader is never "tu".
    Pack {
        // The card is a cartão in both variants, and "carta" stays
        // allowed (Portugal's driving licence is one). Neither variant
        // addresses an adult as tu. A streak is a sequência; bury is
        // ocultar, as both of Anki's translations mostly say.
        banned: &[
            "ficha", "fichas", "tu", "teu", "teus", "tua", "tuas", "contigo", "streak", "streaks",
            "enterr*",
        ],
        banned_phrases: &[
            "por favor",
            "revisão espaçada",
            // Apple renamed it Conta Apple in 2024.
            "apple id",
            "id apple",
        ],
        gendered: &[
            "o(a)",
            "os(as)",
            "a(o)",
            "as(os)",
            "o/a",
            "os/as",
            "a/o",
            "ele(a)",
            // "Boas-vindas" welcomes everyone.
            "bem-vindo",
            "bem-vinda",
        ],
        at_gender: true,
        calm_errors: true,
        // Lower case where the message ends a sentence the page starts.
        failure_start: Some("não foi possível"),
        product_word: Some(ProductWord {
            outside: "flashcard",
            inside: "cartão",
        }),
        same_as_english: &[
            // Keys as both countries' keyboards label them.
            "enter", "shift", "ctrl", "esc", "play", "store", "https", "http", "www", "admin",
            "local", "manual", "total", "normal", "flash", "chatgpt", "claude", "grok", "chat",
            "gemini", "mistral", "anki", "quizlet", "google", "apple", "github", "youtube", "fsrs",
            "pdf", "pdfs", "mcp", "api", "url", "html", "json", "csv", "apkg", "web", "app",
            "plus", "code", "email", "markdown", "latex", "cloze", "oauth", "android", "iphone",
            "ipad", "ios", "windows", "mac", "linux", "docker", "codex", "cursor", "openai",
            "gmail", "outlook", "safari", "chrome", "firefox", "edge", "brave", "feedback",
            "backup", "link", "links", "login", "online", "plugin", "plugins",
            // A theme's colour, the loan both countries' shops write
            // ("pardo" reads as a census category in Brazil).
            "taupe",
            // The path of a YouTube address (youtube.com/watch?v=), which an
            // example link keeps as it is.
            "watch",
        ],
        glossary: &[
            ("apple account", "conta apple"),
            ("spaced repetition", "repetição espaçada"),
        ],
        ..Pack::empty("pt")
    },
    // Portugal's Portuguese: Brazil's words and forms, where both of
    // Portugal's platforms write another. A base message holding one needs Portugal's wording.
    Pack {
        banned: &[
            // Third person with no pronoun: never você written out.
            "você",
            "vocês",
            "tela",
            "telas",
            // A file; the archive is arquivar, arquivado.
            "arquivo",
            "arquivos",
            "usuári*",
            "senha",
            "senhas",
            "salvar",
            "salve",
            "salvando",
            "salvou",
            "compartilh*",
            "configurações",
            "baixar",
            "baixe",
            "baixando",
            "baixado",
            "baixados",
            "baixada",
            "baixadas",
            // Delete is eliminar.
            "excluir",
            "exclua",
            "excluído",
            "excluída",
            "excluídos",
            "excluídas",
            "cadastr*",
            "aplicativo*",
            "gerenci*",
            "pressione",
            "pressionar",
            "acesse",
            "acessar",
            "upload",
            "uploads",
            "conexão",
            "conexões",
            "conectar",
            "conecte",
            "conectado",
            "conectada",
            "conectados",
            "conectadas",
            "conectando",
            // Settings' Appearance is Aspeto.
            "aparência",
            "anotações",
            "mídia",
            "mídias",
            // Review is rever ("revisão" stays, both variants' noun).
            "revisar",
            "revise",
            "revisa",
            "revisam",
            "revisou",
            "revisado",
            "revisada",
            "revisados",
            "revisadas",
            "revisando",
            // Progress is "a" and the infinitive ("A carregar…").
            "carregando",
            "sincronizando",
            "gerando",
            "processando",
            "importando",
            "exportando",
            "enviando",
            "verificando",
            "pesquisando",
            "buscando",
            "aguardando",
            "criando",
            "escrevendo",
            "ouvindo",
            "pensando",
            "lendo",
            "falando",
            "estudando",
            "confirmando",
            "iniciando",
            // Brazil's spellings after the 1990 Agreement.
            "contat*",
            "seção",
            "seções",
            "recepção",
            "concepção",
            "equipe",
            "equipes",
            "registr*",
            "aspecto",
            "aspectos",
            "detect*",
            "econômic*",
            "acadêmic*",
            "gênero",
            "gêneros",
        ],
        banned_phrases: &[
            "algo deu errado",
            "fazer login",
            "faça login",
            "de fato",
            // The device, as phrases: the adjective is Portugal's too
            // ("ciclo celular").
            "seu celular",
            "seus celulares",
            "do celular",
            "no celular",
            "um celular",
            "pelo celular",
            "ao celular",
        ],
        ..Pack::empty("pt-PT")
    },
    // Brazil's Portuguese: Portugal's words, forms and spellings.
    Pack {
        banned: &[
            "ecrã",
            "ecrãs",
            "ficheiro*",
            "telemóve*",
            "utilizador*",
            "registo*",
            "regista*",
            "partilh*",
            "definições",
            // Save is salvar; keeping is manter.
            "guardar",
            "guarde",
            "guardado",
            "guardada",
            "guardados",
            "guardadas",
            "descarreg*",
            "hiperliga*",
            "subscriç*",
            "gerir",
            "aceda",
            "aceder",
            "introduza",
            "apontamento*",
            "multimédia",
            // Portugal's spellings after the 1990 Agreement.
            "contacto",
            "contactos",
            "contactar",
            "contacte",
            "secç*",
            "receção",
            "conceção",
            "equipa",
            "equipas",
            "facto",
            "factos",
            "aspeto",
            "aspetos",
            "detetar",
            "deteção",
            "detetado",
            "detetada",
            "económic*",
            "académic*",
            "género",
            "géneros",
        ],
        banned_phrases: &[
            "palavra-passe",
            "palavras-passe",
            "chave-passe",
            "chaves-passe",
            "algo correu mal",
            "volte a tentar",
            // Progress is the gerund ("Carregando…").
            "a carregar…",
            "a guardar…",
            "a sincronizar…",
            "a gerar…",
            "a processar…",
            "a enviar…",
            "a pesquisar…",
            "a verificar…",
        ],
        ..Pack::empty("pt-BR")
    },
    // French, both variants: `fr` holds France's words and Quebec
    // overrides where it words a message otherwise. A card is a carte
    // and a deck a paquet in both; the reader is vous, never tu, on
    // screens and in mail.
    Pack {
        banned: &[
            // Never tu: its pronouns and possessives. "ton" (a tone) goes
            // with them; a tone is a "tonalité".
            "tu",
            "te",
            "toi",
            "ton",
            "ta",
            "tes",
            // "Vas-y": the one tu imperative no other person shares.
            "vas",
            // Both platforms write "e-mail" in France and "courriel" in
            // Quebec; never the bare English.
            "email",
            "emails",
            "svp",
            // The adjective agrees with the reader; "Bienvenue", the
            // noun, welcomes everyone.
            "bienvenu",
            "bienvenus",
            // A streak is a série, as Duolingo and Quizlet say.
            "streak",
            "streaks",
            // Bury is enfouir, as Anki's core says 26 times.
            "enterr*",
        ],
        banned_phrases: &[
            "s\u{2019}il vous plaît",
            // Mozilla's rule: an ellipsis already says it is going on.
            "en cours\u{2026}",
            "...",
            "révision espacée",
            // SD cards, in search and in both stores.
            "carte mémoire",
            "cartes mémoire",
            // A study session is a séance; "session" stays for signing in.
            "session de révision",
            "session d\u{2019}étude",
            "session terminée",
            // Apple renamed it Compte Apple in 2024.
            "apple id",
            "identifiant apple",
        ],
        gendered: &[
            "(e)",
            "(es)",
            "(ne)",
            "\u{b7}e",
            "\u{b7}ne",
            "\u{b7}rice",
            ".e.s",
            // Agree with the reader; ask about the action instead
            // ("Voulez-vous vraiment…", "Confirmez-vous").
            "êtes-vous sûr",
            "vous êtes connecté",
            "heureux de vous",
            "ravi de vous",
        ],
        at_gender: true,
        calm_errors: true,
        // "Impossible de" and "Impossible d’", lower case where the
        // message ends a sentence the page starts.
        failure_start: Some("impossible d"),
        product_word: Some(ProductWord {
            outside: "flashcard",
            inside: "carte",
        }),
        // France searches and buys "fiches de révision" (its store name),
        // but a fiche on a screen reads as a revision sheet: the card is a
        // carte. Anki's users search "deck anki": the product says paquet.
        outside_only: &[
            ("fiche", "carte"),
            ("fiches", "cartes"),
            ("deck", "paquet"),
            ("decks", "paquets"),
        ],
        same_as_english: &[
            "ctrl",
            "play",
            "store",
            "https",
            "http",
            "www",
            "admin",
            "local",
            "total",
            "normal",
            "flash",
            "chatgpt",
            "claude",
            "grok",
            "chat",
            "gemini",
            "mistral",
            "anki",
            "quizlet",
            "google",
            "apple",
            "github",
            "youtube",
            "fsrs",
            "pdf",
            "pdfs",
            "mcp",
            "api",
            "url",
            "html",
            "json",
            "csv",
            "apkg",
            "web",
            "app",
            "plus",
            "code",
            "markdown",
            "latex",
            "cloze",
            "oauth",
            "android",
            "iphone",
            "ipad",
            "ios",
            "windows",
            "mac",
            "linux",
            "docker",
            "codex",
            "cursor",
            "openai",
            "gmail",
            "outlook",
            "safari",
            "chrome",
            "firefox",
            "edge",
            "brave",
            "plugin",
            "plugins",
            // French words spelled as English ones.
            "pause",
            "stop",
            "options",
            "notifications",
            "message",
            "messages",
            "description",
            "date",
            "type",
            "format",
            "image",
            "images",
            "photo",
            "photos",
            "document",
            "documents",
            "audio",
            "version",
            "question",
            "questions",
            "collection",
            "source",
            "sources",
            "transcription",
            "confirmation",
            "navigation",
            "minute",
            "minutes",
            "page",
            "pages",
            "note",
            "notes",
            "service",
            "instructions",
            "simple",
            "test",
            "tests",
            "taupe",
            "menu",
            "standard",
            "guides",
            // Quebec's word for phone apps, as Apple writes it in Canada.
            "apps",
            "conversation",
            "conversations",
            "open",
            // The path of a YouTube address (youtube.com/watch?v=), which an
            // example link keeps as it is.
            "watch",
        ],
        glossary: &[
            ("apple account", "compte apple"),
            ("spaced repetition", "répétition espacée"),
            ("appearance", "apparence"),
        ],
        banned_chars: &[
            ('\'', "the apostrophe \u{2019}"),
            (
                '\u{202f}',
                "a no-break space (U+00A0), as Apple and Android ship",
            ),
            ('\u{2009}', "a no-break space (U+00A0); a thin space breaks"),
            ('"', "\u{ab} \u{bb} with a no-break space inside"),
            ('\u{201c}', "\u{ab} \u{bb} with a no-break space inside"),
            ('\u{201d}', "\u{ab} \u{bb} with a no-break space inside"),
        ],
        // Both variants: before a colon, and inside « ».
        nbsp_before: &[':'],
        nbsp_in_guillemets: true,
        ..Pack::empty("fr")
    },
    // France's French: Quebec's words, and the no-break space before "?",
    // "!" and ";" that Apple, Android and Firefox ship in France.
    Pack {
        banned: &[
            "courriel*",
            "clavard*",
            "plugiciel*",
            "téléver*",
            "nip",
            "bogue",
            "bogues",
            // "Entrez" a code is Quebec's; France's platforms write
            // "Saisissez".
            "entrez",
            "infonuagique",
            "magasiner",
        ],
        banned_phrases: &[
            "fin de semaine",
            "fins de semaine",
            "communiquez avec",
            "communiquer avec",
            // Quebec's phone, as phrases: the adjective is France's too
            // ("cycle cellulaire", "données cellulaires"), which is why
            // "le cellulaire" is not one ("le cycle cellulaire").
            "votre cellulaire",
            "un cellulaire",
            "du cellulaire",
            "au cellulaire",
            "sur cellulaire",
        ],
        nbsp_before: &['?', '!', ';'],
        ..Pack::empty("fr-FR")
    },
    // Quebec's French: France's words, and no space before "?", "!" and
    // ";" (the OQLF's choice, and what Apple, Android and Google ship in
    // Quebec). Every base message with one of them needs Quebec's wording.
    Pack {
        banned: &[
            "mail",
            "mails",
            // The device is a téléphone in both variants; France's
            // "portable" is not Quebec's word for it.
            "portable",
            "portables",
            "smartphone*",
            "saisissez",
            "contactez",
            "contacter",
            "bug",
            "bugs",
            "cookie",
            "cookies",
        ],
        banned_phrases: &["e-mail", "week-end", "code pin", "codes pin"],
        tight_before: &['?', '!', ';'],
        ..Pack::empty("fr-CA")
    },
];
