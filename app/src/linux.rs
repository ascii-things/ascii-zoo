//! Linux (pensato per Ubuntu/GNOME; funziona anche con altri desktop che rispettano
//! le finestre di tipo "desktop", come KDE e XFCE).
//!
//! - per ogni schermo una finestra con dentro WebKitGTK. Due modi:
//!   - **Wayland, su GNOME con l'estensione** (linux/gnome/…/sfondo.js): una finestra Wayland
//!     normale, con un titolo che l'estensione riconosce; l'estensione la minimizza e ne
//!     mostra il contenuto dentro lo sfondo di GNOME, e dice qui se lo schermo è coperto e
//!     dov'è l'area di lavoro. È il modo leggero: sotto XWayland GTK 3 non può prendere il
//!     fotogramma di WebKit dalla GPU, lo copia in memoria e XWayland lo ricarica sulla GPU
//!     (GNOME 51, scala 125%: ~0,8 core in più e non più di 20 fotogrammi al secondo);
//!   - **X11** (anche sotto Wayland, via XWayland: GDK_BACKEND=x11) negli altri casi: una
//!     finestra di tipo "desktop", sotto tutte le finestre, in tutti gli spazi di lavoro,
//!     fuori dalla barra delle applicazioni e trasparente ai clic. Anche su GNOME finché
//!     l'estensione nuova non è caricata (cioè fino al prossimo accesso dopo l'installazione),
//!     e se l'estensione si spegne mentre l'app gira (l'app riparte da sola su X11).
//! - icona AppIndicator nella barra in alto (Ubuntu la mostra di serie; su GNOME senza
//!   Ubuntu serve l'estensione "AppIndicator and KStatusNotifierItem Support")
//! - ogni mezzo secondo le pause: a batteria, con lo schermo bloccato o spento, con
//!   un'app a schermo intero (vedi `Sonda`); animare quello che nessuno vede costa
//!   CPU e GPU per niente, e qui ogni fotogramma passa da XWayland
//! - ogni 2 secondi: controllo degli schermi collegati, margini della barra in alto
//!   per i widget
//! - lo screensaver: lo fa partire l'app quando non si usa il PC (salvaschermo_linux.rs),
//!   come processo a parte che disegna le scene di dreams/ (sogno_linux.rs)

use std::cell::{Cell, RefCell};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::{gio, glib};
use tao::dpi::{PhysicalPosition, PhysicalSize};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::unix::WindowExtUnix;
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::MenuEvent;
use tray_icon::{TrayIcon, TrayIconBuilder};
use wry::{PageLoadEvent, WebView, WebViewBuilder, WebViewBuilderExtUnix};

use crate::config::autostart;
use crate::{foto, power};
use crate::shared::{self, zoo_log, Core, Esito, Schermo, JS_START, JS_STOP, URL};
use crate::stats::Stats;

#[path = "salvaschermo_linux.rs"]
mod salvaschermo;
#[path = "sogno_linux.rs"]
mod sogno;

/// `ascii-zoo --salvaschermo [--finestra] [--scena id]`: lo screensaver, lanciato dall'app
pub fn salvaschermo(args: &[String]) {
    sogno::avvia(args);
}

/// ogni quanto si ricontrollano le pause: quando lo sfondo torna visibile riparte subito
const GIRO: Duration = Duration::from_millis(500);
/// Quando fare il prossimo giro. Con ControlFlow::WaitUntil tao su Linux non mette nessun
/// timer: guarda l'ora solo quando GTK si sveglia per qualcos'altro. Di solito lo sveglia la
/// pagina, ma con lo sfondo in pausa (screensaver, batteria) può dormire per secondi: lo
/// sveglia allora un timer di GLib ogni GIRO (in `run`). La scadenza sta un po' prima di
/// quel risveglio: se cadesse appena dopo, il giro salterebbe al risveglio seguente (provato:
/// un giro al secondo invece che ogni mezzo secondo).
fn prossimo_giro() -> Instant {
    Instant::now() + GIRO - Duration::from_millis(40)
}
/// ogni quanti giri si controllano batteria, schermi e margini (2 secondi)
const GIRI_CONTROLLO: u32 = 4;

enum Evento {
    Menu(String),
    PaginaPronta,
    /// clic sul desktop dall'estensione di GNOME Shell: x, y, larghezza, altezza
    Clic(f64, f64, f64, f64),
    /// inattività, schermo bloccato… per lo screensaver
    Salvaschermo(salvaschermo::Segnale),
    /// modo Wayland: com'è ogni schermo, dall'estensione di GNOME
    Shell(Vec<StatoSchermo>),
    /// modo Wayland: l'estensione di GNOME è stata spenta
    EstensioneSpenta,
    /// su X11 solo perché l'estensione non rispondeva: ora si è accesa
    EstensioneAccesa,
}

/// Modo Wayland: l'oggetto dell'estensione di GNOME (linux/gnome/…/sfondo.js), sul nome di
/// GNOME Shell. Il titolo delle finestre che l'estensione mette al posto dello sfondo
/// comincia con TITOLO_WAYLAND, seguito dallo schermo in pixel logici: "x,y,larghezza,altezza".
const SFONDO_PERCORSO: &str = "/io/github/tressilloide/AsciiZoo/Sfondo";
const SFONDO_INTERFACCIA: &str = "io.github.tressilloide.AsciiZoo.Sfondo";
const SFONDO_TIPO: &str = "(a(iiiiiiiibbbs))";
const TITOLO_WAYLAND: &str = "@ascii-zoo-sfondo!";
const ESTENSIONE: &str = "zoo-click@tressilloide.github.io";
/// modo Wayland: se dopo questo tempo l'estensione non ha preso le finestre (al loro posto
/// resterebbero finestre normali davanti al desktop), si riparte su X11
const ATTESA_ESTENSIONE: Duration = Duration::from_secs(8);
/// modo Wayland: l'estensione risulta spenta. GNOME spegne e riaccende in un attimo tutte le
/// estensioni accese dopo una che si spegne o si accende (anche solo nell'app Estensioni):
/// si riparte su X11 solo se dopo questo tempo non risponde ancora
const ATTESA_SPENTA: Duration = Duration::from_secs(3);

/// Uno schermo come lo vede GNOME Shell (modo Wayland), in pixel logici.
#[derive(Clone, Debug, PartialEq)]
struct StatoSchermo {
    geo: [i32; 4],
    /// area di lavoro: lo schermo senza la barra in alto, il dock…
    lavoro: [i32; 4],
    /// un'app a schermo intero
    intero: bool,
    /// le finestre coprono tutta l'area di lavoro (o un'app a schermo intero)
    coperto: bool,
    /// la finestra di ASCII Zoo per questo schermo è al posto dello sfondo
    adottata: bool,
    /// "eDP-1", "HDMI-1"… (vuoto se GNOME non lo dice)
    connettore: String,
}

type StatoDBus = (i32, i32, i32, i32, i32, i32, i32, i32, bool, bool, bool, String);

fn leggi_stato(v: &glib::Variant) -> Option<Vec<StatoSchermo>> {
    let (schermi,) = v.get::<(Vec<StatoDBus>,)>()?;
    Some(
        schermi
            .into_iter()
            .map(|(x, y, w, h, lx, ly, lw, lh, intero, coperto, adottata, connettore)| StatoSchermo {
                geo: [x, y, w, h],
                lavoro: [lx, ly, lw, lh],
                intero,
                coperto,
                adottata,
                connettore,
            })
            .collect(),
    )
}

/// Il connettore di ogni schermo ("eDP-1"…) per posizione in pixel logici, da mutter
/// (org.gnome.Mutter.DisplayConfig, la stessa interfaccia di Impostazioni > Schermi). Serve
/// nel modo Wayland quando l'estensione non lo sa dire (GNOME 45–48)
fn connettori_mutter(bus: &gio::DBusConnection) -> Vec<([i32; 2], String)> {
    let Ok(v) = bus.call_sync(
        Some("org.gnome.Mutter.DisplayConfig"),
        "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig",
        "GetCurrentState",
        None,
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        1000,
        gio::Cancellable::NONE,
    ) else {
        return Vec::new();
    };
    // (u, a((ssss)…) monitor, a(iiduba(ssss)a{sv}) schermi logici, a{sv}): degli schermi
    // logici servono x, y e il primo monitor fisico, il cui primo campo è il connettore
    let logici = v.child_value(2);
    (0..logici.n_children())
        .filter_map(|i| {
            let l = logici.child_value(i);
            let (x, y) = (l.child_value(0).get::<i32>()?, l.child_value(1).get::<i32>()?);
            let monitor = l.child_value(5);
            (monitor.n_children() > 0).then(|| ([x, y], monitor.child_value(0).child_value(0).get::<String>().unwrap_or_default()))
        })
        .collect()
}

