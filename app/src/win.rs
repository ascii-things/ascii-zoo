//! Windows.
//!
//! - per ogni monitor una finestra con dentro WebView2 viene messa dietro le icone del desktop
//! - una finestra nascosta di controllo tiene il timer e si accorge dei cambi di schermo
//!   (monitor collegati/scollegati, risoluzione, riavvio di Explorer) e rifà gli sfondi
//! - un'icona nell'area di notifica ha tutte le opzioni
//! - pausa automatica a batteria e, schermo per schermo, con app a schermo intero
//! - un clic su una zona vuota del desktop arriva agli animali: un hook del mouse
//!   guarda solo dove si clicca (non blocca e non cambia niente) e, se sotto c'è il
//!   desktop, lo passa allo sfondo
//! - il menu dell'icona resta aperto quando si sceglie un'impostazione (vedi `menu`)

#[path = "menu_win.rs"]
pub(crate) mod menu;

use std::cell::RefCell;
use std::num::NonZeroIsize;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use tray_icon::menu::MenuEvent;
use tray_icon::{TrayIcon, TrayIconBuilder};
use windows::core::{w, BOOL};
use windows::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_ABANDONED,
    WAIT_OBJECT_0, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, MonitorFromPoint, MonitorFromRect, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
    MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcessId, OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use wry::{PageLoadEvent, WebContext, WebView, WebViewBuilder};

use crate::config::autostart;
use crate::shared::{self, zoo_log, Core, Esito, Schermo, JS_START, JS_STOP, URL};
use crate::stats::Stats;
use crate::{desktop, foto, power};

const TIMER: usize = 1;
/// tick (da 250 ms) da aspettare dopo un cambio di schermo prima di rifare gli sfondi
const REBUILD_DELAY: u32 = 4;

static PAGE_READY: AtomicBool = AtomicBool::new(false);
/// un monitor è cambiato o Explorer è ripartito: gli sfondi vanno rifatti
static REBUILD: AtomicBool = AtomicBool::new(false);
/// messaggio "TaskbarCreated", mandato da Explorer quando riparte
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// la finestra di controllo, per l'hook del mouse
static CTL: AtomicIsize = AtomicIsize::new(0);
/// clic sul desktop: x e y (pixel fisici dello schermo) in wParam e lParam
const WM_CLIC: u32 = WM_APP + 1;

/// HWND che wry può usare come finestra ospite
struct Host(HWND);
impl HasWindowHandle for Host {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let raw = Win32WindowHandle::new(NonZeroIsize::new(self.0 .0 as isize).ok_or(HandleError::Unavailable)?);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(raw)) })
    }
}

/// lo sfondo di un monitor
struct Screen {
    webview: WebView,
    hwnd: HWND,
    parent: HWND,
    rect: RECT,
    /// nome del monitor ("\\.\DISPLAY1"): le impostazioni per schermo si legano a questo
    id: String,
    paused: bool,
    /// ultimi margini mandati alla pagina (pixel fisici coperti dalla barra delle applicazioni)
    margini: Option<[i32; 4]>,
}

impl Screen {
    fn js(&self, code: &str) {
        let _ = self.webview.evaluate_script(code);
    }
}

struct App {
    hinst: HINSTANCE,
    ctl: HWND,
    screens: Vec<Screen>,
    _tray: TrayIcon,
    core: Core,
    last_ac: Option<bool>,
    ticks: u32,
    rebuild_at: Option<u32>,
    stats: Stats,
    hook: Option<HHOOK>,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
}

impl App {
    fn js(&self, code: &str) {
        for s in &self.screens {
            s.js(code);
        }
    }

    /// manda allo sfondo tutte le impostazioni (funzione zooSetting della pagina)
    fn push_settings(&mut self) {
        let ac = power::on_ac();
        self.last_ac = Some(ac);
        for s in &self.screens {
            for code in self.core.js_tutto(ac && power::ha_batteria(), &s.id) {
                s.js(&code);
            }
            if s.paused {
                s.js(JS_STOP);
            }
        }
    }

    /// manda a ogni schermo le impostazioni che dipendono dallo schermo
    fn push_schermi(&self) {
        for s in &self.screens {
            for code in self.core.js_schermo(&s.id) {
                s.js(&code);
            }
        }
    }

