//! Windows: il menu dell'icona resta aperto quando si sceglie un'impostazione.
//!
//! Il menu lo mostra tray-icon con TrackPopupMenu, che per costruzione si chiude a ogni
//! scelta. Mentre è aperto, però, ogni messaggio del mouse e della tastiera passa da un
//! filtro del nostro thread (hook WH_MSGFILTER, codice MSGF_MENU) prima di arrivare al
//! menu. Lì si guarda quale voce c'è sotto il clic (o quale è evidenziata, per Invio):
//! - una spunta (CheckMenuItem di muda: animale, tema, widget, pause…) si applica subito,
//!   con lo stesso WM_COMMAND che il menu manderebbe chiudendosi, e il clic non arriva al
//!   menu, che quindi resta aperto, nello stesso sottomenu, con la spunta ridisegnata;
//! - le altre voci (quelle con "…" che aprono una finestrella, Foto, Ricarica, Esci, il
//!   pomodoro) arrivano al menu come prima e lo chiudono.
//!
//! Il menu si chiude come sempre cliccando fuori o con Esc.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};

use tray_icon::menu::{ContextMenu, Menu, MenuItemKind};
use tray_icon::TrayIcon;
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::{classe, APP};
use crate::shared::zoo_log;

/// la finestra nascosta di tray-icon: riceve il WM_COMMAND del menu (muda la sottoclassa)
static TRAY: AtomicIsize = AtomicIsize::new(0);
/// il menu come lo mostra tray-icon (serve solo alla prova in CI)
static HMENU_TRAY: AtomicIsize = AtomicIsize::new(0);

/// prova in CI: apri il menu nel punto (x, y) in wParam e lParam, con ASCII_ZOO_PROVA_MENU=1
pub const WM_PROVA_MENU: u32 = WM_APP + 2;

thread_local! {
    /// Le voci del primo livello del menu. Le sottovoci si leggono da qui al momento del
    /// clic, perché quelle degli schermi si rifanno quando cambiano i monitor. Non si usa
    /// Menu::items(): mentre muda mostra il menu da sé tiene in prestito la radice.
    static VOCI: RefCell<Vec<MenuItemKind>> = const { RefCell::new(Vec::new()) };
}

/// da chiamare una volta, dopo aver creato l'icona: il filtro vale per tutto il thread e
/// si toglie da solo quando il programma finisce
pub fn prepara(menu: &Menu, tray: &TrayIcon) {
    VOCI.with(|v| *v.borrow_mut() = menu.items());
    TRAY.store(tray.window_handle() as isize, Ordering::SeqCst);
    HMENU_TRAY.store(menu.hpopupmenu(), Ordering::SeqCst);
    let ok = unsafe { SetWindowsHookExW(WH_MSGFILTER, Some(filtro), None, GetCurrentThreadId()) }.is_ok();
    zoo_log!("menu: filtro per tenerlo aperto {}", if ok { "attivo" } else { "NON attivo" });
}

/// Filtro dei messaggi dei cicli modali del thread (menu, ma anche la finestrella dei
/// nomi con IsDialogMessage): qui conta solo il menu. 1 = messaggio mangiato.
unsafe extern "system" fn filtro(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == MSGF_MENU as i32 && lp.0 != 0 && gestito(&*(lp.0 as *const MSG)) {
        return LRESULT(1);
    }
    CallNextHookEx(None, code, wp, lp)
}

/// true se il messaggio riguarda una voce che lascia il menu aperto (e quindi va mangiato)
unsafe fn gestito(msg: &MSG) -> bool {
    match msg.message {
        // anche la pressione: il menu non deve vedere metà clic
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => voce_sotto(msg.pt).is_some(),
        WM_LBUTTONUP => voce_sotto(msg.pt).map(|id| scegli(id)).is_some(),
        WM_KEYDOWN if msg.wParam.0 == VK_RETURN.0 as usize => voce_evidenziata().map(|id| scegli(id)).is_some(),
        _ => false,
    }
}

/// le finestre dei menu aperti (classe "#32768") di questo programma, la più in alto
/// (l'ultimo sottomenu aperto) per prima
unsafe fn finestre_menu() -> Vec<HWND> {
    let mut v = Vec::new();
    let mut dopo = None;
    while let Ok(h) = FindWindowExW(None, dopo, w!("#32768"), None) {
        dopo = Some(h);
        let mut pid = 0;
        GetWindowThreadProcessId(h, Some(&mut pid));
        if pid == GetCurrentProcessId() && IsWindowVisible(h).as_bool() {
            v.push(h);
        }
    }
    v
}

/// il menu dell'icona (o un suo sottomenu) è aperto adesso
pub fn aperto() -> bool {
    unsafe { !finestre_menu().is_empty() }
}