/// Lo stato dall'estensione di GNOME, se c'è (e così si sa che c'è, ed è quella nuova)
fn chiedi_stato(bus: &gio::DBusConnection) -> Option<Vec<StatoSchermo>> {
    let v = bus
        .call_sync(
            Some("org.gnome.Shell"),
            SFONDO_PERCORSO,
            SFONDO_INTERFACCIA,
            "Stato",
            None,
            Some(glib::VariantTy::new(SFONDO_TIPO).ok()?),
            gio::DBusCallFlags::NO_AUTO_START,
            2000,
            gio::Cancellable::NONE,
        )
        .ok()?;
    leggi_stato(&v)
}

/// La versione dell'estensione di GNOME che questa app si aspetta (VERSIONE in
/// linux/gnome/…/sfondo.js). GNOME carica il codice delle estensioni solo all'accesso: dopo
/// un aggiornamento, fino al prossimo accesso gira quella di prima.
const ESTENSIONE_VERSIONE: u32 = 2;

/// La versione dell'estensione che GNOME sta usando adesso: None se non c'è (o è spenta, o non
/// è GNOME); 1 per quelle fino alla 0.8.0, che non sanno dirla (ma rispondono a `Stato`).
fn versione_estensione(bus: &gio::DBusConnection) -> Option<u32> {
    let chiama = |metodo: &str, tipo: &str| {
        bus.call_sync(
            Some("org.gnome.Shell"),
            SFONDO_PERCORSO,
            SFONDO_INTERFACCIA,
            metodo,
            None,
            Some(glib::VariantTy::new(tipo).ok()?),
            gio::DBusCallFlags::NO_AUTO_START,
            2000,
            gio::Cancellable::NONE,
        )
        .ok()
    };
    if let Some(v) = chiama("Versione", "(u)") {
        return v.get::<(u32,)>().map(|(n,)| n);
    }
    chiama("Stato", SFONDO_TIPO).map(|_| 1)
}

/// Il modo Wayland si usa sotto Wayland, se l'estensione di GNOME risponde e nessuno ha
/// chiesto X11 (GDK_BACKEND=x11, oppure ASCII_ZOO_SFONDO=x11: lo mette l'app stessa quando
/// riparte perché l'estensione non c'è più). Restituisce lo stato degli schermi.
fn modo_wayland(bus: Option<&gio::DBusConnection>) -> Option<Vec<StatoSchermo>> {
    std::env::var_os("WAYLAND_DISPLAY")?;
    if std::env::var("GDK_BACKEND").is_ok_and(|b| b != "wayland") || std::env::var("ASCII_ZOO_SFONDO").is_ok_and(|v| v == "x11") {
        return None;
    }
    let bus = bus?;
    if let Some(stato) = chiedi_stato(bus) {
        return Some(stato);
    }
    // All'accesso ASCII Zoo può partire prima che GNOME Shell abbia caricato le estensioni. Se
    // la nostra è accesa la si aspetta: partire su X11 e ripartire appena risponde voleva dire
    // due avvii di WebKit e lo sfondo che spariva e ricompariva. Non oltre ATTESA_ALL_ACCESSO,
    // e si smette prima se non arriverà: GNOME Shell non risponde, non conosce l'estensione
    // (appena installata: la carica solo al prossimo accesso), o dice che non può accenderla.
    if !gnome() || !estensione_accesa() {
        return None;
    }
    let inizio = Instant::now();
    let mut visto = None;
    let mut sconosciuta: Option<Instant> = None;
    let poco = Duration::from_secs(2);
    while inizio.elapsed() < ATTESA_ALL_ACCESSO {
        std::thread::sleep(Duration::from_millis(100));
        if let Some(stato) = chiedi_stato(bus) {
            zoo_log!("sfondo: l'estensione di GNOME ha risposto dopo {} ms di attesa", inizio.elapsed().as_millis());
            return Some(stato);
        }
        let st = stato_estensione(bus);
        if st != visto {
            zoo_log!("sfondo: aspetto l'estensione di GNOME, stato {st:?} dopo {} ms", inizio.elapsed().as_millis());
            visto = st;
        }
        match st {
            // GNOME Shell non risponde (non c'è, o non è GNOME Shell)
            None if inizio.elapsed() > poco => {
                zoo_log!("sfondo: GNOME Shell non risponde");
                break;
            }
            // risponde ma non la conosce: installata dopo l'accesso, la caricherà al prossimo
            // (all'accesso GNOME la conosce già al primo controllo: basta un secondo)
            Some(None) if sconosciuta.get_or_insert_with(Instant::now).elapsed() > Duration::from_secs(1) => {
                zoo_log!("sfondo: GNOME non ha caricato l'estensione (lo farà al prossimo accesso)");
                break;
            }
            // 3 (errore) è anche lo stato di un'estensione che GNOME sta ancora caricando: conta
            // solo se GNOME ha registrato degli errori; 4 = fatta per un'altra versione di GNOME
            Some(Some(x)) if x == 4.0 || (x == 3.0 && errori_estensione(bus) > 0) => {
                zoo_log!("sfondo: GNOME non può accendere l'estensione (stato {x})");
                break;
            }
            _ => {}
        }
    }
    None
}

/// quanto aspettare all'avvio un'estensione accesa che non risponde ancora (vedi modo_wayland)
const ATTESA_ALL_ACCESSO: Duration = Duration::from_secs(10);

/// la sessione è di GNOME (XDG_CURRENT_DESKTOP, es. "ubuntu:GNOME")
fn gnome() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|d| d.split(':').any(|x| x.eq_ignore_ascii_case("gnome")))
}

/// L'estensione è tra quelle accese nelle impostazioni di GNOME (e le estensioni dell'utente
/// non sono spente del tutto): se non risponde è perché GNOME Shell non l'ha ancora caricata
fn estensione_accesa() -> bool {
    let Some(schema) = gio::SettingsSchemaSource::default().and_then(|s| s.lookup("org.gnome.shell", true)) else {
        return false;
    };
    if !schema.has_key("enabled-extensions") {
        return false;
    }
    let s = gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None);
    let spente = schema.has_key("disable-user-extensions") && s.boolean("disable-user-extensions");
    !spente && s.strv("enabled-extensions").iter().any(|u| u.as_str() == ESTENSIONE)
}

/// quanti errori GNOME Shell ha registrato per l'estensione (0 anche se non risponde)
fn errori_estensione(bus: &gio::DBusConnection) -> usize {
    bus.call_sync(
        Some("org.gnome.Shell"),
        "/org/gnome/Shell",
        "org.gnome.Shell.Extensions",
        "GetExtensionErrors",
        Some(&(ESTENSIONE,).to_variant()),
        glib::VariantTy::new("(as)").ok(),
        gio::DBusCallFlags::NO_AUTO_START,
        1000,
        gio::Cancellable::NONE,
    )
    .ok()
    .and_then(|v| v.get::<(Vec<String>,)>())
    .map_or(0, |(e,)| e.len())
}