    /// Pausa schermo per schermo: a batteria, con un'app a schermo intero, oppure
    /// quando le finestre coprono tutto il desktop di quello schermo (animare
    /// qualcosa che nessuno vede costa CPU e GPU per niente).
    fn check_pause(&mut self) {
        let cfg = &self.core.cfg;
        let battery = cfg.pausa_batteria && self.last_ac == Some(false);
        let full = if cfg.pausa_schermo_intero { power::fullscreen_monitor() } else { None };
        let coperto = cfg.pausa_coperto;
        for s in &mut self.screens {
            let pause = battery || full == Some(s.rect) || (coperto && unsafe { desktop_coperto(s.rect) });
            if pause != s.paused {
                s.paused = pause;
                zoo_log!("schermo {}: {}", s.id, if pause { "in pausa" } else { "riparte" });
                s.js(if pause { JS_STOP } else { JS_START });
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

    /// Quanto della parte di schermo coperta dallo sfondo è occupato dalla barra delle
    /// applicazioni (area di lavoro più piccola del monitor): lo si dice alla pagina,
    /// così i widget non ci finiscono sotto. Si manda solo quando cambia.
    fn check_margini(&mut self) {
        for s in &mut self.screens {
            let r = s.rect;
            let (m, scala) = unsafe {
                let mon = MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                if !GetMonitorInfoW(mon, &mut mi).as_bool() {
                    continue;
                }
                let w = mi.rcWork;
                let (mut dx, mut dy) = (96u32, 96u32);
                let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
                let m = [w.top - r.top, r.right - w.right, r.bottom - w.bottom, w.left - r.left].map(|v| v.max(0));
                (m, dx.max(48) as f64 / 96.0)
            };
            if s.margini == Some(m) {
                continue;
            }
            s.margini = Some(m);
            let c = m.map(|v| (v as f64 / scala).round());
            s.js(&format!(
                "window.zooSetting && zooSetting('margini', {{alto: {}, destra: {}, basso: {}, sinistra: {}}})",
                c[0], c[1], c[2], c[3]
            ));
        }
    }

    /// accende o spegne l'hook del mouse secondo l'impostazione "Clic sul desktop"
    fn sync_hook(&mut self) {
        let voglio = self.core.cfg.clic_desktop;
        unsafe {
            match (voglio, self.hook) {
                (true, None) => {
                    self.hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(self.hinst), 0).ok();
                }
                (false, Some(h)) => {
                    let _ = UnhookWindowsHookEx(h);
                    self.hook = None;
                }
                _ => {}
            }
        }
    }

    /// clic sul desktop nel punto (x, y) dello schermo: lo riceve lo sfondo di quel monitor
    fn clic(&self, x: i32, y: i32) {
        let Some(s) = self.screens.iter().find(|s| {
            let r = s.rect;
            x >= r.left && x < r.right && y >= r.top && y < r.bottom
        }) else {
            return;
        };
        if s.paused {
            return;
        }
        // la pagina ragiona in pixel CSS: si divide per la scala del monitor
        let (mut dx, mut dy) = (96u32, 96u32);
        unsafe {
            let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
            let _ = GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        }
        let scala = dx.max(48) as f64 / 96.0;
        let (px, py) = ((x - s.rect.left) as f64 / scala, (y - s.rect.top) as f64 / scala);
        s.js(&format!("window.zooPoke && zooPoke({px:.1}, {py:.1})"));
    }

    /// rifà gli sfondi se i monitor sono cambiati (o se Explorer ha distrutto le finestre)
    fn rebuild(&mut self) {
        let mons = monitors();
        self.core.set_schermi(mons.iter().map(|m| m.1.clone()).collect());
        let same = mons.len() == self.screens.len()
            && self.screens.iter().all(|s| {
                (unsafe { IsWindow(Some(s.hwnd)).as_bool() }) && mons.iter().any(|m| m.0 == s.rect && m.1.id == s.id)
            });
        if same {
            for s in &self.screens {
                unsafe { desktop::fit(s.hwnd, s.parent, s.rect) };
            }
            // l'ordine o lo schermo principale possono essere cambiati
            self.push_schermi();
            return;
        }
        drop_screens(&mut self.screens);
        self.screens = make_screens(self.hinst, &mons, &self.core);
        // le impostazioni arrivano quando le pagine hanno finito di caricare (PAGE_READY)
    }

    fn on_menu(&mut self, id: &str) {
        match self.core.on_menu(id) {
            Esito::Cambiato(js) => {
                if let Some(code) = js {
                    self.js(&code);
                }
                self.check_power();
                self.sync_hook();
            }
            Esito::Schermi => self.push_schermi(),
            Esito::Foto => foto::scatta(self.screens.iter().map(|s| &s.webview)),
            Esito::FermaRotazione => self.core.chiedi_animale(self.screens.iter().map(|s| (s.id.as_str(), &s.webview))),
            Esito::Ricarica => {
                for s in &mut self.screens {
                    s.paused = false;
                    let _ = s.webview.load_url(URL);
                }
            }
            Esito::Salvaschermo => ascii_dreams::installa(),
            Esito::Esci => unsafe {
                let _ = DestroyWindow(self.ctl);
            },
            Esito::Niente => {}
        }
    }

    /// applica le voci del menu scelte (dal timer, o subito se il menu resta aperto)
    fn leggi_menu(&mut self) {
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            self.on_menu(&ev.id.0);
        }
    }

    fn tick(&mut self) {
        self.leggi_menu();
        // "Rotazione > Spenta": la pagina ha detto quale animale mostra
        if self.core.risposta_animale() {
            self.push_schermi();
        }
        self.ticks = self.ticks.wrapping_add(1);
        if REBUILD.swap(false, Ordering::SeqCst) {
            // altri cambi in arrivo spostano l'attesa: si rifà tutto una volta sola
            self.rebuild_at = Some(self.ticks.wrapping_add(REBUILD_DELAY));
        }
        if self.rebuild_at == Some(self.ticks) {
            self.rebuild_at = None;
            self.rebuild();
        }
        if PAGE_READY.swap(false, Ordering::SeqCst) {
            // una pagina (ri)caricata non sa niente: si rimanda tutto, margini compresi
            for s in &mut self.screens {
                shared::log_pagina_pronta(&s.webview, &s.id);
                s.margini = None;
            }
            self.push_settings();
            self.check_margini();
        }
        if self.ticks % 2 == 0 {
            // dopo il salvaschermo lo sfondo fisso di Windows può tornare sopra gli animali
            let mut fatti: Vec<HWND> = Vec::new();
            for s in &self.screens {
                if !fatti.contains(&s.parent) {
                    fatti.push(s.parent);
                    unsafe { desktop::riordina(s.parent) };
                }
            }
        }
        if self.ticks % 8 == 0 {
            self.check_power(); // ricontrolla anche le pause
            self.check_margini();
        } else if self.ticks % 2 == 0 {
            // le pause ogni mezzo secondo: quando si scopre il desktop l'animazione riparte subito
            self.check_pause();
        }
    }
}

/// un monitor: rettangolo (pixel fisici), nome del dispositivo ("\\.\DISPLAY1"), principale?
unsafe fn info_monitor(mon: HMONITOR) -> Option<(RECT, Schermo, bool)> {
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    if !GetMonitorInfoW(mon, &mut mi as *mut MONITORINFOEXW as *mut MONITORINFO).as_bool() {
        return None;
    }
    let r = mi.monitorInfo.rcMonitor;
    let n = mi.szDevice.iter().position(|&c| c == 0).unwrap_or(mi.szDevice.len());
    let id = String::from_utf16_lossy(&mi.szDevice[..n]);
    let s = Schermo { id, larghezza: (r.right - r.left) as u32, altezza: (r.bottom - r.top) as u32 };
    Some((r, s, mi.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0))
}

/// tutti i monitor collegati: il principale per primo, poi da sinistra a destra
fn monitors() -> Vec<(RECT, Schermo)> {
    unsafe extern "system" fn add(mon: HMONITOR, _: HDC, _: *mut RECT, out: LPARAM) -> BOOL {
        let list = &mut *(out.0 as *mut Vec<(RECT, Schermo, bool)>);
        if let Some(m) = info_monitor(mon) {
            list.push(m);
        }
        BOOL(1)
    }
    let mut list: Vec<(RECT, Schermo, bool)> = Vec::new();
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(add), LPARAM(&mut list as *mut _ as isize));
        if list.is_empty() {
            list.extend(info_monitor(MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY)));
        }
    }
    list.sort_by_key(|(r, _, principale)| (!principale, r.left, r.top));
    list.into_iter().map(|(r, s, _)| (r, s)).collect()
}

