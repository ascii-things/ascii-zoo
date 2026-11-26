//! Tutto quello che è uguale su Windows, macOS e Linux: lo sfondo incorporato, le
//! specie, il menu dell'icona e cosa fare quando si sceglie una voce.

use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::Icon;

use crate::config::{
    autostart, data_italiana, leggi_data, pulisci_nome, pulisci_testo, Config, ETICHETTA_MAX, NOME_MAX, NOTA_MAX,
};
use crate::{dialog, tema};
use crate::nerd::{self, Nerd};
use crate::pomodoro::{Durate, Pomodoro, Stato};
use crate::riproduzione::Riproduzione;
use crate::stats::Stats;

pub static HTML: &str = include_str!("../assets/ascii-zoo-wallpaper.html");

/// indirizzo della pagina: wry su Windows trasforma "zoo://" in http://zoo.localhost/
#[cfg(windows)]
pub const URL: &str = "http://zoo.localhost/";
#[cfg(not(windows))]
pub const URL: &str = "zoo://localhost/";

/// gli animali di serie, nello stesso ordine della pagina: (id, nome nel menu)
pub const SPECIE: [(&str, &str); 11] = [
    ("farfalle", "Farfalle"),
    ("meduse", "Meduse"),
    ("libellule", "Libellule"),
    ("koi", "Koi"),
    ("rane", "Rane"),
    ("aironi", "Aironi"),
    ("granchi", "Granchi"),
    ("cavalli", "Cavalli"),
    ("anatre", "Anatre"),
    ("gatti", "Gatti"),
    ("topi", "Topi"),
];

/// i paesaggi di serie, dopo gli animali nella pagina: sfondi senza animali, gli stessi
/// dello screensaver (dreams/), con gli stessi id. (id, nome nel menu)
pub const PAESAGGI: [(&str, &str); 6] = [
    ("stelle", "Stelle"),
    ("pioggia", "Pioggia e lampi"),
    ("fuoco", "Falò"),
    ("aurora", "Aurora"),
    ("vita", "Gioco della vita"),
    ("mare", "Mare di notte"),
];

/// una voce del menu "Sfondo": un animale o un paesaggio
#[derive(Clone, Copy, Debug)]
pub struct Sfondo {
    pub id: &'static str,
    pub nome: &'static str,
    /// true = nel sottomenu "Paesaggi", false = in "Animali"
    pub paesaggio: bool,
}

/// tutto quello che c'è nel menu "Sfondo", nello stesso ordine in cui la pagina lo mette in
/// SPECIES (la posizione è quella che va in `animale` nelle impostazioni): gli animali di
/// serie, i paesaggi di serie e poi le scene dell'utente (cartella `scene`)
pub fn specie() -> &'static [Sfondo] {
    static TUTTE: std::sync::OnceLock<Vec<Sfondo>> = std::sync::OnceLock::new();
    TUTTE.get_or_init(|| {
        specie_di_serie()
            // un file con l'id di uno sfondo di serie ne prende il posto (e il nome)
            .map(|(id, nome, paesaggio)| match crate::scene::tutte().iter().find(|s| s.sostituisce && s.id == id) {
                Some(s) => Sfondo { id, nome: s.nome.as_str(), paesaggio },
                None => Sfondo { id, nome, paesaggio },
            })
            .chain(
                crate::scene::tutte()
                    .iter()
                    .filter(|s| !s.sostituisce)
                    .map(|s| Sfondo { id: s.id.as_str(), nome: s.nome.as_str(), paesaggio: s.paesaggio }),
            )
            .collect()
    })
}

/// gli sfondi di serie, nell'ordine della pagina: (id, nome, paesaggio)
pub fn specie_di_serie() -> impl Iterator<Item = (&'static str, &'static str, bool)> {
    SPECIE.iter().map(|&(id, nome)| (id, nome, false)).chain(PAESAGGI.iter().map(|&(id, nome)| (id, nome, true)))
}

/// true se l'id è di uno sfondo di serie (animale o paesaggio)
pub fn di_serie(id: &str) -> bool {
    specie_di_serie().any(|s| s.0 == id)
}

/// gli animali (con un nome da dare all'animale più grande): le posizioni di "Nomi degli animali"
fn con_nome() -> impl Iterator<Item = &'static Sfondo> {
    specie().iter().filter(|s| !s.paesaggio)
}

/// Mette in `sub` le voci `testa` (prima la prima, una linea, poi le altre) e poi due
/// sottomenu, "Animali" e "Paesaggi", con una voce per ogni sfondo. Restituisce tutte le
/// voci nell'ordine di `testa` e poi di `specie()`, che è quello delle posizioni: la voce
/// `scelta` ha la spunta. `spunta(i)` dice se lo sfondo i ha la spunta (per la rotazione).
fn voci_sfondo(sub: &Submenu, testa: &[&str], spunta: impl Fn(usize) -> bool) -> Vec<CheckMenuItem> {
    let mut voci = Vec::new();
    for (k, nome) in testa.iter().enumerate() {
        let v = CheckMenuItem::new(*nome, true, spunta(k), None);
        let _ = sub.append(&v);
        if k == 0 {
            let _ = sub.append(&PredefinedMenuItem::separator());
        }
        voci.push(v);
    }
    let animali = Submenu::new("Animali", true);
    let paesaggi = Submenu::new("Paesaggi", true);
    for (i, s) in specie().iter().enumerate() {
        let v = CheckMenuItem::new(s.nome, true, spunta(testa.len() + i), None);
        let _ = if s.paesaggio { paesaggi.append(&v) } else { animali.append(&v) };
        voci.push(v);
    }
    let _ = sub.append(&animali);
    let _ = sub.append(&paesaggi);
    voci
}
/// Mette in `sub` i sottomenu "Animali" e "Paesaggi" di "In rotazione", con una spunta per
/// sfondo; `dentro(id)`: se è in rotazione. Restituisce le voci nell'ordine di `specie()`.
fn voci_rotazione(sub: &Submenu, dentro: impl Fn(&str) -> bool) -> Vec<CheckMenuItem> {
    let animali = Submenu::new("Animali", true);
    let paesaggi = Submenu::new("Paesaggi", true);
    let voci = specie()
        .iter()
        .map(|s| {
            let v = CheckMenuItem::new(s.nome, true, dentro(s.id), None);
            let _ = if s.paesaggio { paesaggi.append(&v) } else { animali.append(&v) };
            v
        })
        .collect();
    let _ = sub.append(&animali);
    let _ = sub.append(&paesaggi);
    voci
}

/// gli sfondi senza la spunta, cioè quelli tolti dalla rotazione (voci di voci_rotazione)
fn escluse(voci: &[CheckMenuItem]) -> Vec<String> {
    specie().iter().zip(voci).filter(|(_, v)| !v.is_checked()).map(|(s, _)| s.id.to_string()).collect()
}

/// mette le spunte di "In rotazione" (voci di voci_rotazione) secondo `escluse`
fn segna_rotazione(voci: &[CheckMenuItem], escluse: &[String]) {
    for (s, v) in specie().iter().zip(voci) {
        v.set_checked(!escluse.iter().any(|e| e == s.id));
    }
}

/// con due o più schermi, il sottomenu per tutti e la prima voce di quelli di ogni schermo
const TUTTI_GLI_SCHERMI: &str = "Tutti gli schermi";
const COME_TUTTI: &str = "Come \"Tutti gli schermi\"";

/// "Sfondo" > uno schermo (con due o più schermi): cosa mostra e cosa fa girare
pub struct SfondoSchermo {
    id: String,
    /// "Come \"Tutti gli schermi\"", "A rotazione", poi uno per sfondo (ordine di specie())
    voci: Vec<CheckMenuItem>,
    /// "In rotazione" > "Come \"Tutti gli schermi\"": spuntata se lo schermo non ha una lista sua
    rot_come: CheckMenuItem,
    /// "In rotazione" > una spunta per sfondo (ordine di specie())
    rot: Vec<CheckMenuItem>,
}

