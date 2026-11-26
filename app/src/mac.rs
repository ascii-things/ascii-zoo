//! macOS.
//!
//! - per ogni schermo una finestra senza bordi con dentro WKWebView, al livello del
//!   desktop: sopra l'immagine di sfondo, sotto le icone del Finder e tutte le finestre;
//!   presente in tutti gli Spazi, non riceve clic e non compare in Mission Control
//! - icona nella barra dei menu, niente icona nel Dock (app "accessory")
//! - ogni mezzo secondo le pause: a batteria e quando lo sfondo non si vede (finestre
//!   che coprono il desktop, app a schermo intero, schermo bloccato o spento: lo dice
//!   macOS con lo stato di occlusione della finestra); animare quello che nessuno vede
//!   costa CPU e GPU per niente
//! - ogni 2 secondi: controllo degli schermi collegati
//! - clic sul desktop: un monitor globale dei clic sinistri (vedi `App::sync_clic`)
//! - lo screensaver: il modulo ASCII Zoo.saver, che l'app porta con sé e installa (vedi
//!   `salvaschermo`)

use std::ptr::NonNull;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSEvent, NSEventMask, NSRunningApplication, NSWindow, NSWindowCollectionBehavior, NSWindowOcclusionState,
};
use objc2_foundation::{ns_string, MainThreadMarker, NSArray, NSDictionary, NSNumber, NSPoint, NSRect, NSString};
use tao::dpi::{PhysicalPosition, PhysicalSize};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS, WindowExtMacOS};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::MenuEvent;
use tray_icon::{TrayIcon, TrayIconBuilder};
use wry::{PageLoadEvent, WebView, WebViewBuilder};

use crate::config::autostart;
use crate::{foto, power};
use crate::stats::Stats;
use crate::shared::{self, zoo_log, Core, Esito, Schermo, JS_START, JS_STOP, URL};

#[path = "salvaschermo_mac.rs"]
mod salvaschermo;
#[path = "sogno_mac.rs"]
mod sogno;

/// ogni quanto si ricontrollano le pause: quando lo sfondo torna visibile riparte subito
const GIRO: Duration = Duration::from_millis(500);
/// ogni quanti giri si controllano batteria e schermi (2 secondi)
const GIRI_CONTROLLO: u32 = 4;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGWindowLevelForKey(key: i32) -> i32;
    /// CFArrayRef di CFDictionaryRef, che valgono anche come NSArray e NSDictionary
    fn CGWindowListCopyWindowInfo(option: u32, relative_to_window: u32) -> *mut NSArray<NSDictionary<NSString, AnyObject>>;
    fn CGRectMakeWithDictionaryRepresentation(dict: *const AnyObject, rect: *mut NSRect) -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> NSRect;
}
/// kCGDesktopWindowLevelKey: il livello dell'immagine di sfondo del desktop
const K_CG_DESKTOP_WINDOW_LEVEL_KEY: i32 = 2;
/// kCGDesktopIconWindowLevelKey: il livello delle icone del Finder sul desktop
const K_CG_DESKTOP_ICON_WINDOW_LEVEL_KEY: i32 = 18;
/// kCGWindowListOptionOnScreenOnly: solo le finestre sullo schermo, dalla più in alto
const K_CG_WINDOW_LIST_ON_SCREEN_ONLY: u32 = 1;
/// kCGWindowListOptionIncludingWindow: solo la finestra indicata
const K_CG_WINDOW_LIST_INCLUDING_WINDOW: u32 = 1 << 3;

enum Evento {
    Menu(String),
    PaginaPronta,
    /// clic sinistro in un'altra app, nel punto (x, y) in coordinate di Cocoa (punti, origine
    /// in basso a sinistra dello schermo principale), con il numero della finestra che lo riceve
    Clic(f64, f64, isize),
}