/// Lo stato dell'estensione secondo GNOME Shell (1 accesa, 3 errore, 4 per un'altra versione
/// di GNOME, 6 caricata…); Some(None) se Shell risponde ma non la conosce, None se non risponde
fn stato_estensione(bus: &gio::DBusConnection) -> Option<Option<f64>> {
    let v = bus
        .call_sync(
            Some("org.gnome.Shell"),
            "/org/gnome/Shell",
            "org.gnome.Shell.Extensions",
            "GetExtensionInfo",
            Some(&(ESTENSIONE,).to_variant()),
            Some(glib::VariantTy::new("(a{sv})").ok()?),
            gio::DBusCallFlags::NO_AUTO_START,
            1000,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let (info,) = v.get::<(std::collections::HashMap<String, glib::Variant>,)>()?;
    Some(info.get("state").and_then(|s| s.get::<f64>()))
}

/// Modo Wayland senza l'estensione (spenta, o non prende le finestre): si riparte su X11,
/// lo stesso programma con gli stessi argomenti (exec: stesso processo, l'icona e le finestre
/// si rifanno).
/// Su X11 solo perché all'avvio l'estensione non rispondeva (all'accesso ASCII Zoo può
/// partire prima che GNOME abbia caricato le estensioni): appena risponde si riparte nel modo
/// Wayland. Mai se X11 è stato chiesto (GDK_BACKEND, ASCII_ZOO_SFONDO=x11, che l'app stessa
/// mette quando torna su X11 perché l'estensione non va): niente giri avanti e indietro.
fn riparti_su_wayland() {
    use std::os::unix::process::CommandExt;
    zoo_log!("sfondo: l'estensione di GNOME ora risponde ({} ms dall'avvio): riparto senza XWayland", shared::ms_dall_avvio());
    let exe = std::env::current_exe().ok().filter(|p| p.exists()).unwrap_or_else(|| "/proc/self/exe".into());
    let errore = std::process::Command::new(exe).args(std::env::args_os().skip(1)).env_remove("GDK_BACKEND").exec();
    eprintln!("ascii-zoo: non riesco a ripartire: {errore}");
}

fn riparti_su_x11(motivo: &str) {
    use std::os::unix::process::CommandExt;
    zoo_log!("sfondo: {motivo}: riparto su X11 (XWayland)");
    let exe = std::env::current_exe().ok().filter(|p| p.exists()).unwrap_or_else(|| "/proc/self/exe".into());
    let errore = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("ASCII_ZOO_SFONDO", "x11")
        .env_remove("GDK_BACKEND")
        .exec();
    eprintln!("ascii-zoo: non riesco a ripartire su X11: {errore}");
    std::process::exit(1);
}

/// Clic sul desktop. Sotto Wayland un programma non vede i clic fatti sulle altre finestre,
/// e su Ubuntu le icone del desktop (DING) stanno in una finestra a tutto schermo sopra lo
/// sfondo: i clic li vede solo GNOME Shell. L'estensione `zoo-click@tressilloide.github.io`
/// (linux/gnome) aspetta che questo nome compaia sul bus di sessione e, a ogni clic sinistro
/// su una zona del desktop, chiama Clic(x, y, larghezza, altezza): il punto e le dimensioni
/// dello schermo intero di GNOME. Il nome lo si tiene solo con "Clic sul desktop" acceso,
/// così altrimenti l'estensione non fa niente.
const DBUS_NOME: &str = "io.github.tressilloide.AsciiZoo";
const DBUS_PERCORSO: &str = "/io/github/tressilloide/AsciiZoo";
const DBUS_XML: &str = r#"<node>
  <interface name="io.github.tressilloide.AsciiZoo">
    <method name="Clic">
      <arg type="d" name="x" direction="in"/>
      <arg type="d" name="y" direction="in"/>
      <arg type="d" name="larghezza" direction="in"/>
      <arg type="d" name="altezza" direction="in"/>
    </method>
  </interface>
</node>"#;

/// mette sul bus l'oggetto con il metodo Clic (il nome si prende a parte, vedi `sync_clic`)
fn registra_clic(bus: &gio::DBusConnection, proxy: &EventLoopProxy<Evento>) -> bool {
    let Ok(nodo) = gio::DBusNodeInfo::for_xml(DBUS_XML) else { return false };
    let Some(interfaccia) = nodo.lookup_interface(DBUS_NOME) else { return false };
    let proxy = std::sync::Mutex::new(proxy.clone());
    bus.register_object(
        DBUS_PERCORSO,
        &interfaccia,
        move |_, _, _, _, metodo, parametri, risposta| {
            if metodo == "Clic" {
                if let (Some((x, y, w, h)), Ok(p)) = (parametri.get::<(f64, f64, f64, f64)>(), proxy.lock()) {
                    let _ = p.send_event(Evento::Clic(x, y, w, h));
                }
            }
            risposta.return_value(None);
        },
        |_, _, _, _, _| glib::Variant::from(()),
        |_, _, _, _, _, _| false,
    )
    .is_ok()
}

/// lo sfondo di uno schermo (prima la WebView, poi la sua finestra: i campi si
/// distruggono in quest'ordine)
struct Screen {
    webview: WebView,
    window: Window,
    pos: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    scala: f64,
    /// lo schermo in pixel logici (quelli di GNOME e della pagina): x, y, larghezza, altezza
    logico: [i32; 4],
    /// nome e posizione dello schermo: le impostazioni per schermo si legano a questo
    id: String,
    paused: bool,
    /// ultimi margini mandati alla pagina (barra in alto, dock…)
    margini: Option<[i32; 4]>,
    sonda: Sonda,
    /// l'ultima risposta della sonda: un'app a schermo intero (o finestre) copre tutto
    coperto: bool,
    /// con ASCII_ZOO_LOG: quante volte GTK ha ridisegnato la finestra, e l'ultimo numero di
    /// fotogrammi della pagina (per scrivere nel log quanti al secondo, vedi `log_fotogrammi`)
    disegni: Rc<Cell<u32>>,
    fotogrammi_pagina: std::sync::Arc<std::sync::Mutex<Option<(i64, Instant)>>>,
}

impl Screen {
    /// il centro dello schermo in pixel "logici", quelli di GDK (e della pagina)
    fn centro_logico(&self) -> (i32, i32) {
        let sc = if self.scala > 0.0 { self.scala } else { 1.0 };
        (
            ((self.pos.x as f64 + self.size.width as f64 / 2.0) / sc) as i32,
            ((self.pos.y as f64 + self.size.height as f64 / 2.0) / sc) as i32,
        )
    }
}

/// Capisce se uno schermo è occupato per intero da un'altra finestra (un'app a schermo
/// intero). Sotto Wayland un'app X11 non vede le finestre native che le stanno sopra,
/// ma mutter (GNOME), quando una finestra X11 è nascosta del tutto, le manda i messaggi
/// "fotogramma disegnato" (_NET_WM_FRAME_DRAWN) a un sesto della frequenza dello
/// schermo: il frame clock di GTK passa da ~60 a ~10 giri al secondo. La sonda mette in
/// moto il frame clock e a ogni giro ridisegna un pixel (senza niente da disegnare GTK
/// non aspetta il compositor e gira sempre a 60). Con le finestre solo massimizzate non
/// scatta: per mutter la striscia sotto la barra in alto e la dock resta visibile.
/// Dove il compositor non rallenta (altri desktop) semplicemente non scatta mai, e
/// "lento" vale come "coperto" solo dopo aver visto almeno una volta il ritmo pieno:
/// con un compositor che rallentasse sempre lo sfondo non resta fermo.
///
/// Misura a intervalli (MISURA ogni OGNI_MISURA), non sempre: ogni giro forzato fa
/// ridipingere a GTK tutta la finestra con la WebView (con OpenGL) e Xwayland la passa a
/// GNOME. Tenuta accesa sempre erano 60 ridisegni al secondo anche con lo sfondo a 15
/// fotogrammi o fermo: su un portatile con la scala di GNOME al 125% (finestra di
/// 3072×1728) ~0,6 core di Xwayland, e con la GPU piena le misure lente facevano andare e
/// venire la pausa. Il ritmo si conta dal primo all'ultimo giro: il frame clock fermo ci
/// mette un po' a ripartire, e contando anche quell'attesa lo sfondo sembrava coperto.
#[derive(Default)]
struct Sonda {
    /// giri della misura in corso: (quanti, primo, ultimo)
    giri: Rc<Cell<(u32, Option<Instant>, Option<Instant>)>>,
    /// il giro del frame clock, mentre si misura
    id: Rc<RefCell<Option<gtk::TickCallbackId>>>,
    /// l'ultima misura finita: giri al secondo
    misura: Rc<Cell<Option<f64>>>,
    /// quando parte la prossima misura
    prossima: Option<Instant>,
    /// misure di fila con il frame clock rallentato
    lenti: u32,
    /// il frame clock ha girato almeno una volta a ritmo pieno
    visto_veloce: bool,
}

/// Quanto dura una misura, e ogni quanto se ne fa una: con lo sfondo visibile di rado (ogni
/// misura sono 30 ridisegni della finestra intera, e su un portatile con la scala di GNOME
/// al 125% anche ogni 2 secondi Xwayland restava a ~0,5 core con 15 o 30 fotogrammi); dopo
/// una misura lenta, o con lo sfondo coperto, spesso, per confermare e per ripartire presto.
/// Un'app a schermo intero la si vede così entro ~20 secondi, e quando si chiude lo sfondo
/// riparte entro ~2.
const MISURA: Duration = Duration::from_millis(500);
const OGNI_MISURA: Duration = Duration::from_secs(10);
const OGNI_MISURA_DUBBIO: Duration = Duration::from_secs(2);

impl Sonda {
    /// true se la finestra è nascosta del tutto da più di una misura
    fn coperto(&mut self, gw: &gtk::ApplicationWindow, refresh_hz: f64) -> bool {
        // coperta, mutter la fa andare a un sesto della frequenza dello schermo (~10 giri al
        // secondo a 60 Hz); visibile, anche con la GPU carica, ne fa ben più di un quarto
        if let Some(al_secondo) = self.misura.take() {
            if al_secondo < refresh_hz / 4.0 {
                self.lenti += 1;
            } else {
                self.lenti = 0;
                self.visto_veloce = true;
            }
        }
        let ora = Instant::now();
        if self.id.borrow().is_none() && self.prossima.is_none_or(|p| ora >= p) {
            self.prossima = Some(ora + if self.lenti > 0 { OGNI_MISURA_DUBBIO } else { OGNI_MISURA });
            self.giri.set((0, None, None));
            let giri = self.giri.clone();
            *self.id.borrow_mut() = Some(gw.add_tick_callback(move |w, _| {
                let (n, primo, _) = giri.get();
                let ora = Instant::now();
                giri.set((n + 1, primo.or(Some(ora)), Some(ora)));
                w.queue_draw_area(0, 0, 1, 1);
                glib::ControlFlow::Continue
            }));
            let (giri, id, misura) = (self.giri.clone(), self.id.clone(), self.misura.clone());
            glib::timeout_add_local_once(MISURA, move || {
                if let Some(id) = id.borrow_mut().take() {
                    id.remove();
                    // con meno di 3 giri la misura non dice niente (anche coperta, la finestra
                    // ne fa ~5 in mezzo secondo): si aspetta la prossima
                    if let (n, Some(primo), Some(ultimo)) = giri.get() {
                        let dt = (ultimo - primo).as_secs_f64();
                        if n >= 3 && dt > 0.0 {
                            misura.set(Some((n - 1) as f64 / dt));
                        }
                    }
                }
            });
        }
        self.visto_veloce && self.lenti >= 2
    }

    /// quando non serve (in pausa per altri motivi) il frame clock si lascia stare
    fn spegni(&mut self) {
        if let Some(id) = self.id.borrow_mut().take() {
            id.remove();
        }
        self.misura.set(None);
        self.prossima = None;
        self.lenti = 0;
    }
}

/// Modo Wayland: i messaggi dell'estensione di GNOME (stato degli schermi) e di GNOME Shell
/// (l'estensione spenta: si riparte su X11)
fn ascolta_estensione(bus: &gio::DBusConnection, proxy: &EventLoopProxy<Evento>) {
    let p = std::sync::Mutex::new(proxy.clone());
    bus.signal_subscribe(
        Some("org.gnome.Shell"),
        Some(SFONDO_INTERFACCIA),
        Some("Cambiato"),
        Some(SFONDO_PERCORSO),
        None,
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, parametri| {
            if let (Some(stato), Ok(p)) = (leggi_stato(parametri), p.lock()) {
                let _ = p.send_event(Evento::Shell(stato));
            }
        },
    );
    let p = std::sync::Mutex::new(proxy.clone());
    bus.signal_subscribe(
        Some("org.gnome.Shell"),
        Some("org.gnome.Shell.Extensions"),
        Some("ExtensionStateChanged"),
        Some("/org/gnome/Shell"),
        Some(ESTENSIONE),
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, parametri| {
            // (uuid, informazioni): "state" 1 = accesa
            let Some((uuid, info)) = parametri.get::<(String, std::collections::HashMap<String, glib::Variant>)>() else { return };
            let stato = info.get("state").and_then(|v| v.get::<f64>());
            if uuid == ESTENSIONE {
                if let (Some(s), Ok(p)) = (stato, p.lock()) {
                    let _ = p.send_event(if s == 1.0 { Evento::EstensioneAccesa } else { Evento::EstensioneSpenta });
                }
            }
        },
    );
}

/// Schermo bloccato o spento (salvaschermo di GNOME, o quello standard di KDE e altri):
/// lo sfondo non lo vede nessuno.
fn schermo_bloccato(bus: Option<&gio::DBusConnection>) -> bool {
    let Some(bus) = bus else { return false };
    let Ok(tipo) = glib::VariantTy::new("(b)") else { return false };
    for (nome, percorso) in
        [("org.gnome.ScreenSaver", "/org/gnome/ScreenSaver"), ("org.freedesktop.ScreenSaver", "/org/freedesktop/ScreenSaver")]
    {
        let risposta = bus.call_sync(
            Some(nome),
            percorso,
            nome,
            "GetActive",
            None,
            Some(tipo),
            gio::DBusCallFlags::NO_AUTO_START,
            200,
            gio::Cancellable::NONE,
        );
        if let Some((attivo,)) = risposta.ok().and_then(|v| v.get::<(bool,)>()) {
            return attivo;
        }
    }
    false
}

struct App {
    core: Core,
    screens: Vec<Screen>,
    tray: Option<TrayIcon>,
    last_ac: Option<bool>,
    stats: Stats,
    /// bus di sessione, per sapere se lo schermo è bloccato e per i clic sul desktop
    bus: Option<gio::DBusConnection>,
    /// il nome DBUS_NOME, tenuto mentre "Clic sul desktop" è acceso
    nome_clic: Option<gio::OwnerId>,
    /// lo screensaver: quando farlo partire, e il suo processo
    salva: salvaschermo::Salvaschermo,
    /// schermo bloccato o spento, all'ultimo controllo
    bloccato: bool,
    /// modo Wayland (vedi l'inizio del file)
    wayland: bool,
    /// modo Wayland: gli schermi come li vede GNOME Shell, dall'estensione
    shell: Vec<StatoSchermo>,
    /// modo Wayland: da quando si aspetta che l'estensione prenda le finestre
    da_adottare: Option<Instant>,
    /// modo Wayland: da quando l'estensione risulta spenta (vedi ATTESA_SPENTA)
    spenta_da: Option<Instant>,
    /// su X11 solo perché l'estensione all'avvio non rispondeva: si passa al modo Wayland
    /// appena risponde (vedi riparti_su_wayland)
    x11_in_attesa: bool,
}

impl App {
    fn js(&self, code: &str) {
        for s in &self.screens {
            let _ = s.webview.evaluate_script(code);
        }
    }

    fn push_settings(&mut self) {
        let ac = power::on_ac();
        self.last_ac = Some(ac);
        for s in &mut self.screens {
            shared::log_pagina_pronta(&s.webview, &s.id);
            for code in self.core.js_tutto(ac && power::ha_batteria(), &s.id) {
                let _ = s.webview.evaluate_script(&code);
            }
            if s.paused {
                let _ = s.webview.evaluate_script(JS_STOP);
            }
            // una pagina (ri)caricata non sa niente dei margini
            s.margini = None;
        }
        self.check_margini();
    }

    /// Con ASCII_ZOO_LOG, ogni 10 secondi: quante volte al secondo GTK ha davvero ridisegnato
    /// la finestra di ogni schermo e quanti fotogrammi ha fatto la pagina. Serve a capire,
    /// dal rapporto di prestazioni.sh, se qualcosa ridisegna lo sfondo più spesso del dovuto.
    fn log_fotogrammi(&mut self, da: Instant) {
        let secondi = da.elapsed().as_secs_f64().max(0.001);
        for s in &self.screens {
            let disegni = s.disegni.replace(0);
            if let Some(x) = s.window.gtk_window().window().and_then(|w| w.downcast::<gdkx11::X11Window>().ok()) {
                zoo_log!("schermo {}: finestra X11 0x{:x}", s.id, x.xid());
            }
            zoo_log!("schermo {}: finestra ridisegnata {:.1} volte al secondo", s.id, disegni as f64 / secondi);
            let (id, prima) = (s.id.clone(), s.fotogrammi_pagina.clone());
            let _ = s.webview.evaluate_script_with_callback(
                "window.asciiZoo ? window.asciiZoo.frameNo : -1",
                move |r| {
                    let Ok(n) = r.trim().parse::<i64>() else { return };
                    let ora = Instant::now();
                    let Ok(mut p) = prima.lock() else { return };
                    if let Some((n0, t0)) = p.replace((n, ora)) {
                        let dt = (ora - t0).as_secs_f64();
                        if n >= n0 && dt > 0.5 {
                            zoo_log!("schermo {id}: la pagina ha fatto {:.1} fotogrammi al secondo", (n - n0) as f64 / dt);
                        }
                    }
                },
            );
        }
    }

    /// Modo Wayland, a ogni giro: l'estensione deve prendere le finestre entro
    /// ATTESA_ESTENSIONE, se no resterebbero finestre normali davanti al desktop e si riparte
    /// su X11
    /// su X11 in attesa dell'estensione: risponde adesso?
    fn prova_wayland(&self) {
        if self.x11_in_attesa && self.bus.as_ref().and_then(chiedi_stato).is_some() {
            riparti_su_wayland();
        }
    }

    fn giro_wayland(&mut self) {
        if let Some(da) = self.spenta_da {
            if da.elapsed() < ATTESA_SPENTA {
                return;
            }
            match self.bus.as_ref().and_then(chiedi_stato) {
                Some(stato) => {
                    zoo_log!("sfondo: l'estensione di GNOME è di nuovo accesa");
                    self.spenta_da = None;
                    self.shell = stato;
                    // riaccesa, riprende le finestre: si ricontrolla come all'inizio
                    self.da_adottare = Some(Instant::now());
                }
                None => riparti_su_x11("l'estensione di GNOME è stata spenta"),
            }
            return;
        }
        let Some(da) = self.da_adottare else { return };
        let tutte = !self.screens.is_empty()
            && self.screens.iter().all(|s| stato_di(&self.shell, &s.logico).is_some_and(|st| st.adottata));
        if tutte {
            zoo_log!("sfondo: l'estensione ha messo le finestre al posto dello sfondo");
            self.da_adottare = None;
        } else if da.elapsed() > ATTESA_ESTENSIONE {
            riparti_su_x11("l'estensione di GNOME non ha preso le finestre");
        } else if let Some(stato) = self.bus.as_ref().and_then(chiedi_stato) {
            // il segnale Cambiato può essere partito prima che ci fossimo: si chiede
            self.shell = stato;
        }
    }

    /// manda a ogni schermo le impostazioni che dipendono dallo schermo
    fn push_schermi(&self) {
        for s in &self.screens {
            for code in self.core.js_schermo(&s.id) {
                let _ = s.webview.evaluate_script(&code);
            }
        }
    }

    /// Pausa schermo per schermo: a batteria, con lo schermo bloccato o spento, mentre
    /// gira lo screensaver, con un'app a schermo intero o finestre che coprono tutto lo
    /// sfondo. Da qui le ultime due non si distinguono (su Wayland non si vedono le
    /// finestre delle altre app): basta una delle due voci del menu.
    fn check_pause(&mut self) {
        let cfg = &self.core.cfg;
        let batteria = cfg.pausa_batteria && self.last_ac == Some(false);
        let bloccato = schermo_bloccato(self.bus.as_ref());
        self.bloccato = bloccato;
        let salvaschermo = self.salva.in_corso();
        // modo Wayland: schermo intero e finestre che coprono li dice l'estensione, e qui si
        // distinguono (come su Windows e macOS)
        if self.wayland {
            for s in &mut self.screens {
                let st = stato_di(&self.shell, &s.logico);
                let (intero, coperto) = st.map_or((false, false), |st| (st.intero, st.coperto));
                s.coperto = intero || coperto;
                let nascosto = (intero && cfg.pausa_schermo_intero) || (coperto && cfg.pausa_coperto);
                let pause = batteria || bloccato || salvaschermo || nascosto;
                if pause != s.paused {
                    s.paused = pause;
                    zoo_log!(
                        "schermo {}: {} (batteria {batteria}, bloccato {bloccato}, screensaver {salvaschermo}, schermo intero {intero}, coperto {coperto})",
                        s.id,
                        if pause { "in pausa" } else { "riparte" }
                    );
                    let _ = s.webview.evaluate_script(if pause { JS_STOP } else { JS_START });
                }
            }
            return;
        }
        let display = gtk::gdk::Display::default();
        let nascosto_conta = cfg.pausa_schermo_intero || cfg.pausa_coperto;
        // la sonda serve se lo sfondo coperto è l'unico motivo di pausa che resta, e allo
        // screensaver, per non partire sopra un'app a schermo intero
        let sonda = (nascosto_conta || cfg.salvaschermo_minuti > 0) && !batteria && !bloccato && !salvaschermo;
        for s in &mut self.screens {
            s.coperto = if sonda {
                let (cx, cy) = s.centro_logico();
                let refresh = display
                    .as_ref()
                    .and_then(|d| d.monitor_at_point(cx, cy))
                    .map(|m| m.refresh_rate())
                    .filter(|&mhz| mhz > 0)
                    .map_or(60.0, |mhz| mhz as f64 / 1000.0);
                s.sonda.coperto(s.window.gtk_window(), refresh)
            } else {
                s.sonda.spegni();
                false
            };
            let coperto = s.coperto && nascosto_conta;
            let pause = batteria || bloccato || salvaschermo || coperto;
            if pause != s.paused {
                s.paused = pause;
                zoo_log!(
                    "schermo {}: {} (batteria {batteria}, bloccato {bloccato}, screensaver {salvaschermo}, coperto {coperto})",
                    s.id,
                    if pause { "in pausa" } else { "riparte" }
                );
                let _ = s.webview.evaluate_script(if pause { JS_STOP } else { JS_START });
            }
        }
    }

    fn check_power(&mut self) {
        // "Tema > Come il sistema": il tema del sistema è cambiato
        if let Some(js) = self.core.tema_cambiato() {
            self.js(&js);
        }
        let ac = power::on_ac();
        if self.last_ac != Some(ac) {
            self.last_ac = Some(ac);
            self.js(&shared::js_in_carica(ac && power::ha_batteria()));
        }
        self.check_pause();
        // widget: nuovi valori ogni 2 secondi (statistiche solo se si vedono)
        for code in self.core.giro(&mut self.stats, self.screens.iter().any(|s| !s.paused)) {
            self.js(&code);
        }
    }

    /// Quanto dello schermo occupano la barra in alto di GNOME, il dock e simili
    /// (area di lavoro più piccola del monitor): lo si dice alla pagina, così i widget
    /// non ci finiscono sotto. Si manda solo quando cambia.
    fn check_margini(&mut self) {
        let Some(display) = gtk::gdk::Display::default() else { return };
        for s in &mut self.screens {
            // modo Wayland: GTK non sa l'area di lavoro, la dice l'estensione
            let (g, w) = if self.wayland {
                let Some(st) = stato_di(&self.shell, &s.logico) else { continue };
                (st.geo, st.lavoro)
            } else {
                let (cx, cy) = s.centro_logico();
                let Some(mon) = display.monitor_at_point(cx, cy) else { continue };
                let (g, w) = (mon.geometry(), mon.workarea());
                ([g.x(), g.y(), g.width(), g.height()], [w.x(), w.y(), w.width(), w.height()])
            };
            let m = [w[1] - g[1], (g[0] + g[2]) - (w[0] + w[2]), (g[1] + g[3]) - (w[1] + w[3]), w[0] - g[0]].map(|v| v.max(0));
            if s.margini == Some(m) {
                continue;
            }
            s.margini = Some(m);
            let _ = s.webview.evaluate_script(&format!(
                "window.zooSetting && zooSetting('margini', {{alto: {}, destra: {}, basso: {}, sinistra: {}}})",
                m[0], m[1], m[2], m[3]
            ));
        }
    }

    /// prende o lascia il nome sul bus secondo l'impostazione "Clic sul desktop"
    fn sync_clic(&mut self) {
        let voglio = self.core.cfg.clic_desktop;
        match (voglio, self.nome_clic.is_some(), self.bus.as_ref()) {
            (true, false, Some(bus)) => {
                self.nome_clic = Some(gio::bus_own_name_on_connection(
                    bus,
                    DBUS_NOME,
                    gio::BusNameOwnerFlags::NONE,
                    |_, _| zoo_log!("clic sul desktop: nome {DBUS_NOME} preso"),
                    |_, _| zoo_log!("clic sul desktop: nome {DBUS_NOME} non disponibile"),
                ));
            }
            (false, true, _) => {
                if let Some(id) = self.nome_clic.take() {
                    gio::bus_unown_name(id);
                }
            }
            _ => {}
        }
    }

    /// Clic sul desktop nel punto (x, y) dello schermo intero di GNOME, largo `w` e alto `h`.
    /// Per XWayland lo stesso schermo intero è il riquadro che contiene tutti gli sfondi
    /// (a un'altra scala, se GNOME disegna le app X11 più grandi): si passa in proporzione e
    /// si trova lo sfondo sotto il punto. La pagina ragiona in pixel CSS: si divide per la
    /// scala dello schermo.
    fn clic(&self, x: f64, y: f64, w: f64, h: f64) {
        if !self.core.cfg.clic_desktop || !(w > 0.0 && h > 0.0) || !x.is_finite() || !y.is_finite() {
            return;
        }
        // modo Wayland: il punto è già in pixel logici di GNOME, gli stessi della pagina
        if self.wayland {
            let Some(s) = self.screens.iter().find(|s| {
                let [lx, ly, lw, lh] = s.logico.map(f64::from);
                x >= lx && y >= ly && x < lx + lw && y < ly + lh
            }) else {
                return;
            };
            if s.paused {
                return;
            }
            let (cx, cy) = (x - s.logico[0] as f64, y - s.logico[1] as f64);
            zoo_log!("clic sul desktop: {x:.0},{y:.0} → schermo {} ({cx:.0}, {cy:.0})", s.id);
            let _ = s.webview.evaluate_script(&format!("window.zooPoke && zooPoke({cx:.1}, {cy:.1})"));
            return;
        }
        let x0 = self.screens.iter().map(|s| s.pos.x).min().unwrap_or(0) as f64;
        let y0 = self.screens.iter().map(|s| s.pos.y).min().unwrap_or(0) as f64;
        let x1 = self.screens.iter().map(|s| s.pos.x + s.size.width as i32).max().unwrap_or(0) as f64;
        let y1 = self.screens.iter().map(|s| s.pos.y + s.size.height as i32).max().unwrap_or(0) as f64;
        let (px, py) = (x0 + x / w * (x1 - x0), y0 + y / h * (y1 - y0));
        let Some(s) = self.screens.iter().find(|s| {
            px >= s.pos.x as f64
                && py >= s.pos.y as f64
                && px < (s.pos.x + s.size.width as i32) as f64
                && py < (s.pos.y + s.size.height as i32) as f64
        }) else {
            return;
        };
        if s.paused {
            return;
        }
        let sc = if s.scala > 0.0 { s.scala } else { 1.0 };
        let (cx, cy) = ((px - s.pos.x as f64) / sc, (py - s.pos.y as f64) / sc);
        zoo_log!("clic sul desktop: {x:.0},{y:.0} su {w:.0}×{h:.0} → schermo {} ({cx:.0}, {cy:.0})", s.id);
        let _ = s.webview.evaluate_script(&format!("window.zooPoke && zooPoke({cx:.1}, {cy:.1})"));
    }

    /// in fondo alla pila, anche sotto la finestra delle icone del desktop (che su
    /// GNOME è anche lei di tipo "desktop")
    fn tieni_sotto(&self) {
        if self.wayland {
            return; // ci pensa l'estensione
        }
        for s in &self.screens {
            if let Some(w) = s.window.gtk_window().window() {
                w.lower();
            }
        }
    }

    /// se gli schermi sono cambiati (collegati, scollegati, risoluzione) rifà gli sfondi
    fn check_screens(&mut self, target: &EventLoopWindowTarget<Evento>, proxy: &EventLoopProxy<Evento>) {
        // modo Wayland: i connettori dall'estensione, se no da mutter
        let mutter = if self.wayland && self.shell.iter().any(|st| st.connettore.is_empty()) {
            self.bus.as_ref().map(connettori_mutter).unwrap_or_default()
        } else {
            Vec::new()
        };
        let now = schermi(target, self.wayland.then_some(self.shell.as_slice()), &mutter);
        self.core.set_schermi(now.iter().map(|m| m.3.clone()).collect());
        // anche la scala: GNOME può cambiarla dopo che le finestre ci sono (all'accesso l'app
        // può partire prima che arrivi la scala dello schermo, ed è 1; poi diventa 2). La
        // misura in pixel del monitor resta la stessa, ma GTK tiene quella logica della
        // finestra, che così diventa grande il doppio: si vedeva solo il quarto in alto a
        // sinistra dello sfondo, con l'orologio enorme nell'angolo in basso a destra
        let same = now.len() == self.screens.len()
            && self.screens.iter().all(|s| {
                now.iter().any(|m| m.0 == s.pos && m.1 == s.size && m.3.id == s.id && (m.2 - s.scala).abs() < 0.01)
            });
        if !same {
            for s in &self.screens {
                if let Some(m) = now.iter().find(|m| m.3.id == s.id && (m.2 - s.scala).abs() >= 0.01) {
                    zoo_log!("schermo {}: la scala è passata da {} a {}, rifaccio lo sfondo", s.id, s.scala, m.2);
                }
            }
            self.screens.clear();
            self.screens = now
                .into_iter()
                .filter_map(|(pos, size, scala, sc)| {
                    let iniziale = self.core.script_iniziale(&sc.id);
                    make_screen(target, proxy, pos, size, scala, sc.id, self.wayland, &iniziale)
                })
                .collect();
            for s in &self.screens {
                zoo_log!("schermo {}: {}×{} a {},{}, scala {}", s.id, s.size.width, s.size.height, s.pos.x, s.pos.y, s.scala);
            }
            if self.wayland {
                // l'estensione deve prendere le finestre nuove: si controlla in `giro_wayland`
                self.da_adottare = Some(Instant::now());
            }
        }
    }
}

/// posizione e dimensione (pixel fisici) e scala di ogni schermo, il principale per primo.
/// Il nome è quello del connettore ("eDP-1"): sotto X11 lo dà GTK, sotto Wayland GTK 3 dà solo
/// il modello e il connettore lo dice l'estensione di GNOME (`shell`), così le impostazioni
/// per schermo restano le stesse nei due modi.
fn schermi(
    target: &EventLoopWindowTarget<Evento>,
    shell: Option<&[StatoSchermo]>,
    mutter: &[([i32; 2], String)],
) -> Vec<(PhysicalPosition<i32>, PhysicalSize<u32>, f64, Schermo)> {
    let principale = target.primary_monitor().map(|m| m.position());
    let mut v: Vec<_> = target
        .available_monitors()
        .map(|m| {
            let (pos, size) = (m.position(), m.size());
            let geo = logico(pos, size, m.scale_factor());
            let connettore = shell
                .and_then(|sh| stato_di(sh, &geo))
                .map(|st| st.connettore.clone())
                .filter(|c| !c.is_empty())
                .or_else(|| mutter.iter().find(|(xy, _)| *xy == [geo[0], geo[1]]).map(|(_, c)| c.clone()))
                .filter(|c| !c.is_empty());
            let nome = connettore.or_else(|| m.name()).unwrap_or_else(|| "schermo".into());
            let id = format!("{nome}@{},{}", pos.x, pos.y);
            (pos, size, m.scale_factor(), Schermo { id, larghezza: size.width, altezza: size.height })
        })
        .collect();
    v.sort_by_key(|(pos, _, _, _)| (Some(*pos) != principale, pos.x, pos.y));
    v
}

/// lo schermo in pixel logici, da posizione e misura in pixel fisici
fn logico(pos: PhysicalPosition<i32>, size: PhysicalSize<u32>, scala: f64) -> [i32; 4] {
    let sc = if scala > 0.0 { scala } else { 1.0 };
    [pos.x as f64, pos.y as f64, size.width as f64, size.height as f64].map(|v| (v / sc).round() as i32)
}

/// lo stato che GNOME Shell dà per uno schermo: quello con la stessa geometria, se no quello
/// che contiene il suo centro (la scala è appena cambiata e la finestra non è ancora rifatta)
fn stato_di<'a>(shell: &'a [StatoSchermo], logico: &[i32; 4]) -> Option<&'a StatoSchermo> {
    shell.iter().find(|st| st.geo == *logico).or_else(|| {
        let (cx, cy) = (logico[0] + logico[2] / 2, logico[1] + logico[3] / 2);
        shell.iter().find(|st| cx >= st.geo[0] && cy >= st.geo[1] && cx < st.geo[0] + st.geo[2] && cy < st.geo[1] + st.geo[3])
    })
}