/// Riempie il menu "Sfondo" per questi schermi. Con uno solo: "A rotazione", "Animali" e
/// "Paesaggi" (uno sfondo fisso) e "In rotazione" (quali girano) stanno lì. Con due o più
/// stanno in "Tutti gli schermi", e ogni schermo ha un sottomenu uguale, con in più "Come
/// \"Tutti gli schermi\"" (anche dentro "In rotazione"). In fondo la cartella delle scene.
fn riempi_sfondo(m: &mut Menus, cfg: &Config, schermi: &[Schermo]) {
    while m.sfondo_sub.remove_at(0).is_some() {}
    let tutti = (schermi.len() >= 2).then(|| Submenu::new(TUTTI_GLI_SCHERMI, true));
    let dove = tutti.as_ref().unwrap_or(&m.sfondo_sub);
    m.animali = voci_sfondo(dove, &["A rotazione"], |i| i as u32 == cfg.animale);
    let _ = dove.append(&PredefinedMenuItem::separator());
    let rot = Submenu::new("In rotazione", true);
    m.rot_specie = voci_rotazione(&rot, |id| !cfg.rotazione_escluse.iter().any(|e| e == id));
    let _ = dove.append(&rot);
    m.sfondo_schermi.clear();
    if let Some(tutti) = &tutti {
        let _ = m.sfondo_sub.append(tutti);
        for (n, s) in schermi.iter().enumerate() {
            let sub = Submenu::new(nome_schermo(n, s), true);
            let attuale = cfg.animale_schermi.get(&s.id).map(|a| *a as usize + 1).unwrap_or(0);
            let voci = voci_sfondo(&sub, &[COME_TUTTI, "A rotazione"], |k| k == attuale);
            let _ = sub.append(&PredefinedMenuItem::separator());
            let rot = Submenu::new("In rotazione", true);
            let proprie = cfg.rotazione_escluse_schermi.get(&s.id);
            let rot_come = CheckMenuItem::new(COME_TUTTI, true, proprie.is_none(), None);
            let _ = rot.append(&rot_come);
            let _ = rot.append(&PredefinedMenuItem::separator());
            let escluse = proprie.unwrap_or(&cfg.rotazione_escluse);
            let rot_voci = voci_rotazione(&rot, |id| !escluse.iter().any(|e| e == id));
            let _ = sub.append(&rot);
            let _ = m.sfondo_sub.append(&sub);
            m.sfondo_schermi.push(SfondoSchermo { id: s.id.clone(), voci, rot_come, rot: rot_voci });
        }
    }
    let _ = m.sfondo_sub.append(&PredefinedMenuItem::separator());
    let _ = m.sfondo_sub.append(&m.scene_cartella);
}

/// ogni quanto cambia animale "a rotazione"
pub const INTERVALLI: [(u32, &str); 7] = [
    (30, "Ogni 30 secondi"),
    (60, "Ogni minuto"),
    (120, "Ogni 2 minuti"),
    (300, "Ogni 5 minuti"),
    (600, "Ogni 10 minuti"),
    (1800, "Ogni 30 minuti"),
    (3600, "Ogni ora"),
];

/// dove stanno i widget: (valore per la pagina, nome nel menu)
pub const ANGOLI: [(&str, &str); 4] = [
    ("basso-destra", "In basso a destra"),
    ("basso-sinistra", "In basso a sinistra"),
    ("alto-destra", "In alto a destra"),
    ("alto-sinistra", "In alto a sinistra"),
];

/// il tema dello sfondo: (valore in Config, nome nel menu)
pub const TEMI: [(&str, &str); 3] = [("scuro", "Scuro"), ("chiaro", "Chiaro"), ("sistema", "Come il sistema")];
/// fotogrammi al secondo dello sfondo: (valore in Config, nome nel menu). Meno fotogrammi,
/// meno lavoro per CPU e GPU: conta sui portatili e con la scala frazionaria di GNOME, dove
/// lo sfondo si disegna più grande dello schermo
pub const FOTOGRAMMI: [(u32, &str); 3] = [(30, "30 (più fluido)"), (20, "20"), (15, "15 (più leggero)")];

/// aspetto dei widget: (valore per la pagina, nome nel menu)
pub const DIMENSIONI: [(&str, &str); 3] = [("piccoli", "Piccoli"), ("normali", "Normali"), ("grandi", "Grandi")];
pub const CORNICI: [(&str, &str); 3] = [
    ("ascii", "+-- ASCII --+"),
    ("linee", "┌── Linee ──┐"),
    ("arrotondata", "╭── Arrotondata ──╮"),
];

/// durate del pomodoro: (minuti di lavoro, di pausa, di pausa lunga, nome nel menu)
pub const POMODORI: [(u32, u32, u32, &str); 2] =
    [(25, 5, 15, "25 min di lavoro, 5 di pausa"), (50, 10, 30, "50 min di lavoro, 10 di pausa")];

/// ASCII_ZOO_LOG=1: scrive su stderr cosa succede (pause, schermi, clic), per le prove e la CI
pub fn log_attivo() -> bool {
    static ATTIVO: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ATTIVO.get_or_init(|| std::env::var_os("ASCII_ZOO_LOG").is_some_and(|v| !v.is_empty() && v != "0"))
}

/// quando è partito il programma (main): per i tempi dell'avvio nel log
static AVVIO: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
pub fn segna_avvio() {
    AVVIO.get_or_init(std::time::Instant::now);
}
/// millisecondi dall'avvio del programma
pub fn ms_dall_avvio() -> u128 {
    AVVIO.get_or_init(std::time::Instant::now).elapsed().as_millis()
}

/// una riga di log, solo con ASCII_ZOO_LOG=1
macro_rules! zoo_log {
    ($($a:tt)*) => {
        if $crate::shared::log_attivo() {
            eprintln!("ascii-zoo: {}", format_args!($($a)*));
        }
    };
}
pub(crate) use zoo_log;

/// Nel log (solo con ASCII_ZOO_LOG=1), a pagina caricata: quanti fotogrammi ha già disegnato
/// e da quanti millisecondi c'è la pagina, per vedere quanto ci mette lo sfondo a comparire
pub fn log_pagina_pronta(webview: &wry::WebView, schermo: &str) {
    if !log_attivo() {
        return;
    }
    let schermo = schermo.to_string();
    let quando = ms_dall_avvio();
    let _ = webview.evaluate_script_with_callback(
        "(function () { try { return [asciiZoo.frameNo, Math.round(performance.now()), asciiZoo.species.id, \
         asciiZoo.chiaro ? 'chiaro' : 'scuro', JSON.stringify(AsciiZoo.TEMPI)].join(' '); } catch (e) { return '?'; } })()",
        move |r| {
            let r = r.trim_matches('"').to_string();
            let mut p = r.split(' ');
            match (p.next(), p.next(), p.next(), p.next(), p.next()) {
                (Some(f), Some(t), Some(sfondo), Some(tema), tempi) => zoo_log!(
                    "avvio: schermo {schermo}: pagina caricata a {quando} ms dall'avvio; la pagina c'è da {t} ms, ha già \
                     disegnato {f} fotogrammi, mostra {sfondo}, tema {tema}; tempi nella pagina {}",
                    tempi.unwrap_or("?").replace("\\\"", "")
                ),
                _ => zoo_log!("avvio: schermo {schermo}: pagina caricata a {quando} ms dall'avvio ({r})"),
            }
        },
    );
}

pub const JS_STOP: &str = "window.asciiZoo && window.asciiZoo.stop()";
pub const JS_START: &str = "window.asciiZoo && window.asciiZoo.start()";

/// Regole per la pagina dentro l'app: niente rete e niente moduli verso fuori. Le scene
/// dell'utente sono codice scritto da altri e la pagina conosce la nota e le statistiche
/// del PC: così non possono mandarle da nessuna parte. Tutto quello che serve allo
/// sfondo (caratteri, atlante) è già nella pagina o fatto al volo (data:, blob:).
const CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
                   img-src data: blob:; font-src data:; connect-src 'none'; media-src 'none'; \
                   frame-src 'none'; worker-src 'none'; form-action 'none'; base-uri 'none'";

/// lo sfondo con le regole qui sopra e, prima del motore, le scene dell'utente
fn html_app() -> &'static str {
    static PAGINA: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    PAGINA.get_or_init(|| {
        let meta = format!("<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP}\">");
        let mut html = HTML.replacen("<meta charset=\"utf-8\">", &meta, 1);
        let scene = crate::scene::script();
        if let Some(i) = html.find("<script>") {
            html.insert_str(i, &scene);
        }
        html
    })
}

/// risposta della pagina: la richiesta del protocollo "zoo" restituisce sempre lo sfondo
pub fn pagina() -> wry::http::Response<std::borrow::Cow<'static, [u8]>> {
    wry::http::Response::builder()
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Content-Security-Policy", CSP)
        .body(std::borrow::Cow::Borrowed(html_app().as_bytes()))
        .unwrap()
}

/// la WebView resta sulla pagina dello sfondo: nessun link o script la porta altrove
pub fn navigazione_permessa(url: String) -> bool {
    url.starts_with(URL) || url.starts_with("zoo://")
}