/// lo sfondo di uno schermo (prima la WebView, poi la sua finestra: i campi si
/// distruggono in quest'ordine)
struct Screen {
    webview: WebView,
    window: Window,
    pos: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    /// nome e posizione dello schermo: le impostazioni per schermo si legano a questo
    id: String,
    paused: bool,
    /// macOS ha detto almeno una volta che la finestra si vede: solo dopo "non si vede"
    /// vale come pausa (se per qualche motivo risultasse sempre coperta, per esempio
    /// dalla finestra trasparente delle icone del Finder, lo sfondo non resta fermo)
    visto: bool,
    /// ultimi margini mandati alla pagina (barra dei menu, Dock), in punti
    margini: Option<[i32; 4]>,
}

impl Screen {
    /// dove sta la finestra, in coordinate di Cocoa
    fn frame(&self) -> Option<NSRect> {
        let ns = self.window.ns_window() as *const NSWindow;
        (!ns.is_null()).then(|| unsafe { &*ns }.frame())
    }

    /// qualche pezzo della finestra si vede (lo calcola il window server)
    fn visibile(&self) -> bool {
        let ns = self.window.ns_window() as *const NSWindow;
        if ns.is_null() {
            return true;
        }
        unsafe { &*ns }.occlusionState().contains(NSWindowOcclusionState::Visible)
    }

    /// Quanto dello schermo coprono la barra dei menu e il Dock (in punti, come i pixel
    /// CSS della pagina): `frame` contro `visibleFrame` dello schermo della finestra.
    fn margini(&self) -> Option<[i32; 4]> {
        let ns = self.window.ns_window() as *const NSWindow;
        if ns.is_null() {
            return None;
        }
        let schermo = unsafe { &*ns }.screen()?;
        let (f, v) = (schermo.frame(), schermo.visibleFrame());
        // Cocoa ha l'origine in basso a sinistra
        let m = [
            (f.origin.y + f.size.height) - (v.origin.y + v.size.height),
            (f.origin.x + f.size.width) - (v.origin.x + v.size.width),
            v.origin.y - f.origin.y,
            v.origin.x - f.origin.x,
        ];
        Some(m.map(|x| x.round().max(0.0) as i32))
    }
}

/// C'è un'app a schermo intero sul riquadro `f` (coordinate di Cocoa)? Lo è una finestra
/// normale (livello 0) di un'altra app grande quanto tutto lo schermo: le app a schermo
/// intero di macOS e i giochi. Una finestra solo ingrandita lascia fuori la barra dei menu.
fn schermo_intero(f: NSRect) -> bool {
    let mio = std::process::id() as f64;
    let alto = unsafe { CGDisplayBounds(CGMainDisplayID()) }.size.height;
    // lo stesso riquadro in coordinate di CoreGraphics (origine in alto a sinistra)
    let (x, y) = (f.origin.x, alto - (f.origin.y + f.size.height));
    let Some(lista) = (unsafe { Retained::from_raw(CGWindowListCopyWindowInfo(K_CG_WINDOW_LIST_ON_SCREEN_ONLY, 0)) })
    else {
        return false;
    };
    let numero = |d: &NSDictionary<NSString, AnyObject>, chiave: &NSString| -> Option<f64> {
        d.objectForKey(chiave)?.downcast::<NSNumber>().ok().map(|n| n.doubleValue())
    };
    lista.iter().any(|d| {
        if numero(&d, ns_string!("kCGWindowOwnerPID")) == Some(mio)
            || numero(&d, ns_string!("kCGWindowLayer")).unwrap_or(-1.0) != 0.0
            || numero(&d, ns_string!("kCGWindowAlpha")).is_some_and(|a| a <= 0.0)
        {
            return false;
        }
        let Some(b) = d.objectForKey(ns_string!("kCGWindowBounds")) else { return false };
        let mut r = NSRect::ZERO;
        if !unsafe { CGRectMakeWithDictionaryRepresentation(Retained::as_ptr(&b), &mut r) } {
            return false;
        }
        (r.origin.x - x).abs() < 1.0
            && (r.origin.y - y).abs() < 1.0
            && (r.size.width - f.size.width).abs() < 1.0
            && (r.size.height - f.size.height).abs() < 1.0
    })
}