/// finestra + WebView dietro le icone, sopra il monitor `rect`
unsafe fn make_screen(hinst: HINSTANCE, rect: RECT, id: String, iniziale: &str) -> Option<Screen> {
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        w!("AsciiZooWallpaper"),
        w!("ASCII Zoo"),
        WS_POPUP,
        rect.left,
        rect.top,
        rect.right - rect.left,
        rect.bottom - rect.top,
        None,
        None,
        Some(hinst),
        None,
    )
    .ok()?;
    let Some(embed) = desktop::attach(hwnd) else {
        let _ = DestroyWindow(hwnd);
        return None;
    };
    desktop::fit(hwnd, embed.parent, rect);
    zoo_log!("avvio: schermo {id}: finestra pronta a {} ms dall'avvio, ora la WebView", shared::ms_dall_avvio());

    // Il profilo della WebView2 sempre nello stesso posto, %LOCALAPPDATA%\AsciiZoo\WebView2: di
    // serie starebbe accanto all'exe, che non si installa, e ogni versione nuova (scaricata in
    // un'altra cartella) ripartiva da un profilo vuoto, da creare, più lenta a partire; con
    // l'exe in una cartella di sola lettura la WebView non partiva proprio. Il contesto serve
    // solo a crearla.
    let mut contesto = WebContext::new(profilo_webview());
    let built = WebViewBuilder::new_with_web_context(&mut contesto)
        .with_custom_protocol("zoo".into(), |_id, _req| shared::pagina())
        .with_navigation_handler(shared::navigazione_permessa)
        .with_new_window_req_handler(|_, _| wry::NewWindowResponse::Deny)
        .with_url(URL)
        .with_initialization_script(iniziale)
        .with_background_color((9, 10, 15, 255))
        .with_devtools(false)
        .with_on_page_load_handler(|ev, _url| {
            if let PageLoadEvent::Finished = ev {
                PAGE_READY.store(true, Ordering::SeqCst);
            }
        })
        .build(&Host(hwnd));
    match built {
        Ok(webview) => {
            zoo_log!("avvio: schermo {id}: finestra e WebView pronte a {} ms dall'avvio", shared::ms_dall_avvio());
            Some(Screen { webview, hwnd, parent: embed.parent, rect, id, paused: false, margini: None })
        }
        Err(_) => {
            let _ = DestroyWindow(hwnd);
            None
        }
    }
}