/// `in_carica`: alimentatore collegato E c'è una batteria (un PC fisso non è mai "in carica",
/// se no con "Koi nello stagno in carica" le koi non lascerebbero mai il posto alla rotazione)
pub fn js_in_carica(in_carica: bool) -> String {
    format!("window.zooSetting && zooSetting('inCarica', {in_carica})")
}

/// valori di CPU, RAM e batteria per il widget "pc" e delle statistiche per nerd
pub fn js_statistiche(v: &serde_json::Value) -> String {
    format!("window.zooSetting && zooSetting('statistiche', {v})")
}

/// uno schermo collegato: id stabile (nome del dispositivo) e dimensioni
#[derive(Clone, PartialEq, Debug)]
pub struct Schermo {
    pub id: String,
    pub larghezza: u32,
    pub altezza: u32,
}

/// valori speciali di "Widget > Schermo"
pub const PRINCIPALE: &str = "";
pub const TUTTI: &str = "*";

/// "Schermo 1, principale (2560×1600)"
fn nome_schermo(n: usize, s: &Schermo) -> String {
    let principale = if n == 0 { ", principale" } else { "" };
    format!("Schermo {}{principale} ({}×{})", n + 1, s.larghezza, s.altezza)
}

pub struct Menus {
    pub animali: Vec<CheckMenuItem>,
    /// "Rotazione > Spenta": spuntata quando c'è un animale fisso
    pub rot_spenta: CheckMenuItem,
    pub rot_tempi: Vec<CheckMenuItem>,
    /// "In rotazione" (per tutti gli schermi), ordine di specie()
    pub rot_specie: Vec<CheckMenuItem>,
    pub nomi: Vec<MenuItem>,
    /// il menu "Sfondo" (vedi riempi_sfondo) e, con due o più schermi, i sottomenu degli schermi
    sfondo_sub: Submenu,
    sfondo_schermi: Vec<SfondoSchermo>,
    /// "Apri la cartella delle scene…" in fondo al menu "Sfondo"
    pub scene_cartella: MenuItem,
    pub nota: MenuItem,
    pub conto: MenuItem,
    pub statistiche: CheckMenuItem,
    /// "Statistiche per nerd": (id della voce, voce)
    pub nerd: Vec<(&'static str, CheckMenuItem)>,
    pub musica: CheckMenuItem,
    /// "Aspetto": dimensione, cornice, sfondo pieno
    pub dimensioni: Vec<CheckMenuItem>,
    pub cornici: Vec<CheckMenuItem>,
    pub sfondo_pieno: CheckMenuItem,
    pub angoli: Vec<CheckMenuItem>,
    pub temi: Vec<CheckMenuItem>,
    /// "Fotogrammi al secondo", stesso ordine di FOTOGRAMMI
    pub fotogrammi: Vec<CheckMenuItem>,
    /// "Pomodoro": il titolo (Windows, macOS) e la prima voce, non cliccabile, mostrano fase e
    /// minuti rimasti; poi Avvia/Pausa/Riprendi
    pub pomodoro_sub: Submenu,
    pub pomodoro_stato: MenuItem,
    pub pomodoro_avvia: MenuItem,
    pub pomodoro_salta: MenuItem,
    pub pomodoro_ferma: MenuItem,
    pub pomodoro_durate: Vec<CheckMenuItem>,
    pub pomodoro_lunga: CheckMenuItem,
    pub widget_sub: Submenu,
    /// "Widget > Schermo": (valore, voce); i primi due sono principale e tutti
    pub widget_schermi: Vec<(String, CheckMenuItem)>,
    widget_schermi_fissi: Vec<(String, CheckMenuItem)>,
    pub clic: CheckMenuItem,
    pub orologio: CheckMenuItem,
    pub koi: CheckMenuItem,
    pub batteria: CheckMenuItem,
    pub schermo: CheckMenuItem,
    pub coperto: CheckMenuItem,
    pub avvio: CheckMenuItem,
    /// "Usa come screensaver…" (Windows e macOS)
    pub salvaschermo: MenuItem,
    /// Windows e macOS, sottomenu "Screensaver": la scena (id, voce; "rotazione" = tutte) e
    /// "Prova adesso" (su Linux il sottomenu è in salvaschermo_linux.rs)
    #[cfg(any(windows, target_os = "macos"))]
    salva_scene: Vec<(&'static str, CheckMenuItem)>,
    #[cfg(any(windows, target_os = "macos"))]
    salva_prova: MenuItem,
    pub foto: MenuItem,
    pub ricarica: MenuItem,
    pub esci: MenuItem,
}

/// cosa deve fare il programma dopo una voce del menu
pub enum Esito {
    /// impostazione cambiata (e salvata): manda lo script alle pagine, se c'è, e ricontrolla le pause
    Cambiato(Option<String>),
    /// cambiato qualcosa che dipende dallo schermo: a ogni pagina va mandato js_schermo
    Schermi,
    Ricarica,
    /// installare lo screensaver e aprire le sue impostazioni di sistema
    Salvaschermo,
    /// salvare una foto di ogni schermo (vedi `foto::scatta`)
    Foto,
    /// "Rotazione > Spenta": chiedere alla pagina del primo schermo quale animale mostra
    /// (`Core::chiedi_animale`); la risposta la applica `Core::risposta_animale`
    FermaRotazione,
    Esci,
    Niente,
}

/// impostazioni + menu: la logica del menu, senza finestre
pub struct Core {
    pub cfg: Config,
    pub menus: Menus,
    /// gli schermi collegati, il principale per primo
    pub schermi: Vec<Schermo>,
    /// il tema mandato alle pagine: "scuro", "chiaro" o "auto" (sistema non riconosciuto:
    /// decide la pagina con prefers-color-scheme)
    tema_pagina: &'static str,
    /// le statistiche per nerd (sysinfo), lette solo quando servono
    nerd: Nerd,
    /// widget "In riproduzione": il thread che chiede ai lettori cosa suona
    musica: Riproduzione,
    /// il pomodoro, uno solo per tutti gli schermi
    pomodoro: Pomodoro,
    /// ultimo titolo del sottomenu "Pomodoro" (si cambia solo quando cambia)
    pomodoro_titolo: String,
    /// "Rotazione > Spenta": lo sfondo che la pagina dice di mostrare (1.., come `animale`), quando risponde
    animale_mostrato: std::sync::Arc<std::sync::Mutex<Option<u32>>>,
}

/// lo sfondo che la pagina mostra adesso: 1.. come `animale` nelle impostazioni (0 = non si sa)
const JS_ANIMALE: &str = "(function () { try { return window.AsciiZoo && window.asciiZoo && window.asciiZoo.species \
    ? AsciiZoo.SPECIES.indexOf(asciiZoo.species) + 1 : 0; } catch (e) { return 0; } })()";

impl Core {
    /// `solo_windows`: se mostrare la voce dello screensaver (Windows e macOS)
    pub fn new(solo_windows: bool) -> (Self, Menu) {
        let mut cfg = Config::load();
        // posizioni nel menu "Animale": un file di scena tolto dalla cartella può lasciarle
        // fuori lista, e allora si torna alla rotazione (solo in memoria finché non cambia altro)
        let voci = specie().len() as u32;
        if cfg.animale > voci {
            cfg.animale = 0;
        }
        cfg.animale_schermi.retain(|_, a| *a <= voci);
        let (menu, menus) = build_menu(&cfg, solo_windows);
        let pomodoro = Pomodoro::new(durate(&cfg));
        let mut core = Self {
            cfg,
            menus,
            schermi: Vec::new(),
            tema_pagina: "scuro",
            nerd: Nerd::default(),
            musica: Riproduzione::default(),
            pomodoro,
            pomodoro_titolo: String::new(),
            animale_mostrato: Default::default(),
        };
        core.tema_pagina = core.tema_ora();
        zoo_log!("tema: {} (alla pagina: {})", core.cfg.tema, core.tema_pagina);
        (core, menu)
    }

    /// Il tema da mandare alle pagine adesso. Con "Come il sistema" lo si chiede al sistema
    /// (registro di Windows, aspetto di macOS, portale o impostazioni di GNOME su Linux).
    fn tema_ora(&self) -> &'static str {
        match self.cfg.tema.as_str() {
            "chiaro" => "chiaro",
            "sistema" => match tema::scuro() {
                Some(true) => "scuro",
                Some(false) => "chiaro",
                None => "auto",
            },
            _ => "scuro",
        }
    }

    pub fn js_tema(&self) -> String {
        format!("window.zooSetting && zooSetting('tema', '{}')", self.tema_pagina)
    }

    /// Con "Come il sistema", da chiamare ogni pochi secondi: lo script per le pagine se il
    /// tema del sistema è cambiato dall'ultima volta, se no None.
    pub fn tema_cambiato(&mut self) -> Option<String> {
        if self.cfg.tema != "sistema" {
            return None;
        }
        let ora = self.tema_ora();
        if ora == self.tema_pagina {
            return None;
        }
        zoo_log!("tema del sistema: {ora}");
        self.tema_pagina = ora;
        Some(self.js_tema())
    }

    /// i nomi come oggetto JSON: serde_json mette gli escape, quindi apostrofi e
    /// virgolette nei nomi non rompono lo script
    pub fn js_nomi(&self) -> String {
        let nomi = serde_json::to_string(&self.cfg.nomi).unwrap_or_else(|_| "{}".into());
        format!("window.zooSetting && zooSetting('nomi', {nomi})")
    }

    pub fn js_fotogrammi(&self) -> String {
        format!("window.zooSetting && zooSetting('fps', {})", self.cfg.fotogrammi)
    }

    pub fn js_orologio(&self) -> String {
        format!("window.zooSetting && zooSetting('orologio', {})", self.cfg.orologio)
    }

    pub fn js_rotazione(&self, schermo: &str) -> String {
        let escluse = self.escluse_di(schermo);
        let specie: Vec<&str> = specie().iter().map(|s| s.id).filter(|id| !escluse.iter().any(|e| e == id)).collect();
        let rot = serde_json::json!({ "secondi": self.cfg.rotazione_secondi, "specie": specie });
        format!("window.zooSetting && zooSetting('rotazione', {rot})")
    }

    /// i widget si vedono su questo schermo?
    fn widget_qui(&self, schermo: &str) -> bool {
        let scelta = self.cfg.widget_schermo.as_str();
        if scelta == TUTTI {
            return true;
        }
        // schermo scelto non collegato (o "principale"): vale il principale
        let scelta_c = self.schermi.iter().any(|s| s.id == scelta);
        if scelta_c {
            scelta == schermo
        } else {
            self.schermi.first().is_none_or(|s| s.id == schermo)
        }
    }

    /// i widget per uno schermo (spenti dove non devono vedersi)
    pub fn js_widget(&self, schermo: &str) -> String {
        let c = &self.cfg;
        let qui = self.widget_qui(schermo);
        let conto = (qui && !c.conto_data.is_empty())
            .then(|| serde_json::json!({ "data": c.conto_data, "etichetta": c.conto_etichetta }));
        let nerd: Vec<&str> =
            if qui { c.widget_nerd.iter().map(String::as_str).filter(|id| nerd::disponibile(id)).collect() } else { Vec::new() };
        let w = serde_json::json!({
            "qui": qui,
            "nota": if qui { c.nota.as_str() } else { "" },
            "conto": conto,
            "statistiche": qui && c.widget_statistiche,
            "nerd": nerd,
            "musica": qui && c.widget_musica,
            "angolo": c.widget_angolo,
            "dimensione": c.widget_dimensione,
            "cornice": c.widget_cornice,
            "sfondo": if c.widget_sfondo_pieno { "pieno" } else { "trasparente" },
        });
        format!("window.zooSetting && zooSetting('widget', {w})")
    }

    /// Il giro periodico dei widget (ogni ~2 secondi, dal controllo delle pause di ogni
    /// piattaforma): restituisce gli script da mandare a tutte le pagine. `visibile`: c'è
    /// almeno uno schermo non in pausa; se no non si legge niente.
    pub fn giro(&mut self, stats: &mut Stats, visibile: bool) -> Vec<String> {
        let mut js = Vec::new();
        let c = &self.cfg;
        let nerd_su = c.widget_nerd.iter().any(|id| nerd::disponibile(id));
        if visibile && (c.widget_statistiche || nerd_su) {
            let mut v = serde_json::Map::new();
            if c.widget_statistiche {
                if let serde_json::Value::Object(m) = stats.leggi() {
                    v = m;
                }
            }
            self.nerd.leggi(&c.widget_nerd, &mut v);
            js.push(js_statistiche(&v.into()));
        } else if !nerd_su {
            // spente: sysinfo libera quello che aveva
            self.nerd.leggi(&[], &mut serde_json::Map::new());
        }
        js.extend(self.musica.giro(self.cfg.widget_musica, visibile));
        // il pomodoro va avanti anche con lo sfondo in pausa: il tempo passa lo stesso
        if self.pomodoro.controlla(adesso_ms()) {
            js.push(self.js_pomodoro());
        }
        self.aggiorna_menu_pomodoro();
        js
    }

    /// zooSetting('pomodoro', …): a tutte le pagine; lo mostrano quelle con i widget
    pub fn js_pomodoro(&self) -> String {
        format!("window.zooSetting && zooSetting('pomodoro', {})", self.pomodoro.json())
    }

    /// Testi delle voci del pomodoro, aggiornati al minuto: "Pomodoro: lavoro, 23 min" nel
    /// titolo del sottomenu, "Lavoro: ancora 23 min" nella prima voce (su Linux il titolo di
    /// un sottomenu non si può cambiare: muda mette l'etichetta dentro un contenitore GTK) e
    /// Avvia, Pausa o Riprendi nella seconda.
    fn aggiorna_menu_pomodoro(&mut self) {
        let p = &self.pomodoro;
        let minuti = p.rimasto(adesso_ms()).map(|ms| ms.div_ceil(60_000));
        let fase = p.fase.nome();
        let (titolo, stato, voce) = match (p.stato, minuti) {
            (Stato::InCorso { .. }, Some(m)) => {
                (format!("Pomodoro: {fase}, {m} min"), format!("{}: ancora {m} min", maiuscola(fase)), "Pausa")
            }
            (Stato::InPausa { .. }, Some(m)) => (
                format!("Pomodoro: {fase} in pausa, {m} min"),
                format!("{} in pausa: mancano {m} min", maiuscola(fase)),
                "Riprendi",
            ),
            _ => ("Pomodoro".to_string(), "Fermo".to_string(), "Avvia"),
        };
        if titolo == self.pomodoro_titolo {
            return;
        }
        let m = &self.menus;
        m.pomodoro_sub.set_text(&titolo);
        m.pomodoro_stato.set_text(&stato);
        m.pomodoro_avvia.set_text(voce);
        let attivo = p.stato != Stato::Fermo;
        m.pomodoro_salta.set_enabled(attivo);
        m.pomodoro_ferma.set_enabled(attivo);
        self.pomodoro_titolo = titolo;
    }

    /// l'animale di uno schermo: il suo, se scelto, se no quello di "Animale"
    /// (con un solo schermo vale sempre quello di "Sfondo": non c'è un menu per lo schermo)
    pub fn animale_di(&self, schermo: &str) -> u32 {
        let suo = if self.schermi.len() >= 2 { self.cfg.animale_schermi.get(schermo) } else { None };
        suo.copied().unwrap_or(self.cfg.animale)
    }

    /// gli sfondi tolti dalla rotazione su uno schermo: i suoi, se li ha (e ci sono due o più
    /// schermi), se no quelli di tutti
    fn escluse_di(&self, schermo: &str) -> &[String] {
        let sue = if self.schermi.len() >= 2 { self.cfg.rotazione_escluse_schermi.get(schermo) } else { None };
        sue.unwrap_or(&self.cfg.rotazione_escluse)
    }

    /// le impostazioni che cambiano da schermo a schermo (e la rotazione, che cambia insieme
    /// all'animale quando si sceglie "Rotazione > Spenta" o un intervallo)
    pub fn js_schermo(&self, schermo: &str) -> [String; 3] {
        [
            self.js_widget(schermo),
            self.js_rotazione(schermo),
            format!("window.zooSetting && zooSetting('animale', {})", self.animale_di(schermo)),
        ]
    }

    /// le spunte di "Animale" e "Rotazione" secondo la configurazione: con un animale fisso
    /// la rotazione è "Spenta", se no è spuntato il suo intervallo
    fn segna_animale(&self) {
        let m = &self.menus;
        for (k, item) in m.animali.iter().enumerate() {
            item.set_checked(k as u32 == self.cfg.animale);
        }
        m.rot_spenta.set_checked(self.cfg.animale != 0);
        for (item, (secondi, _)) in m.rot_tempi.iter().zip(INTERVALLI.iter()) {
            item.set_checked(self.cfg.animale == 0 && *secondi == self.cfg.rotazione_secondi);
        }
    }

    /// "Rotazione > Spenta": chiede alla pagina quale animale mostra (risponde dopo)
    pub fn chiedi_animale(&self, webview: &wry::WebView) {
        let mostrato = self.animale_mostrato.clone();
        let fatto = webview.evaluate_script_with_callback(JS_ANIMALE, move |r| {
            let i = r.trim().parse::<u32>().unwrap_or(0);
            if (1..=specie().len() as u32).contains(&i) {
                if let Ok(mut m) = mostrato.lock() {
                    *m = Some(i);
                }
            }
        });
        if let Err(e) = fatto {
            zoo_log!("rotazione: la pagina non risponde: {e}");
        }
    }

    /// la risposta di `chiedi_animale`, se è arrivata: quell'animale diventa fisso (come
    /// sceglierlo in "Animale"). true = le pagine vanno aggiornate (`js_schermo`)
    pub fn risposta_animale(&mut self) -> bool {
        let Some(i) = self.animale_mostrato.lock().ok().and_then(|mut m| m.take()) else { return false };
        zoo_log!("rotazione spenta: resta {}", specie().get(i as usize - 1).map(|s| s.id).unwrap_or("?"));
        self.cfg.animale = i;
        self.segna_animale();
        self.cfg.save();
        true
    }

    /// tutte le impostazioni di uno schermo, da mandare quando la sua pagina ha finito di caricare
    /// (`in_carica`: vedi js_in_carica)
    /// Le impostazioni di uno schermo come script che la WebView esegue prima della pagina
    /// (`with_initialization_script`): la pagina le applica prima del primo fotogramma
    /// (ZOO_INIZIO), così lo sfondo compare già con il tema, lo sfondo scelto, l'orologio e i
    /// widget giusti, invece di partire con quelli di serie e cambiarli appena caricato. Le
    /// stesse impostazioni arrivano comunque a pagina caricata (push_settings).
    pub fn script_iniziale(&self, schermo: &str) -> String {
        let in_carica = crate::power::on_ac() && crate::power::ha_batteria();
        let mut s = String::from("window.ZOO_INIZIO = function () {\n");
        for c in self.js_tutto(in_carica, schermo) {
            s.push_str("  try { ");
            s.push_str(&c);
            s.push_str("; } catch (e) {}\n");
        }
        s.push_str("};\n");
        s
    }

    pub fn js_tutto(&self, in_carica: bool, schermo: &str) -> Vec<String> {
        let mut v = vec![
            self.js_tema(),
            self.js_fotogrammi(),
            // su Linux i fotogrammi agganciati agli aggiornamenti dello schermo: il timer di
            // WebKitGTK è impreciso e l'animazione scattava (vedi _loop nella pagina)
            format!("window.zooSetting && zooSetting('vsync', {})", cfg!(target_os = "linux")),
            self.js_nomi(),
            self.js_rotazione(schermo),
            self.js_orologio(),
            format!(
                "if (window.zooSetting) {{ zooSetting('inCarica', {in_carica}); zooSetting('koiInCarica', {}); }}",
                self.cfg.koi_in_carica
            ),
        ];
        v.extend(self.js_schermo(schermo));
        // quello che vive nel programma e non nella pagina: torna dopo "Ricarica sfondo"
        v.push(self.musica.js_attuale());
        v.push(self.js_pomodoro());
        v
    }

    /// Gli schermi collegati, il principale per primo. Rifà le voci del menu che
    /// dipendono dagli schermi (solo se sono cambiati).
    pub fn set_schermi(&mut self, schermi: Vec<Schermo>) {
        if schermi == self.schermi {
            return;
        }
        let m = &mut self.menus;
        for (id, item) in m.widget_schermi.drain(..) {
            if id == PRINCIPALE || id == TUTTI {
                m.widget_schermi_fissi.push((id, item));
            } else {
                let _ = m.widget_sub.remove(&item);
            }
        }
        m.widget_schermi.append(&mut m.widget_schermi_fissi);
        for (n, s) in schermi.iter().enumerate() {
            let item = CheckMenuItem::new(nome_schermo(n, s), true, false, None);
            let _ = m.widget_sub.append(&item);
            m.widget_schermi.push((s.id.clone(), item));
        }
        riempi_sfondo(m, &self.cfg, &schermi);
        self.schermi = schermi;
        self.segna_widget_schermo();
    }

    /// mette la spunta sulla voce giusta di "Widget > Schermo"
    fn segna_widget_schermo(&self) {
        let scelta = &self.cfg.widget_schermo;
        let esiste = self.schermi.iter().any(|s| &s.id == scelta) || scelta == TUTTI;
        for (id, item) in &self.menus.widget_schermi {
            item.set_checked(if esiste { id == scelta } else { id == PRINCIPALE });
        }
    }

    pub fn on_menu(&mut self, id: &str) -> Esito {
        let m = &self.menus;
        if id == m.scene_cartella.id().0 {
            crate::scene::apri_cartella();
            return Esito::Niente;
        }
        #[cfg(any(windows, target_os = "macos"))]
        if let Some(i) = m.salva_scene.iter().position(|(_, v)| v.id().0 == id) {
            // la scena dello screensaver: va nelle sue impostazioni (screensaver.json, che
            // su macOS il modulo legge anche dalla sua sandbox: vedi dreams/src/config.rs)
            for (k, (_, v)) in m.salva_scene.iter().enumerate() {
                v.set_checked(k == i);
            }
            let mut c = ascii_dreams::Config::carica();
            c.scena = m.salva_scene[i].0.to_string();
            if let Err(e) = c.salva() {
                zoo_log!("screensaver: impostazioni non salvate: {e}");
            }
            return Esito::Niente;
        }
        #[cfg(windows)]
        if id == m.salva_prova.id().0 {
            // lo screensaver si chiude se non è in primo piano: dopo il clic sul menu
            // l'app può darglielo
            if let Ok(exe) = std::env::current_exe() {
                if let Ok(figlio) = std::process::Command::new(exe).arg("/s").spawn() {
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(figlio.id());
                    }
                }
            }
            return Esito::Niente;
        }
        // macOS: il modulo dello screensaver, a tutto schermo, in un processo a parte (mac.rs)
        #[cfg(target_os = "macos")]
        if id == m.salva_prova.id().0 {
            if let Ok(exe) = std::env::current_exe() {
                if let Err(e) = std::process::Command::new(exe).arg("--salvaschermo").spawn() {
                    zoo_log!("screensaver: non parte: {e}");
                }
            }
            return Esito::Niente;
        }
        if let Some(i) = m.animali.iter().position(|x| x.id().0 == id) {
            self.cfg.animale = i as u32;
            self.segna_animale();
            self.cfg.save();
            return Esito::Schermi;
        }
        // "Rotazione > Spenta": resta l'animale di adesso, che sa solo la pagina. Le spunte
        // restano com'erano finché non risponde (il menu le cambia da solo al clic)
        if m.rot_spenta.id().0 == id {
            self.segna_animale();
            return if self.cfg.animale == 0 { Esito::FermaRotazione } else { Esito::Niente };
        }
        // un intervallo: la rotazione riparte, se era spenta
        if let Some(i) = m.rot_tempi.iter().position(|x| x.id().0 == id) {
            self.cfg.rotazione_secondi = INTERVALLI[i].0;
            self.cfg.animale = 0;
            self.segna_animale();
            self.cfg.save();
            return Esito::Schermi;
        }
        if let Some(i) = m.widget_schermi.iter().position(|x| x.1.id().0 == id) {
            self.cfg.widget_schermo = m.widget_schermi[i].0.clone();
            self.segna_widget_schermo();
            self.cfg.save();
            return Esito::Schermi;
        }
        // "Sfondo" > uno schermo: cosa mostra
        if let Some((ss, k)) = m.sfondo_schermi.iter().find_map(|ss| ss.voci.iter().position(|x| x.id().0 == id).map(|k| (ss, k))) {
            for (j, v) in ss.voci.iter().enumerate() {
                v.set_checked(j == k);
            }
            if k == 0 {
                self.cfg.animale_schermi.remove(&ss.id);
            } else {
                self.cfg.animale_schermi.insert(ss.id.clone(), k as u32 - 1);
            }
            self.cfg.save();
            return Esito::Schermi;
        }
        // "In rotazione" per tutti gli schermi: lo seguono anche gli schermi senza una lista sua
        if let Some(i) = m.rot_specie.iter().position(|x| x.id().0 == id) {
            // almeno uno sfondo deve restare in rotazione
            if !m.rot_specie.iter().any(|x| x.is_checked()) {
                m.rot_specie[i].set_checked(true);
            }
            self.cfg.rotazione_escluse = escluse(&m.rot_specie);
            for ss in m.sfondo_schermi.iter().filter(|ss| !self.cfg.rotazione_escluse_schermi.contains_key(&ss.id)) {
                segna_rotazione(&ss.rot, &self.cfg.rotazione_escluse);
            }
            self.cfg.save();
            return Esito::Schermi;
        }
        // "In rotazione" di uno schermo > "Come \"Tutti gli schermi\"": da spuntata, lo schermo
        // prende una lista sua (per ora uguale); da tolta, torna a seguire quella di tutti
        if let Some(ss) = m.sfondo_schermi.iter().find(|ss| ss.rot_come.id().0 == id) {
            if self.cfg.rotazione_escluse_schermi.remove(&ss.id).is_some() {
                segna_rotazione(&ss.rot, &self.cfg.rotazione_escluse);
            } else {
                self.cfg.rotazione_escluse_schermi.insert(ss.id.clone(), self.cfg.rotazione_escluse.clone());
            }
            ss.rot_come.set_checked(!self.cfg.rotazione_escluse_schermi.contains_key(&ss.id));
            self.cfg.save();
            return Esito::Schermi;
        }
        // "In rotazione" di uno schermo: uno sfondo (lo schermo ha da adesso una lista sua)
        if let Some((ss, i)) = m.sfondo_schermi.iter().find_map(|ss| ss.rot.iter().position(|x| x.id().0 == id).map(|i| (ss, i))) {
            if !ss.rot.iter().any(|x| x.is_checked()) {
                ss.rot[i].set_checked(true);
            }
            self.cfg.rotazione_escluse_schermi.insert(ss.id.clone(), escluse(&ss.rot));
            ss.rot_come.set_checked(false);
            self.cfg.save();
            return Esito::Schermi;
        }
        // i widget cambiano da schermo a schermo: si rimandano a ognuno i suoi
        let mut widget = false;
        let js = if let Some(i) = m.nomi.iter().position(|x| x.id().0 == id) {
            let Some(&Sfondo { id: sid, nome: label, .. }) = con_nome().nth(i) else { return Esito::Niente };
            let attuale = self.cfg.nomi.get(sid).cloned().unwrap_or_default();
            let domanda = format!("Nome per {} (vuoto = nessun nome):", label.to_lowercase());
            let Some(nuovo) = dialog::chiedi_nome("ASCII Zoo", &domanda, &attuale, NOME_MAX) else {
                return Esito::Niente; // annullato: non cambia niente
            };
            let nuovo = pulisci_nome(&nuovo);
            self.menus.nomi[i].set_text(etichetta_nome(label, &nuovo));
            self.cfg.nomi.insert(sid.to_string(), nuovo);
            Some(self.js_nomi())
        } else if id == m.nota.id().0 {
            let Some(nuova) = dialog::chiedi_nome(
                "ASCII Zoo",
                "Nota da mostrare sul desktop (vuoto = nessuna nota):",
                &self.cfg.nota,
                NOTA_MAX,
            ) else {
                return Esito::Niente;
            };
            self.cfg.nota = pulisci_testo(&nuova, NOTA_MAX);
            self.menus.nota.set_text(etichetta_nota(&self.cfg.nota));
            {
                widget = true;
                None
            }
        } else if id == m.conto.id().0 {
            if !self.chiedi_conto() {
                return Esito::Niente;
            }
            self.menus.conto.set_text(etichetta_conto(&self.cfg));
            {
                widget = true;
                None
            }
        } else if id == m.statistiche.id().0 {
            self.cfg.widget_statistiche = m.statistiche.is_checked();
            {
                widget = true;
                None
            }
        } else if [&m.pomodoro_avvia, &m.pomodoro_salta, &m.pomodoro_ferma].iter().any(|x| x.id().0 == id) {
            let ora = adesso_ms();
            if id == m.pomodoro_avvia.id().0 {
                self.pomodoro.avvia_pausa_riprendi(ora);
            } else if id == m.pomodoro_salta.id().0 {
                self.pomodoro.salta(ora);
            } else {
                self.pomodoro.ferma();
            }
            self.aggiorna_menu_pomodoro();
            // niente da salvare: il pomodoro non sopravvive alla chiusura del programma
            return Esito::Cambiato(Some(self.js_pomodoro()));
        } else if let Some(i) = m.pomodoro_durate.iter().position(|x| x.id().0 == id) {
            let (lavoro, pausa, lunga, _) = POMODORI[i];
            (self.cfg.pomodoro_lavoro, self.cfg.pomodoro_pausa, self.cfg.pomodoro_lunga) = (lavoro, pausa, lunga);
            for (k, item) in self.menus.pomodoro_durate.iter().enumerate() {
                item.set_checked(k == i);
            }
            self.pomodoro.set_durate(durate(&self.cfg));
            None
        } else if id == m.pomodoro_lunga.id().0 {
            self.cfg.pomodoro_ogni = if m.pomodoro_lunga.is_checked() { 4 } else { 0 };
            self.pomodoro.set_durate(durate(&self.cfg));
            None
        } else if id == m.musica.id().0 {
            self.cfg.widget_musica = m.musica.is_checked();
            {
                widget = true;
                None
            }
        } else if m.nerd.iter().any(|(_, item)| item.id().0 == id) {
            self.cfg.widget_nerd =
                m.nerd.iter().filter(|(_, item)| item.is_checked()).map(|(nid, _)| nid.to_string()).collect();
            {
                widget = true;
                None
            }
        } else if let Some(i) = m.dimensioni.iter().position(|x| x.id().0 == id) {
            self.cfg.widget_dimensione = DIMENSIONI[i].0.to_string();
            for (k, item) in self.menus.dimensioni.iter().enumerate() {
                item.set_checked(k == i);
            }
            {
                widget = true;
                None
            }
        } else if let Some(i) = m.cornici.iter().position(|x| x.id().0 == id) {
            self.cfg.widget_cornice = CORNICI[i].0.to_string();
            for (k, item) in self.menus.cornici.iter().enumerate() {
                item.set_checked(k == i);
            }
            {
                widget = true;
                None
            }
        } else if id == m.sfondo_pieno.id().0 {
            self.cfg.widget_sfondo_pieno = m.sfondo_pieno.is_checked();
            {
                widget = true;
                None
            }
        } else if let Some(i) = m.angoli.iter().position(|x| x.id().0 == id) {
            self.cfg.widget_angolo = ANGOLI[i].0.to_string();
            for (k, item) in self.menus.angoli.iter().enumerate() {
                item.set_checked(k == i);
            }
            {
                widget = true;
                None
            }
        } else if let Some(i) = m.temi.iter().position(|x| x.id().0 == id) {
            self.cfg.tema = TEMI[i].0.to_string();
            for (k, item) in self.menus.temi.iter().enumerate() {
                item.set_checked(k == i);
            }
            self.tema_pagina = self.tema_ora();
            Some(self.js_tema())
        } else if let Some(i) = m.fotogrammi.iter().position(|x| x.id().0 == id) {
            self.cfg.fotogrammi = FOTOGRAMMI[i].0;
            for (k, item) in self.menus.fotogrammi.iter().enumerate() {
                item.set_checked(k == i);
            }
            Some(self.js_fotogrammi())
        } else if id == m.clic.id().0 {
            self.cfg.clic_desktop = m.clic.is_checked();
            None
        } else if id == m.orologio.id().0 {
            self.cfg.orologio = m.orologio.is_checked();
            Some(self.js_orologio())
        } else if id == m.koi.id().0 {
            self.cfg.koi_in_carica = m.koi.is_checked();
            Some(format!("window.zooSetting && zooSetting('koiInCarica', {})", self.cfg.koi_in_carica))
        } else if id == m.batteria.id().0 {
            self.cfg.pausa_batteria = m.batteria.is_checked();
            None
        } else if id == m.schermo.id().0 {
            self.cfg.pausa_schermo_intero = m.schermo.is_checked();
            None
        } else if id == m.coperto.id().0 {
            self.cfg.pausa_coperto = m.coperto.is_checked();
            None
        } else if id == m.avvio.id().0 {
            autostart::set(m.avvio.is_checked());
            m.avvio.set_checked(autostart::enabled());
            return Esito::Niente;
        } else if id == m.salvaschermo.id().0 {
            return Esito::Salvaschermo;
        } else if id == m.foto.id().0 {
            return Esito::Foto;
        } else if id == m.ricarica.id().0 {
            return Esito::Ricarica;
        } else if id == m.esci.id().0 {
            return Esito::Esci;
        } else {
            return Esito::Niente;
        };
        self.cfg.save();
        if widget {
            return Esito::Schermi;
        }
        Esito::Cambiato(js)
    }
}