fn make_screen(
    target: &EventLoopWindowTarget<Evento>,
    proxy: &EventLoopProxy<Evento>,
    pos: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    scala: f64,
    id: String,
    wayland: bool,
    iniziale: &str,
) -> Option<Screen> {
    let logico = logico(pos, size, scala);
    // modo Wayland: il titolo dice all'estensione di GNOME che è lo sfondo e di quale
    // schermo; la misura è in pixel logici (sotto Wayland GTK non sa ancora la scala)
    let builder = WindowBuilder::new().with_decorations(false).with_resizable(false).with_visible(false);
    let builder = if wayland {
        let [x, y, w, h] = logico;
        builder
            .with_title(format!("{TITOLO_WAYLAND}{x},{y},{w},{h}"))
            .with_inner_size(tao::dpi::LogicalSize::new(w as f64, h as f64))
    } else {
        builder.with_title("ASCII Zoo").with_position(pos).with_inner_size(size)
    };
    let window = builder.build(target).ok()?;
    // va detto prima che la finestra compaia: il gestore delle finestre lo legge allora
    let gw = window.gtk_window();
    gw.set_type_hint(gtk::gdk::WindowTypeHint::Desktop);
    gw.set_skip_taskbar_hint(true);
    gw.set_skip_pager_hint(true);
    gw.set_accept_focus(false);
    gw.set_focus_on_map(false);
    gw.set_keep_below(true);
    gw.stick();
    // la WebView copre tutta la finestra: GTK non deve ridipingere a ogni fotogramma lo
    // sfondo della finestra sotto di lei (in software, una superficie grande quanto lo
    // schermo: con la scala di GNOME al 125%, 3072×1728). Misurato con GNOME Shell 51 e
    // una GPU Intel: il processo dell'app da 0,26 a 0,13 core
    gw.set_app_paintable(true);
    let disegni = Rc::new(Cell::new(0u32));
    if shared::log_attivo() {
        let d = disegni.clone();
        gw.connect_draw(move |_, _| {
            d.set(d.get() + 1);
            glib::Propagation::Proceed
        });
    }

    let pronta = proxy.clone();
    let webview = WebViewBuilder::new()
        .with_custom_protocol("zoo".into(), |_id, _req| shared::pagina())
        .with_navigation_handler(shared::navigazione_permessa)
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_url(URL)
        .with_initialization_script(iniziale)
        .with_background_color((9, 10, 15, 255))
        .with_devtools(false)
        .with_on_page_load_handler(move |ev, _url| {
            if let PageLoadEvent::Finished = ev {
                let _ = pronta.send_event(Evento::PaginaPronta);
            }
        })
        .build_gtk(window.default_vbox()?)
        .ok()?;
    window.set_visible(true);
    zoo_log!("avvio: schermo {id}: finestra e WebView pronte a {} ms dall'avvio", shared::ms_dall_avvio());
    // trasparente ai clic: vanno alle icone del desktop o a quello che c'è sotto
    gw.input_shape_combine_region(Some(&gtk::cairo::Region::create()));
    if let Some(w) = gw.window().filter(|_| !wayland) {
        w.lower();
    }
    Some(Screen {
        webview,
        window,
        pos,
        size,
        scala,
        logico,
        id,
        paused: false,
        margini: None,
        sonda: Sonda::default(),
        coperto: false,
        disegni,
        fotogrammi_pagina: Default::default(),
    })
}