/// gli sfondi, con le impostazioni di ogni schermo già nella pagina dal primo fotogramma (il
/// Core deve già conoscere gli schermi: set_schermi prima)
/// %LOCALAPPDATA%\AsciiZoo\WebView2 (None se non si sa dov'è: allora accanto all'exe)
fn profilo_webview() -> Option<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("AsciiZoo").join("WebView2");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn make_screens(hinst: HINSTANCE, mons: &[(RECT, Schermo)], core: &Core) -> Vec<Screen> {
    mons.iter().filter_map(|(r, s)| unsafe { make_screen(hinst, *r, s.id.clone(), &core.script_iniziale(&s.id)) }).collect()
}

fn drop_screens(screens: &mut Vec<Screen>) {
    for s in screens.drain(..) {
        let hwnd = s.hwnd;
        drop(s); // prima la WebView, poi la sua finestra
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// nome della classe di una finestra
unsafe fn classe(h: HWND) -> String {
    let mut buf = [0u16; 64];
    let n = GetClassNameW(h, &mut buf) as usize;
    String::from_utf16_lossy(&buf[..n])
}

/// true se nel punto c'è il desktop (lo spazio tra le icone o lo sfondo stesso)
unsafe fn sul_desktop(pt: POINT) -> bool {
    let h = WindowFromPoint(pt);
    if h.is_invalid() {
        return false;
    }
    match classe(h).as_str() {
        "Progman" | "WorkerW" | "AsciiZooWallpaper" => true,
        // la finestra delle icone: su Windows 11 24H2 risponde lei stessa,
        // prima rispondeva l'elenco (SysListView32) che contiene
        "SHELLDLL_DefView" => GetParent(h).is_ok_and(|p| matches!(classe(p).as_str(), "Progman" | "WorkerW")),
        "SysListView32" => GetParent(h).is_ok_and(|p| classe(p) == "SHELLDLL_DefView"),
        _ => false,
    }
}

/// true se le finestre coprono tutto il desktop dello schermo `r`: si prova una griglia
/// di punti dell'area di lavoro (uno ogni 160 pixel circa, barra delle applicazioni
/// esclusa) e nessuno cade sul desktop. WindowFromPoint salta le finestre trasparenti
/// ai clic e quelle nascoste, e non manda messaggi ad altri programmi.
unsafe fn desktop_coperto(r: RECT) -> bool {
    let mon = MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST);
    let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    let w = if GetMonitorInfoW(mon, &mut mi).as_bool() { mi.rcWork } else { r };
    let (lw, lh) = (w.right - w.left, w.bottom - w.top);
    if lw <= 0 || lh <= 0 {
        return false;
    }
    let (nx, ny) = ((lw / 160).clamp(4, 40), (lh / 160).clamp(3, 40));
    for j in 0..ny {
        for i in 0..nx {
            let pt = POINT { x: w.left + (2 * i + 1) * lw / (2 * nx), y: w.top + (2 * j + 1) * lh / (2 * ny) };
            if sul_desktop(pt) {
                return false;
            }
        }
    }
    true
}

/// Hook del mouse: legge solo dove si preme il tasto sinistro e lascia passare
/// sempre il clic (CallNextHookEx), così le icone funzionano come prima.
/// Il lavoro vero si fa dopo, nella finestra di controllo.
unsafe extern "system" fn mouse_hook(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && wp.0 as u32 == WM_LBUTTONDOWN {
        let info = &*(lp.0 as *const MSLLHOOKSTRUCT);
        if sul_desktop(info.pt) {
            let ctl = HWND(CTL.load(Ordering::SeqCst) as *mut _);
            let _ = PostMessageW(Some(ctl), WM_CLIC, WPARAM(info.pt.x as isize as usize), LPARAM(info.pt.y as isize));
        }
    }
    CallNextHookEx(None, code, wp, lp)
}

/// la finestra nascosta di controllo: timer, cambi di schermo, uscita
unsafe extern "system" fn control_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            APP.with(|a| {
                if let Ok(mut a) = a.try_borrow_mut() {
                    if let Some(app) = a.as_mut() {
                        app.tick();
                    }
                }
            });
            LRESULT(0)
        }
        WM_CLIC => {
            let (x, y) = (wp.0 as isize as i32, lp.0 as i32);
            APP.with(|a| {
                if let Ok(a) = a.try_borrow() {
                    if let Some(app) = a.as_ref() {
                        app.clic(x, y);
                    }
                }
            });
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            REBUILD.store(true, Ordering::SeqCst);
            LRESULT(0)
        }
        menu::WM_PROVA_MENU => {
            menu::apri_per_prova(wp.0 as i32, lp.0 as i32);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        m if m != 0 && m == TASKBAR_CREATED.load(Ordering::SeqCst) => {
            // Explorer è ripartito: il desktop è nuovo, gli sfondi vanno riagganciati
            REBUILD.store(true, Ordering::SeqCst);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// le finestre degli sfondi non fanno niente di speciale
unsafe extern "system" fn screen_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wp, lp)
}