impl Core {
    /// chiede etichetta e data del conto alla rovescia; false se si annulla
    fn chiedi_conto(&mut self) -> bool {
        let attuale = if self.cfg.conto_etichetta.is_empty() { "esame" } else { &self.cfg.conto_etichetta };
        let Some(etichetta) = dialog::chiedi_nome("ASCII Zoo", "Conto alla rovescia: per cosa?", attuale, ETICHETTA_MAX)
        else {
            return false;
        };
        let mut domanda = "Che giorno? (es. 25/12/2026; vuoto = spegni il conto alla rovescia)";
        let mut data = data_italiana(&self.cfg.conto_data);
        loop {
            let Some(scritta) = dialog::chiedi_nome("ASCII Zoo", domanda, &data, 10) else {
                return false;
            };
            if scritta.trim().is_empty() {
                self.cfg.conto_data.clear();
                break;
            }
            if let Some(iso) = leggi_data(&scritta) {
                self.cfg.conto_data = iso;
                break;
            }
            domanda = "Data non valida. Scrivila come giorno/mese/anno, es. 25/12/2026:";
            data = scritta;
        }
        self.cfg.conto_etichetta = pulisci_testo(&etichetta, ETICHETTA_MAX);
        true
    }
}

/// millisecondi dall'epoch, come Date.now() nella pagina
pub fn adesso_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// "pausa lunga" → "Pausa lunga"
fn maiuscola(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|p| p.to_uppercase().chain(c).collect()).unwrap_or_default()
}