/// Una sola copia per utente: un socket "astratto" (non è un file, sparisce da solo
/// quando il programma si chiude). Se il nome è già preso c'è già un ASCII Zoo, magari una
/// versione vecchia: lo si chiude e questa copia, appena avviata, prende il suo posto.
/// Si chiude solo il processo che tiene proprio quel socket (lo usano tutte le versioni).
fn una_sola_copia() -> Option<UnixListener> {
    let utente = std::env::var("USER").unwrap_or_default();
    let nome = format!("ascii-zoo-{utente}");
    let addr = SocketAddr::from_abstract_name(&nome).ok()?;
    if let Ok(l) = UnixListener::bind_addr(&addr) {
        return Some(l);
    }
    let vecchi = chi_tiene(&nome);
    zoo_log!("c'è già un ASCII Zoo (pid {vecchi:?}): lo chiudo e prendo il suo posto");
    // prima gli si chiede di uscire, poi, se dopo 3 secondi c'è ancora, lo si chiude per forza
    for segnale in [libc::SIGTERM, libc::SIGKILL] {
        for &pid in &vecchi {
            unsafe { libc::kill(pid, segnale) };
        }
        for _ in 0..30 {
            std::thread::sleep(Duration::from_millis(100));
            if let Ok(l) = UnixListener::bind_addr(&addr) {
                return Some(l);
            }
        }
    }
    zoo_log!("non riesco a prendere il posto dell'altra copia: esco");
    None
}