/// il menu mostrato in una finestra "#32768"
unsafe fn menu_di(h: HWND) -> Option<HMENU> {
    let m = SendMessageW(h, MN_GETHMENU, None, None).0;
    (m != 0).then_some(HMENU(m as *mut _))
}

/// la voce sotto il punto (coordinate dello schermo), se lascia il menu aperto
unsafe fn voce_sotto(pt: POINT) -> Option<u32> {
    let h = WindowFromPoint(pt);
    if h.is_invalid() || classe(h) != "#32768" || !finestre_menu().contains(&h) {
        return None;
    }
    let hm = menu_di(h)?;
    let pos = MenuItemFromPoint(None, hm, pt);
    (pos >= 0).then(|| voce_che_resta(hm, pos)).flatten()
}

/// la voce evidenziata (dove andrebbe Invio): quella dell'ultimo sottomenu aperto che non
/// apre a sua volta un sottomenu
unsafe fn voce_evidenziata() -> Option<u32> {
    for h in finestre_menu() {
        let Some(hm) = menu_di(h) else { continue };
        for pos in 0..GetMenuItemCount(Some(hm)).max(0) {
            let id = GetMenuItemID(hm, pos);
            if id != u32::MAX && id != 0 && GetMenuState(hm, pos as u32, MF_BYPOSITION) & MF_HILITE.0 != 0 {
                return voce_che_resta(hm, pos);
            }
        }
    }
    None
}

/// L'id Win32 della voce `pos` di `hm` se è una spunta attiva. muda dà come id Win32 il
/// suo numero interno, che è anche il MenuId delle voci create senza id (tutte, in
/// shared.rs): basta confrontare i due. Una voce sconosciuta chiude il menu come prima.
unsafe fn voce_che_resta(hm: HMENU, pos: i32) -> Option<u32> {
    let id = GetMenuItemID(hm, pos);
    if id == u32::MAX || id == 0 || GetMenuState(hm, pos as u32, MF_BYPOSITION) & (MF_GRAYED.0 | MF_DISABLED.0) != 0 {
        return None;
    }
    // con la finestrella di un nome aperta il programma è fermo lì dentro: la scelta si
    // applicherebbe solo dopo, quindi il menu si chiude come sempre
    if !APP.with(|a| a.try_borrow_mut().is_ok()) {
        return None;
    }
    let testo = id.to_string();
    VOCI.with(|v| v.try_borrow().is_ok_and(|v| e_una_spunta(&v, &testo))).then_some(id)
}

/// true se tra le voci (sottomenu compresi) c'è una spunta con questo id
fn e_una_spunta(voci: &[MenuItemKind], id: &str) -> bool {
    voci.iter().any(|v| match v {
        MenuItemKind::Check(c) => c.id().0 == id,
        MenuItemKind::Submenu(s) => e_una_spunta(&s.items(), id),
        _ => false,
    })
}

/// Sceglie la voce senza chiudere il menu: lo stesso WM_COMMAND che manderebbe il menu
/// (muda cambia la spunta e manda il MenuEvent), poi la scelta si applica subito, così lo
/// sfondo cambia mentre il menu è ancora aperto, e il menu si ridisegna con le spunte nuove.
unsafe fn scegli(id: u32) {
    zoo_log!("menu: voce {id} scelta, il menu resta aperto");
    let tray = HWND(TRAY.load(Ordering::SeqCst) as *mut _);
    SendMessageW(tray, WM_COMMAND, Some(WPARAM(id as usize & 0xFFFF)), Some(LPARAM(0)));
    APP.with(|a| {
        if let Ok(mut a) = a.try_borrow_mut() {
            if let Some(app) = a.as_mut() {
                app.leggi_menu();
            }
        }
    });
    for h in finestre_menu() {
        let _ = InvalidateRect(Some(h), None, true);
    }
}

/// Prova in CI: apre il menu come fa tray-icon dopo un clic destro sull'icona (stessa
/// chiamata, stessa finestra), per quando l'icona non si può cliccare davvero.
pub unsafe fn apri_per_prova(x: i32, y: i32) {
    if std::env::var_os("ASCII_ZOO_PROVA_MENU").is_none_or(|v| v != "1") {
        return;
    }
    zoo_log!("prova: apro il menu in {x}, {y}");
    let tray = HWND(TRAY.load(Ordering::SeqCst) as *mut _);
    let _ = SetForegroundWindow(tray);
    let hm = HMENU(HMENU_TRAY.load(Ordering::SeqCst) as *mut _);
    let _ = TrackPopupMenu(hm, TPM_BOTTOMALIGN | TPM_LEFTALIGN, x, y, None, tray, None);
}