/// C'è già un ASCII Zoo, magari una versione vecchia in un'altra cartella: gli si chiede di
/// chiudersi (WM_CLOSE alla sua finestra di controllo: esce come con "Esci" e rimette lo
/// sfondo di Windows) e si aspetta il suo mutex. Se dopo 8 secondi c'è ancora, lo si
/// termina. true se il mutex adesso è di questa copia.
unsafe fn prendi_il_posto(mutex: HANDLE) -> bool {
    let preso = |ms| matches!(WaitForSingleObject(mutex, ms), WAIT_OBJECT_0 | WAIT_ABANDONED);
    let mut pid = 0u32;
    if let Ok(vecchia) = FindWindowW(w!("AsciiZooControl"), None) {
        GetWindowThreadProcessId(vecchia, Some(&mut pid));
        let _ = PostMessageW(Some(vecchia), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    zoo_log!("c'è già un ASCII Zoo (pid {pid}): lo chiudo e prendo il suo posto");
    if preso(8000) {
        return true;
    }
    if pid != 0 && pid != GetCurrentProcessId() {
        if let Ok(p) = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid) {
            let _ = TerminateProcess(p, 1);
            let _ = WaitForSingleObject(p, 3000);
            let _ = CloseHandle(p);
        }
    }
    preso(3000)
}

/// ridisegna lo sfondo normale di Windows quando il programma si chiude
fn restore_wallpaper() {
    unsafe {
        let mut buf = [0u16; 520];
        if SystemParametersInfoW(SPI_GETDESKWALLPAPER, buf.len() as u32, Some(buf.as_mut_ptr().cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)).is_ok() {
            let _ = SystemParametersInfoW(SPI_SETDESKWALLPAPER, 0, Some(buf.as_mut_ptr().cast()), SPIF_UPDATEINIFILE);
        }
    }
}

pub fn run() {
    unsafe {
        // una sola copia alla volta: se ce n'è già una (magari una versione vecchia) si
        // chiude lei e questa, appena avviata, prende il suo posto. Va fatto prima di creare
        // la finestra di controllo, se no FindWindow troverebbe la nostra.
        let mutex = CreateMutexW(None, true, w!("AsciiZoo.SingleInstance"));
        let gia_avviata = GetLastError() == ERROR_ALREADY_EXISTS;
        // all'accesso partono sia l'attività pianificata sia la chiave Run (vedi autostart):
        // la seconda trova la prima già avviata e lascia stare
        if gia_avviata && std::env::args().any(|a| a == autostart::ACCESSO) {
            zoo_log!("avvio con Windows: ASCII Zoo è già partito, questa copia si chiude");
            return;
        }
        if gia_avviata && !mutex.as_ref().is_ok_and(|m| prendi_il_posto(*m)) {
            return;
        }
        // "Avvia con Windows" segue la copia appena avviata (chiama schtasks: in un thread)
        std::thread::spawn(autostart::aggiorna);
        // all'accesso l'app può partire prima che Explorer abbia fatto il desktop e la barra
        // delle applicazioni: li si aspetta (fino a un minuto)
        let attesa = std::time::Instant::now();
        while !desktop::pronto() && attesa.elapsed() < std::time::Duration::from_secs(60) {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        if attesa.elapsed().as_millis() >= 200 {
            zoo_log!("avvio: aspettato il desktop di Explorer per {} ms", attesa.elapsed().as_millis());
        }
        // e anche lo screensaver, se è installato: dopo un aggiornamento dell'app è la versione
        // nuova. In un thread a parte: legge per intero due eseguibili per confrontarli, e
        // all'accesso il disco è lento (lo sfondo non lo deve aspettare)
        std::thread::spawn(ascii_dreams::aggiorna_se_installato);
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let hinst: HINSTANCE = GetModuleHandleW(None).expect("modulo").into();
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(control_proc),
            hInstance: hinst,
            lpszClassName: w!("AsciiZooControl"),
            ..Default::default()
        });
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(screen_proc),
            hInstance: hinst,
            lpszClassName: w!("AsciiZooWallpaper"),
            hCursor: cursor,
            ..Default::default()
        });

        // finestra di primo livello mai mostrata: riceve WM_DISPLAYCHANGE e TaskbarCreated,
        // che le finestre figlie del desktop non ricevono
        let ctl = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            w!("AsciiZooControl"),
            w!("ASCII Zoo"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinst),
            None,
        )
        .expect("finestra");
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::SeqCst);
        CTL.store(ctl.0 as isize, Ordering::SeqCst);

        // prima le impostazioni e gli schermi, poi gli sfondi: ognuno parte già con le sue
        let mons = monitors();
        let (mut core, menu) = Core::new(true);
        core.set_schermi(mons.iter().map(|m| m.1.clone()).collect());
        zoo_log!("avvio: impostazioni e menu pronti a {} ms dall'avvio", shared::ms_dall_avvio());
        let screens = make_screens(hinst, &mons, &core);
        if screens.is_empty() {
            MessageBoxW(None, w!("Non riesco a trovare il desktop di Windows."), w!("ASCII Zoo"), MB_ICONERROR);
            return;
        }
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu.clone()))
            .with_tooltip("ASCII Zoo")
            .with_icon(shared::tray_icon_image())
            .build()
            .expect("icona di notifica");
        menu::prepara(&menu, &tray);

        APP.with(|a| {
            *a.borrow_mut() = Some(App {
                hinst,
                ctl,
                screens,
                _tray: tray,
                core,
                last_ac: None,
                ticks: 0,
                rebuild_at: None,
                stats: Stats::default(),
                hook: None,
            })
        });
        APP.with(|a| {
            if let Some(app) = a.borrow_mut().as_mut() {
                app.sync_hook();
            }
        });

        SetTimer(Some(ctl), TIMER, 250, None);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        if let Some(mut app) = APP.with(|a| a.borrow_mut().take()) {
            if let Some(h) = app.hook.take() {
                let _ = UnhookWindowsHookEx(h);
            }
            drop_screens(&mut app.screens);
        }
        restore_wallpaper();
    }
}