/// I processi che tengono aperto il socket astratto `nome`: il suo inode da /proc/net/unix,
/// poi chi ha quel socket tra i suoi file aperti (si leggono solo i processi dell'utente).
fn chi_tiene(nome: &str) -> Vec<i32> {
    let cercato = format!("@{nome}");
    let socket: Vec<String> = std::fs::read_to_string("/proc/net/unix")
        .unwrap_or_default()
        .lines()
        .skip(1)
        .filter_map(|riga| {
            let c: Vec<&str> = riga.split_whitespace().collect();
            (c.len() >= 8 && c[7] == cercato).then(|| format!("socket:[{}]", c[6]))
        })
        .collect();
    if socket.is_empty() {
        return Vec::new();
    }
    let mio = std::process::id() as i32;
    let mut pids = Vec::new();
    for voce in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        let Some(pid) = voce.file_name().to_str().and_then(|s| s.parse::<i32>().ok()) else { continue };
        if pid == mio {
            continue;
        }
        let Ok(fd) = std::fs::read_dir(voce.path().join("fd")) else { continue };
        let tiene = fd.flatten().any(|f| {
            std::fs::read_link(f.path()).is_ok_and(|l| socket.iter().any(|s| l.as_os_str() == s.as_str()))
        });
        if tiene {
            pids.push(pid);
        }
    }
    pids
}