struct App {
    core: Core,
    screens: Vec<Screen>,
    tray: Option<TrayIcon>,
    last_ac: Option<bool>,
    stats: Stats,
    proxy: EventLoopProxy<Evento>,
    /// il monitor globale dei clic, acceso con "Clic sul desktop"
    monitor_clic: Option<Retained<AnyObject>>,
}

/// Una finestra (di qualsiasi app) dal suo numero: livello, app, nome.
fn finestra(numero: isize) -> Option<(f64, String, String)> {
    let n = u32::try_from(numero).ok().filter(|&n| n > 0)?;
    let lista = unsafe { Retained::from_raw(CGWindowListCopyWindowInfo(K_CG_WINDOW_LIST_INCLUDING_WINDOW, n)) }?;
    let d = lista.iter().next()?;
    let testo = |chiave: &NSString| {
        d.objectForKey(chiave).and_then(|o| o.downcast::<NSString>().ok()).map(|s| s.to_string()).unwrap_or_default()
    };
    let livello = d.objectForKey(ns_string!("kCGWindowLayer"))?.downcast::<NSNumber>().ok()?.doubleValue();
    Some((livello, testo(ns_string!("kCGWindowOwnerName")), testo(ns_string!("kCGWindowName"))))
}

/// Il clic nel punto (x, y), in coordinate di Cocoa, è andato al desktop? Lo dice il window
/// server: la finestra che riceve il clic è quella dell'evento (`numero_evento`) o, se manca,
/// quella che "verrebbe colpita da un clic" in quel punto (`windowNumberAtPoint`, che salta
/// le finestre trasparenti ai clic, come lo sfondo). Se è al livello delle icone del Finder
/// o più giù (la scrivania del Finder, con le sue icone, o l'immagine di sfondo) sì; se è una
/// finestra, il Dock, la barra dei menu e così via, no.
/// Non basta guardare la lista delle finestre sotto il punto: il Dock ha una finestra
/// grande quanto tutto lo schermo, trasparente ai clic fuori dalla sua striscia.
fn sul_desktop(x: f64, y: f64, numero_evento: isize) -> bool {
    let livello_icone = unsafe { CGWindowLevelForKey(K_CG_DESKTOP_ICON_WINDOW_LEVEL_KEY) } as f64;
    let numero_punto = MainThreadMarker::new()
        .map(|mtm| NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(NSPoint::new(x, y), 0, mtm))
        .unwrap_or(0);
    let numero = if numero_evento > 0 { numero_evento } else { numero_punto };
    let info = finestra(numero);
    zoo_log!(
        "clic: finestra dell'evento {numero_evento} {:?}, nel punto {numero_punto} {:?} (livello delle icone {livello_icone})",
        finestra(numero_evento),
        finestra(numero_punto)
    );
    // nessuna finestra: solo l'immagine di sfondo
    info.is_none_or(|(livello, _, _)| livello <= livello_icone)
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

    /// manda a ogni schermo le impostazioni che dipendono dallo schermo
    fn push_schermi(&self) {
        for s in &self.screens {
            for code in self.core.js_schermo(&s.id) {
                let _ = s.webview.evaluate_script(&code);
            }
        }
    }

    /// Pausa schermo per schermo: a batteria, oppure quando lo sfondo non si vede. Se non
    /// si vede perché sopra c'è un'app a schermo intero conta "Pausa con app a schermo
    /// intero", se no (finestre che coprono il desktop) "Pausa quando le finestre coprono il
    /// desktop".
    fn check_pause(&mut self) {
        let cfg = &self.core.cfg;
        let batteria = cfg.pausa_batteria && self.last_ac == Some(false);
        for s in &mut self.screens {
            let visibile = s.visibile();
            s.visto |= visibile;
            let nascosto = s.visto && !visibile;
            let conta = if !nascosto || batteria {
                false
            } else if s.frame().is_some_and(schermo_intero) {
                cfg.pausa_schermo_intero
            } else {
                cfg.pausa_coperto
            };
            let pause = batteria || conta;
            if pause != s.paused {
                s.paused = pause;
                zoo_log!(
                    "schermo {}: {} (batteria {batteria}, visibile {visibile})",
                    s.id,
                    if pause { "in pausa" } else { "riparte" }
                );
                let _ = s.webview.evaluate_script(if pause { JS_STOP } else { JS_START });
            }
        }
    }

    /// la barra dei menu e il Dock: lo si dice alla pagina, così i widget non ci finiscono
    /// sotto. Si manda solo quando cambia (Dock spostato, nascosto, schermo cambiato).
    fn check_margini(&mut self) {
        for s in &mut self.screens {
            let Some(m) = s.margini() else { continue };
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

    fn check_power(&mut self) {
        self.check_margini();
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

    /// Clic sul desktop: un monitor globale dei clic sinistri di NSEvent. Riceve solo i clic
    /// destinati alle altre app (lo sfondo è trasparente ai clic, quindi vanno al Finder) e
    /// non può né bloccarli né cambiarli; per i clic non serve il permesso Accessibilità
    /// (serve solo per i tasti). Si guarda solo dove si preme.
    fn sync_clic(&mut self) {
        match (self.core.cfg.clic_desktop, self.monitor_clic.is_some()) {
            (true, false) => {
                let proxy = self.proxy.clone();
                let blocco = RcBlock::new(move |e: NonNull<NSEvent>| {
                    let p = NSEvent::mouseLocation();
                    let numero = unsafe { e.as_ref() }.windowNumber();
                    let _ = proxy.send_event(Evento::Clic(p.x, p.y, numero));
                });
                self.monitor_clic = NSEvent::addGlobalMonitorForEventsMatchingMask_handler(NSEventMask::LeftMouseDown, &blocco);
                zoo_log!("clic sul desktop: monitor {}", if self.monitor_clic.is_some() { "acceso" } else { "non disponibile" });
            }
            (false, true) => {
                if let Some(m) = self.monitor_clic.take() {
                    unsafe { NSEvent::removeMonitor(&m) };
                }
            }
            _ => {}
        }
    }

    /// clic nel punto (x, y) in coordinate di Cocoa: se lì c'è il desktop, lo riceve lo
    /// sfondo di quello schermo (la pagina ragiona in punti, come Cocoa, ma con l'origine in alto)
    fn clic(&self, x: f64, y: f64, numero: isize) {
        if !self.core.cfg.clic_desktop {
            return;
        }
        for s in &self.screens {
            let Some(f) = s.frame() else { continue };
            if x < f.origin.x || y < f.origin.y || x >= f.origin.x + f.size.width || y >= f.origin.y + f.size.height {
                continue;
            }
            if s.paused {
                return;
            }
            if !sul_desktop(x, y, numero) {
                zoo_log!("clic a {x:.0},{y:.0}: non sul desktop (c'è una finestra, il Dock o la barra dei menu)");
                return;
            }
            let (cx, cy) = (x - f.origin.x, f.origin.y + f.size.height - y);
            zoo_log!("clic sul desktop: {x:.0},{y:.0} → schermo {} ({cx:.0}, {cy:.0})", s.id);
            let _ = s.webview.evaluate_script(&format!("window.zooPoke && zooPoke({cx:.1}, {cy:.1})"));
            return;
        }
    }

    /// se gli schermi sono cambiati (collegati, scollegati, risoluzione) rifà gli sfondi
    fn check_screens(&mut self, target: &EventLoopWindowTarget<Evento>, proxy: &EventLoopProxy<Evento>) {
        let now = schermi(target);
        self.core.set_schermi(now.iter().map(|m| m.2.clone()).collect());
        let same = now.len() == self.screens.len()
            && self.screens.iter().all(|s| now.iter().any(|m| m.0 == s.pos && m.1 == s.size && m.2.id == s.id));
        if !same {
            self.screens.clear();
            self.screens =
                now.into_iter()
                    .filter_map(|(pos, size, sc)| {
                        let iniziale = self.core.script_iniziale(&sc.id);
                        make_screen(target, proxy, pos, size, sc.id, &iniziale)
                    })
                    .collect();
            for s in &self.screens {
                zoo_log!("schermo {}: {}×{} a {},{}", s.id, s.size.width, s.size.height, s.pos.x, s.pos.y);
            }
        }
    }
}

/// posizione e dimensione (pixel fisici) di ogni schermo collegato, il principale per primo
fn schermi(target: &EventLoopWindowTarget<Evento>) -> Vec<(PhysicalPosition<i32>, PhysicalSize<u32>, Schermo)> {
    let principale = target.primary_monitor().map(|m| m.position());
    let mut v: Vec<_> = target
        .available_monitors()
        .map(|m| {
            let (pos, size) = (m.position(), m.size());
            let nome = m.name().unwrap_or_else(|| "schermo".into());
            let id = format!("{nome}@{},{}", pos.x, pos.y);
            (pos, size, Schermo { id, larghezza: size.width, altezza: size.height })
        })
        .collect();
    v.sort_by_key(|(pos, _, _)| (Some(*pos) != principale, pos.x, pos.y));
    v
}

/// porta la finestra al livello del desktop, in tutti gli Spazi, trasparente ai clic
fn al_livello_desktop(window: &Window) {
    let ns = window.ns_window() as *const NSWindow;
    if ns.is_null() {
        return;
    }
    unsafe {
        let ns = &*ns;
        // un gradino sopra l'immagine di sfondo, molto sotto le icone del Finder
        ns.setLevel(CGWindowLevelForKey(K_CG_DESKTOP_WINDOW_LEVEL_KEY) as isize + 1);
        ns.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns.setIgnoresMouseEvents(true);
        ns.setHasShadow(false);
    }
}

fn make_screen(
    target: &EventLoopWindowTarget<Evento>,
    proxy: &EventLoopProxy<Evento>,
    pos: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    id: String,
    iniziale: &str,
) -> Option<Screen> {
    let window = WindowBuilder::new()
        .with_title("ASCII Zoo")
        .with_decorations(false)
        .with_resizable(false)
        .with_visible(false)
        .with_position(pos)
        .with_inner_size(size)
        .build(target)
        .ok()?;
    al_livello_desktop(&window);

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
        .build(&window)
        .ok()?;
    window.set_visible(true);
    zoo_log!("avvio: schermo {id}: finestra e WebView pronte a {} ms dall'avvio", shared::ms_dall_avvio());
    Some(Screen { webview, window, pos, size, id, paused: false, visto: false, margini: None })
}

/// l'identificativo dell'app (CFBundleIdentifier in macos/Info.plist)
const BUNDLE_ID: &str = "io.github.tressilloide.asciizoo";

/// Una sola copia: se ce n'è già una (magari una versione vecchia, anche da un'altra .app)
/// le si chiede di uscire, come con "Esci" dal menu, e questa, appena avviata, prende il suo
/// posto. Se dopo 5 secondi c'è ancora, la si chiude per forza. Le .app non si cancellano:
/// le ha messe l'utente.
fn prendi_il_posto() {
    let mio = std::process::id() as libc::pid_t;
    let altre: Vec<_> = NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(BUNDLE_ID))
        .iter()
        .filter(|a| a.processIdentifier() != mio)
        .collect();
    if altre.is_empty() {
        return;
    }
    let pids: Vec<libc::pid_t> = altre.iter().map(|a| a.processIdentifier()).collect();
    zoo_log!("c'è già un ASCII Zoo (pid {pids:?}): lo chiudo e prendo il suo posto");
    // kill(pid, 0) dice solo se il processo c'è ancora (le proprietà di NSRunningApplication
    // si aggiornano solo con il ciclo degli eventi, che qui non è ancora partito)
    let vive = || pids.iter().any(|&p| p > 0 && unsafe { libc::kill(p, 0) } == 0);
    for a in &altre {
        a.terminate();
    }
    for _ in 0..50 {
        if !vive() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for a in &altre {
        a.forceTerminate();
    }
    for _ in 0..20 {
        if !vive() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn run() {
    // "Prova adesso" del menu Screensaver: un processo a parte, che non tocca l'app che gira
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--salvaschermo") {
        sogno::esegui(&args, salvaschermo::nell_app().as_deref());
    }
    // solo per la prova in CI: installa lo screensaver (e, se richiesto, apre le sue
    // impostazioni) ed esce, senza toccare l'app che gira
    if std::env::args().nth(1).as_deref() == Some("--installa-salvaschermo") {
        salvaschermo::installa_da_riga_di_comando(std::env::args().any(|a| a == "--apri-impostazioni"));
    }
    prendi_il_posto();
    // "Apri al login" segue la copia appena avviata
    autostart::aggiorna();
    // e anche lo screensaver, se è installato: dopo un aggiornamento dell'app è la versione nuova.
    // In un thread a parte: confronta i file del modulo, e all'accesso il disco è lento (lo
    // sfondo non lo deve aspettare)
    std::thread::spawn(|| objc2::rc::autoreleasepool(|_| salvaschermo::aggiorna_se_installato()));
    let mut event_loop = EventLoopBuilder::<Evento>::with_user_event().build();
    // niente icona nel Dock né nel selettore delle app: solo la barra dei menu
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let proxy = event_loop.create_proxy();

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let _ = menu_proxy.send_event(Evento::Menu(e.id.0));
    }));

    // true: anche qui c'è "Usa come screensaver…"
    let (core, menu) = Core::new(true);
    let mut menu = Some(menu);
    let mut app = App {
        core,
        screens: Vec::new(),
        tray: None,
        last_ac: None,
        stats: Stats::default(),
        proxy: proxy.clone(),
        monitor_clic: None,
    };
    let mut giro: u32 = 0;

    event_loop.run(move |event, target, control_flow| match event {
        Event::NewEvents(StartCause::Init) => {
            app.check_screens(target, &proxy);
            app.sync_clic();
            // su macOS l'icona si crea quando il ciclo degli eventi è già partito
            if let Some(menu) = menu.take() {
                app.tray = TrayIconBuilder::new()
                    .with_menu(Box::new(menu))
                    .with_tooltip("ASCII Zoo")
                    .with_icon(shared::tray_icon_image())
                    .build()
                    .ok();
            }
            *control_flow = ControlFlow::WaitUntil(Instant::now() + GIRO);
        }
        Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
            giro = giro.wrapping_add(1);
            // "Rotazione > Spenta": la pagina ha detto quale animale mostra
            if app.core.risposta_animale() {
                app.push_schermi();
            }
            if giro % GIRI_CONTROLLO == 0 {
                app.check_screens(target, &proxy);
                app.check_power(); // ricontrolla anche le pause
            } else {
                app.check_pause();
            }
            *control_flow = ControlFlow::WaitUntil(Instant::now() + GIRO);
        }
        Event::UserEvent(Evento::PaginaPronta) => app.push_settings(),
        Event::UserEvent(Evento::Clic(x, y, numero)) => app.clic(x, y, numero),
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
                app.screens.clear();
                app.tray = None;
                *control_flow = ControlFlow::Exit;
            }
            Esito::Salvaschermo => salvaschermo::installa(),
            Esito::Niente => {}
        },
        _ => {}
    });
}