/// le durate del pomodoro scelte nella config
fn durate(cfg: &Config) -> Durate {
    Durate::minuti(cfg.pomodoro_lavoro, cfg.pomodoro_pausa, cfg.pomodoro_lunga, cfg.pomodoro_ogni)
}

/// testo scelto dall'utente dentro una voce di menu
fn nel_menu(s: &str) -> String {
    // nei menu di Windows "&" sottolinea la lettera dopo: va raddoppiata
    if cfg!(windows) {
        s.replace('&', "&&")
    } else {
        s.to_string()
    }
}

fn etichetta_nota(nota: &str) -> String {
    if nota.is_empty() {
        return "Nota: (nessuna)…".into();
    }
    let corta: String = nota.chars().take(24).collect();
    let altro = if nota.chars().count() > 24 { "…" } else { "" };
    format!("Nota: {}{altro}…", nel_menu(&corta))
}

fn etichetta_conto(cfg: &Config) -> String {
    if cfg.conto_data.is_empty() {
        return "Conto alla rovescia: (spento)…".into();
    }
    format!("Conto alla rovescia: {} {}…", nel_menu(&cfg.conto_etichetta), data_italiana(&cfg.conto_data))
}

/// icona dell'area di notifica / barra dei menu: una piccola koi arancione e bianca
pub fn tray_icon_image() -> Icon {
    const N: u32 = 32;
    let mut px = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 - 15.5, y as f32 - 15.5);
            // corpo ellittico + coda a sinistra
            let body = (fx / 12.0).powi(2) + (fy / 7.5).powi(2) <= 1.0 && fx > -9.0;
            let tail = fx <= -8.0 && fx >= -15.0 && fy.abs() <= (-8.0 - fx) * 0.9 + 1.0;
            if !(body || tail) {
                continue;
            }
            let spot = ((fx - 2.0) / 5.0).powi(2) + ((fy + 1.5) / 3.5).powi(2) <= 1.0;
            let eye = (fx - 8.0).powi(2) + (fy + 2.0).powi(2) <= 1.6;
            let (r, g, b) = if eye { (20, 20, 30) } else if spot { (246, 242, 233) } else { (255, 143, 46) };
            let i = ((y * N + x) * 4) as usize;
            px[i..i + 4].copy_from_slice(&[r, g, b, 255]);
        }
    }
    Icon::from_rgba(px, N, N).expect("icona")
}