pub fn run() {
    let Some(_una_sola) = una_sola_copia() else { return };
    autostart::aggiorna();

    // Wayland non permette a un programma di mettersi al posto dello sfondo: su GNOME lo fa
    // l'estensione (modo Wayland), se no si usa XWayland, dove le finestre di tipo "desktop"
    // funzionano. Si decide prima che parta GTK
    let bus = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok();
    let shell = modo_wayland(bus.as_ref());
    let wayland = shell.is_some();
    // su X11 solo perché l'estensione non risponde (non chiesto): la si aspetta
    let x11_in_attesa = !wayland
        && std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("GDK_BACKEND").is_none()
        && std::env::var_os("ASCII_ZOO_SFONDO").is_none()
        && gnome();
    if wayland {
        std::env::set_var("GDK_BACKEND", "wayland");
        zoo_log!("sfondo: Wayland, al posto dello sfondo lo mette l'estensione di GNOME");
    } else if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        let perche = if std::env::var("ASCII_ZOO_SFONDO").is_ok_and(|v| v == "x11") {
            "chiesto con ASCII_ZOO_SFONDO=x11"
        } else if std::env::var_os("GDK_BACKEND").is_some() {
            "chiesto con GDK_BACKEND"
        } else {
            "l'estensione di GNOME per lo sfondo non risponde"
        };
        if std::env::var_os("GDK_BACKEND").is_none() {
            std::env::set_var("GDK_BACKEND", "x11");
        }
        zoo_log!("sfondo: X11 (XWayland): {perche}");
    }
    // con i driver NVIDIA proprietari WebKitGTK a volte mostra una pagina vuota
    if std::path::Path::new("/proc/driver/nvidia").exists() && std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let event_loop = EventLoopBuilder::<Evento>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = menu_proxy.send_event(Evento::Menu(e.id.0));
    }));

    // un aggiornamento senza uscire e rientrare: GNOME usa ancora l'estensione di prima, e il
    // menu si adatta a lei (vedi shared::estensione_vecchia)
    if let Some(v) = bus.as_ref().and_then(versione_estensione) {
        if v < ESTENSIONE_VERSIONE {
            zoo_log!("menu: GNOME usa ancora l'estensione di prima (versione {v}, questa app vuole la {ESTENSIONE_VERSIONE}): esci e rientra");
            shared::estensione_vecchia();
        }
    }
    // false: niente "Usa come screensaver…" (Windows e macOS); qui c'è il sottomenu Screensaver
    let (core, menu) = Core::new(false);
    if let Some(bus) = &bus {
        if !registra_clic(bus, &proxy) {
            zoo_log!("clic sul desktop: non riesco a registrare l'oggetto D-Bus");
        }
        if wayland || x11_in_attesa {
            ascolta_estensione(bus, &proxy);
        }
    }
    // lo screensaver: il suo sottomenu va dopo "Apri al login"
    let p = std::sync::Mutex::new(proxy.clone());
    let avvisa: std::sync::Arc<dyn Fn(salvaschermo::Segnale) + Send + Sync> = std::sync::Arc::new(move |s| {
        if let Ok(p) = p.lock() {
            let _ = p.send_event(Evento::Salvaschermo(s));
        }
    });
    let salva = salvaschermo::Salvaschermo::new(bus.as_ref(), avvisa, &core.cfg);
    salva.inserisci(&menu, &core.menus.avvio.id().0);
    let mut menu = Some(menu);
    let mut app = App {
        core,
        screens: Vec::new(),
        tray: None,
        last_ac: None,
        stats: Stats::default(),
        bus,
        nome_clic: None,
        salva,
        bloccato: false,
        wayland,
        shell: shell.unwrap_or_default(),
        da_adottare: None,
        spenta_da: None,
        x11_in_attesa,
    };
    app.sync_clic();
    let mut giro: u32 = 0;
    let mut conteggio = Instant::now();
    // sveglia il ciclo ogni mezzo secondo anche con lo sfondo in pausa (vedi prossimo_giro)
    glib::timeout_add_local(GIRO, || glib::ControlFlow::Continue);

    event_loop.run(move |event, target, control_flow| match event {
        Event::NewEvents(StartCause::Init) => {
            app.check_screens(target, &proxy);
            if let Some(menu) = menu.take() {
                // l'id dell'icona ("tray-icon tray app ascii-zoo") fa riconoscere il menu
                // all'estensione di GNOME (linux/gnome/…/menu.js) senza guardare le voci
                app.tray = TrayIconBuilder::new()
                    .with_id("ascii-zoo")
                    .with_menu(Box::new(menu))
                    .with_tooltip("ASCII Zoo")
                    .with_icon(shared::tray_icon_image())
                    .build()
                    .map_err(|e| eprintln!("ascii-zoo: icona nella barra non disponibile: {e}"))
                    .ok();
            }
            *control_flow = ControlFlow::WaitUntil(prossimo_giro());
        }
        Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
            giro = giro.wrapping_add(1);
            app.giro_wayland();
            // "Rotazione > Spenta": la pagina ha detto quale animale mostra
            if app.core.risposta_animale() {
                app.push_schermi();
            }
            // il segnale dell'estensione accesa potrebbe essere arrivato prima che lo si
            // ascoltasse: si chiede comunque dopo 10 e dopo 30 secondi
            if giro == 20 || giro == 60 {
                app.prova_wayland();
            }
            let coperto = app.screens.iter().any(|s| s.coperto);
            app.salva.giro(&app.core.cfg, app.bus.as_ref(), app.bloccato, coperto);
            if giro % GIRI_CONTROLLO == 0 {
                app.check_screens(target, &proxy);
                app.check_power(); // ricontrolla anche le pause
                app.check_margini();
                app.tieni_sotto();
                if shared::log_attivo() && giro % (GIRI_CONTROLLO * 5) == 0 {
                    app.log_fotogrammi(conteggio);
                    conteggio = Instant::now();
                }
            } else {
                app.check_pause();
            }
            *control_flow = ControlFlow::WaitUntil(prossimo_giro());
        }
        Event::UserEvent(Evento::PaginaPronta) => app.push_settings(),
        Event::UserEvent(Evento::Shell(stato)) => {
            if stato != app.shell {
                zoo_log!("sfondo: da GNOME Shell {stato:?}");
                app.shell = stato;
                app.check_pause();
                app.check_margini();
            }
        }
        Event::UserEvent(Evento::EstensioneAccesa) => app.prova_wayland(),
        Event::UserEvent(Evento::EstensioneSpenta) if app.wayland => {
            if app.spenta_da.is_none() {
                zoo_log!("sfondo: l'estensione di GNOME risulta spenta, vediamo se si riaccende");
                app.spenta_da = Some(Instant::now());
            }
        }
        Event::UserEvent(Evento::Clic(x, y, w, h)) => app.clic(x, y, w, h),
        Event::UserEvent(Evento::Salvaschermo(s)) => {
            let coperto = app.screens.iter().any(|s| s.coperto);
            app.salva.segnale(s, &app.core.cfg, app.bus.as_ref(), app.bloccato, coperto);
            app.check_pause();
        }
        Event::UserEvent(Evento::Menu(id)) if app.salva.on_menu(&id, &mut app.core.cfg, app.bus.as_ref()) => {
            app.check_pause();
        }
        Event::UserEvent(Evento::Menu(id)) => match app.core.on_menu(&id) {
            Esito::Cambiato(js) => {
                if let Some(code) = js {
                    app.js(&code);
                }
                app.check_power();
                app.sync_clic();
            }
            Esito::Schermi => app.push_schermi(),
            Esito::Foto => foto::scatta(app.screens.iter().map(|s| &s.webview)),
            Esito::FermaRotazione => app.core.chiedi_animale(app.screens.iter().map(|s| (s.id.as_str(), &s.webview))),
            Esito::Ricarica => {
                for s in &mut app.screens {
                    s.paused = false;
                    let _ = s.webview.load_url(URL);
                }
            }
            Esito::Esci => {
                app.salva.ferma(app.bus.as_ref(), "esco");
                app.screens.clear();
                app.tray = None;
                *control_flow = ControlFlow::Exit;
            }
            Esito::Salvaschermo | Esito::Niente => {} // lo screensaver c'è solo su Windows
        },
        _ => {}
    });
}

#[cfg(test)]
mod prove {
    /// l'estensione nella repo è quella che questa app si aspetta (se no, dopo ogni accesso,
    /// il menu direbbe di uscire e rientrare)
    #[test]
    fn versione_estensione() {
        let js = include_str!("../../linux/gnome/zoo-click@tressilloide.github.io/sfondo.js");
        assert!(js.contains(&format!("export const VERSIONE = {};", super::ESTENSIONE_VERSIONE)));
    }
}