/// voce del menu dei nomi: "Koi: KARPISA…" oppure "Farfalle: (nessuno)…"
pub fn etichetta_nome(label: &str, nome: &str) -> String {
    let nome = if nome.is_empty() { "(nessuno)".to_string() } else { nel_menu(nome) };
    format!("{label}: {nome}…")
}

/// i nomi italiani delle scene dello screensaver (in dreams/ sono in inglese, per la
/// finestra delle impostazioni di Windows)
pub fn nome_salvaschermo(id: &str) -> Option<&'static str> {
    PAESAGGI.iter().find(|n| n.0 == id).map(|n| n.1)
}

/// Linux con GNOME: l'app è stata aggiornata ma GNOME usa ancora l'estensione di prima (la
/// carica solo all'accesso). Quella della 0.7.0 riconosce il menu di ASCII Zoo solo dalla voce
/// "Animale" (linux/gnome/…/menu.js), e senza di lei i sottomenu si aprono in una striscia di
/// pochi pixel: finché non si esce e rientra il sottomenu "Sfondo" si chiama come allora, e in
/// cima al menu c'è una riga che lo dice. Si sceglie prima di Core::new (vedi linux.rs).
static ESTENSIONE_VECCHIA: std::sync::OnceLock<()> = std::sync::OnceLock::new();

#[cfg(target_os = "linux")]
pub fn estensione_vecchia() {
    let _ = ESTENSIONE_VECCHIA.set(());
}

/// il titolo del sottomenu "Sfondo" (vedi ESTENSIONE_VECCHIA)
fn titolo_sfondo() -> &'static str {
    if ESTENSIONE_VECCHIA.get().is_some() {
        "Animale"
    } else {
        "Sfondo"
    }
}

fn build_menu(cfg: &Config, solo_windows: bool) -> (Menu, Menus) {
    let menu = Menu::new();
    if ESTENSIONE_VECCHIA.get().is_some() {
        let _ = menu.append(&MenuItem::new("Esci e rientra nella sessione per finire l'aggiornamento", false, None));
        let _ = menu.append(&PredefinedMenuItem::separator());
    }

    // "Sfondo": cosa si vede e cosa gira (riempi_sfondo, poi di nuovo quando cambiano gli schermi)
    let sub = Submenu::new(titolo_sfondo(), true);
    let scene_cartella = MenuItem::new("Apri la cartella delle scene…", true, None);

    let rot = Submenu::new("Rotazione", true);
    // spenta = uno sfondo fisso (scelto in "Sfondo", o quello che si vede quando si spegne)
    let rot_spenta = CheckMenuItem::new("Spenta: resta lo sfondo di adesso", true, cfg.animale != 0, None);
    let _ = rot.append(&rot_spenta);
    let rot_tempi: Vec<CheckMenuItem> = INTERVALLI
        .iter()
        .map(|(s, name)| CheckMenuItem::new(*name, true, cfg.animale == 0 && *s == cfg.rotazione_secondi, None))
        .collect();
    for item in &rot_tempi {
        let _ = rot.append(item);
    }

    let nomi_sub = Submenu::new("Nomi degli animali", true);
    // solo gli animali: un paesaggio non ha un animale più grande a cui dare un nome
    let nomi: Vec<MenuItem> = con_nome()
        .map(|s| MenuItem::new(etichetta_nome(s.nome, cfg.nomi.get(s.id).map(String::as_str).unwrap_or("")), true, None))
        .collect();
    for item in &nomi {
        let _ = nomi_sub.append(item);
    }

    let widget = Submenu::new("Widget", true);
    let nota = MenuItem::new(etichetta_nota(&cfg.nota), true, None);
    let conto = MenuItem::new(etichetta_conto(cfg), true, None);
    let statistiche = CheckMenuItem::new("CPU, RAM e batteria", true, cfg.widget_statistiche, None);
    let nerd_sub = Submenu::new("Statistiche per nerd", true);
    let nerd: Vec<(&'static str, CheckMenuItem)> = nerd::VOCI
        .iter()
        .filter(|(id, _)| nerd::disponibile(id))
        .map(|(id, name)| (*id, CheckMenuItem::new(*name, true, cfg.widget_nerd.iter().any(|x| x == id), None)))
        .collect();
    for (_, item) in &nerd {
        let _ = nerd_sub.append(item);
    }
    let aspetto = Submenu::new("Aspetto", true);
    let _ = aspetto.append(&MenuItem::new("Dimensione:", false, None));
    let dimensioni: Vec<CheckMenuItem> = DIMENSIONI
        .iter()
        .map(|(id, name)| CheckMenuItem::new(*name, true, cfg.widget_dimensione == *id, None))
        .collect();
    for item in &dimensioni {
        let _ = aspetto.append(item);
    }
    let _ = aspetto.append(&PredefinedMenuItem::separator());
    let _ = aspetto.append(&MenuItem::new("Cornice:", false, None));
    let cornici: Vec<CheckMenuItem> = CORNICI
        .iter()
        .map(|(id, name)| CheckMenuItem::new(*name, true, cfg.widget_cornice == *id, None))
        .collect();
    for item in &cornici {
        let _ = aspetto.append(item);
    }
    let _ = aspetto.append(&PredefinedMenuItem::separator());
    let sfondo_pieno = CheckMenuItem::new("Sfondo pieno dietro i riquadri", true, cfg.widget_sfondo_pieno, None);
    let _ = aspetto.append(&sfondo_pieno);
    let angoli: Vec<CheckMenuItem> = ANGOLI
        .iter()
        .map(|(id, name)| CheckMenuItem::new(*name, true, cfg.widget_angolo == *id, None))
        .collect();
    let _ = widget.append(&nota);
    let _ = widget.append(&conto);
    let _ = widget.append(&statistiche);
    let _ = widget.append(&nerd_sub);
    let musica = CheckMenuItem::new("In riproduzione", true, cfg.widget_musica, None);
    let _ = widget.append(&musica);
    let _ = widget.append(&PredefinedMenuItem::separator());
    let _ = widget.append(&aspetto);
    let _ = widget.append(&PredefinedMenuItem::separator());
    let _ = widget.append(&MenuItem::new("Posizione:", false, None));
    for item in &angoli {
        let _ = widget.append(item);
    }
    let _ = widget.append(&PredefinedMenuItem::separator());
    let _ = widget.append(&MenuItem::new("Schermo:", false, None));
    let widget_schermi = vec![
        (PRINCIPALE.to_string(), CheckMenuItem::new("Schermo principale", true, true, None)),
        (TUTTI.to_string(), CheckMenuItem::new("Tutti gli schermi", true, false, None)),
    ];
    for (_, item) in &widget_schermi {
        let _ = widget.append(item);
    }
    // gli schermi veri si aggiungono con set_schermi

    let pomodoro_sub = Submenu::new("Pomodoro", true);
    let pomodoro_stato = MenuItem::new("Fermo", false, None);
    let pomodoro_avvia = MenuItem::new("Avvia", true, None);
    let pomodoro_salta = MenuItem::new("Salta fase", false, None);
    let pomodoro_ferma = MenuItem::new("Ferma", false, None);
    let pomodoro_durate: Vec<CheckMenuItem> = POMODORI
        .iter()
        .map(|(l, p, _, name)| CheckMenuItem::new(*name, true, cfg.pomodoro_lavoro == *l && cfg.pomodoro_pausa == *p, None))
        .collect();
    let pomodoro_lunga = CheckMenuItem::new("Pausa lunga ogni 4 pomodori", true, cfg.pomodoro_ogni > 0, None);
    let _ = pomodoro_sub.append(&pomodoro_stato);
    let _ = pomodoro_sub.append(&pomodoro_avvia);
    let _ = pomodoro_sub.append(&pomodoro_salta);
    let _ = pomodoro_sub.append(&pomodoro_ferma);
    let _ = pomodoro_sub.append(&PredefinedMenuItem::separator());
    for item in &pomodoro_durate {
        let _ = pomodoro_sub.append(item);
    }
    let _ = pomodoro_sub.append(&pomodoro_lunga);

    let tema_sub = Submenu::new("Tema", true);
    // un valore sconosciuto nella config vale come "scuro"
    let tema_cfg = if TEMI.iter().any(|t| t.0 == cfg.tema) { cfg.tema.as_str() } else { "scuro" };
    let temi: Vec<CheckMenuItem> =
        TEMI.iter().map(|(id, name)| CheckMenuItem::new(*name, true, tema_cfg == *id, None)).collect();
    for item in &temi {
        let _ = tema_sub.append(item);
    }

    let fotogrammi_sub = Submenu::new("Fotogrammi al secondo", true);
    // un valore che non è nel menu vale come 30
    let fps_cfg = if FOTOGRAMMI.iter().any(|f| f.0 == cfg.fotogrammi) { cfg.fotogrammi } else { 30 };
    let fotogrammi: Vec<CheckMenuItem> =
        FOTOGRAMMI.iter().map(|(n, name)| CheckMenuItem::new(*name, true, fps_cfg == *n, None)).collect();
    for item in &fotogrammi {
        let _ = fotogrammi_sub.append(item);
    }

    let avvio_label = if cfg!(windows) { "Avvia con Windows" } else { "Apri al login" };
    let mut m = Menus {
        animali: Vec::new(),
        rot_spenta,
        rot_tempi,
        rot_specie: Vec::new(),
        nomi,
        sfondo_sub: sub,
        sfondo_schermi: Vec::new(),
        scene_cartella,
        nota,
        conto,
        statistiche,
        nerd,
        musica,
        dimensioni,
        cornici,
        sfondo_pieno,
        angoli,
        temi,
        fotogrammi,
        pomodoro_sub,
        pomodoro_stato,
        pomodoro_avvia,
        pomodoro_salta,
        pomodoro_ferma,
        pomodoro_durate,
        pomodoro_lunga,
        widget_sub: widget,
        widget_schermi,
        widget_schermi_fissi: Vec::new(),
        clic: CheckMenuItem::new("Clic sul desktop agli animali", true, cfg.clic_desktop, None),
        orologio: CheckMenuItem::new("Orologio al centro", true, cfg.orologio, None),
        koi: CheckMenuItem::new("Koi nello stagno in carica (con \"A rotazione\")", true, cfg.koi_in_carica, None),
        batteria: CheckMenuItem::new("Pausa a batteria", true, cfg.pausa_batteria, None),
        schermo: CheckMenuItem::new("Pausa con app a schermo intero", true, cfg.pausa_schermo_intero, None),
        coperto: CheckMenuItem::new("Pausa quando le finestre coprono il desktop", true, cfg.pausa_coperto, None),
        avvio: CheckMenuItem::new(avvio_label, true, autostart::enabled(), None),
        #[cfg(target_os = "linux")]
        salvaschermo: MenuItem::new("Usa come screensaver…", true, None),
        // su Windows e macOS sta nel sottomenu "Screensaver", sotto la scelta della scena
        #[cfg(windows)]
        salvaschermo: MenuItem::new("Attiva come screensaver di Windows…", true, None),
        #[cfg(target_os = "macos")]
        salvaschermo: MenuItem::new("Attiva come screensaver di macOS…", true, None),
        #[cfg(any(windows, target_os = "macos"))]
        salva_scene: {
            let scelta = ascii_dreams::Config::carica().scena;
            let scelta = if ascii_dreams::scene().iter().any(|s| s.0 == scelta) { scelta } else { "rotazione".into() };
            std::iter::once(("rotazione", "Tutte, una dopo l'altra"))
                .chain(ascii_dreams::scene().iter().map(|&(id, nome)| (id, nome_salvaschermo(id).unwrap_or(nome))))
                .map(|(id, nome)| (id, CheckMenuItem::new(nome, true, id == scelta, None)))
                .collect()
        },
        #[cfg(any(windows, target_os = "macos"))]
        salva_prova: MenuItem::new("Prova adesso", true, None),
        foto: MenuItem::new("Salva una foto dello sfondo", true, None),
        ricarica: MenuItem::new("Ricarica sfondo", true, None),
        esci: MenuItem::new("Esci", true, None),
    };
    // gli schermi veri arrivano con set_schermi: per ora come con uno solo
    riempi_sfondo(&mut m, cfg, &[]);
    let _ = menu.append(&m.sfondo_sub);
    let _ = menu.append(&rot);
    let _ = menu.append(&nomi_sub);
    let _ = menu.append(&m.widget_sub);
    let _ = menu.append(&m.pomodoro_sub);
    let _ = menu.append(&tema_sub);
    let _ = menu.append(&fotogrammi_sub);
    let _ = menu.append(&m.orologio);
    let _ = menu.append(&m.koi);
    let _ = menu.append(&m.clic);
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&m.batteria);
    let _ = menu.append(&m.schermo);
    let _ = menu.append(&m.coperto);
    let _ = menu.append(&m.avvio);
    // Windows e macOS: un sottomenu, la scena si sceglie da qui (Linux ha il suo, con i minuti)
    #[cfg(target_os = "linux")]
    let _ = solo_windows;
    #[cfg(any(windows, target_os = "macos"))]
    if solo_windows {
        let salva_sub = Submenu::new("Screensaver", true);
        for (_, v) in &m.salva_scene {
            let _ = salva_sub.append(v);
        }
        let _ = salva_sub.append(&PredefinedMenuItem::separator());
        let _ = salva_sub.append(&m.salva_prova);
        let _ = salva_sub.append(&m.salvaschermo);
        let _ = menu.append(&salva_sub);
    }
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&m.foto);
    let _ = menu.append(&m.ricarica);
    let _ = menu.append(&m.esci);
    (menu, m)
}
