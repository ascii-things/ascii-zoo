//! Mette una finestra dietro le icone del desktop (il trucco "WorkerW").
//!
//! Due casi:
//! - Windows 10 / 11 classico: Progman, ricevuto il messaggio 0x052C, crea una
//!   WorkerW di primo livello dietro quella che contiene le icone
//!   (SHELLDLL_DefView). La finestra dello sfondo diventa figlia di quella WorkerW.
//! - Windows 11 24H2 e successivi ("raised desktop"): SHELLDLL_DefView e WorkerW
//!   sono figlie di Progman. La finestra dello sfondo diventa figlia di Progman,
//!   messa tra le icone (sopra) e la WorkerW (sotto).

use windows::core::{w, BOOL};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct Embed {
    pub parent: HWND,
}

unsafe extern "system" fn find_icons_host(top: HWND, out: LPARAM) -> BOOL {
    if FindWindowExW(Some(top), None, w!("SHELLDLL_DefView"), None).is_ok() {
        // la WorkerW giusta è la finestra di primo livello subito dopo
        if let Ok(worker) = FindWindowExW(None, Some(top), w!("WorkerW"), None) {
            *(out.0 as *mut HWND) = worker;
        }
    }
    BOOL(1)
}

unsafe extern "system" fn cerca_icone(top: HWND, out: LPARAM) -> BOOL {
    if FindWindowExW(Some(top), None, w!("SHELLDLL_DefView"), None).is_ok() {
        *(out.0 as *mut bool) = true;
        return BOOL(0);
    }
    BOOL(1)
}

/// Explorer ha già fatto il desktop (Progman con le icone, SHELLDLL_DefView, dentro di lui o
/// in una WorkerW) e la barra delle applicazioni (dove va l'icona)? All'accesso l'app può
/// arrivare prima di lui.
pub fn pronto() -> bool {
    unsafe {
        let Ok(progman) = FindWindowW(w!("Progman"), None) else { return false };
        if FindWindowW(w!("Shell_TrayWnd"), None).is_err() {
            return false;
        }
        if FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), None).is_ok() {
            return true;
        }
        let mut trovate = false;
        let _ = EnumWindows(Some(cerca_icone), LPARAM(&mut trovate as *mut bool as isize));
        trovate
    }
}

/// Rende `hwnd` figlia del desktop. Restituisce la finestra genitore usata.
pub unsafe fn attach(hwnd: HWND) -> Option<Embed> {
    let progman = FindWindowW(w!("Progman"), None).ok()?;
    // chiede a Explorer di creare la WorkerW dietro le icone
    let mut res = 0usize;
    SendMessageTimeoutW(progman, 0x052C, Default::default(), Default::default(), SMTO_NORMAL, 1000, Some(&mut res));
    SendMessageTimeoutW(progman, 0x052C, windows::Win32::Foundation::WPARAM(0xD), LPARAM(1), SMTO_NORMAL, 1000, Some(&mut res));

    let shell_in_progman = FindWindowExW(Some(progman), None, w!("SHELLDLL_DefView"), None).ok();
    let worker_in_progman = FindWindowExW(Some(progman), None, w!("WorkerW"), None).ok();

    // stile da finestra figlia
    let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
    SetWindowLongPtrW(hwnd, GWL_STYLE, (style & !(WS_POPUP.0 as isize)) | WS_CHILD.0 as isize);

    if let (Some(shell), Some(worker)) = (shell_in_progman, worker_in_progman) {
        // Windows 11 24H2+
        SetParent(hwnd, Some(progman)).ok()?;
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        let _ = SetWindowPos(hwnd, Some(shell), 0, 0, 0, 0, flags);
        // la WorkerW (sfondo fisso) va in fondo, non subito sotto questa finestra:
        // con più schermi le finestre agganciate prima finirebbero dietro di lei
        let _ = SetWindowPos(worker, Some(HWND_BOTTOM), 0, 0, 0, 0, flags);
        return Some(Embed { parent: progman });
    }

    let mut worker = HWND::default();
    let _ = EnumWindows(Some(find_icons_host), LPARAM(&mut worker as *mut HWND as isize));
    let parent = if worker.is_invalid() { progman } else { worker };
    SetParent(hwnd, Some(parent)).ok()?;
    Some(Embed { parent })
}

/// Windows 11 24H2+: la WorkerW con lo sfondo fisso deve restare l'ultima figlia di
/// Progman, sotto gli sfondi animati. Dopo il salvaschermo (e a volte dopo il blocco
/// o un cambio di sfondo) Explorer la rimette subito sotto le icone, sopra gli animali,
/// che restano accesi ma coperti. Se è successo la si rimanda in fondo.
/// Sono due chiamate e non manda messaggi: si può fare spesso.
pub unsafe fn riordina(parent: HWND) {
    let Ok(worker) = FindWindowExW(Some(parent), None, w!("WorkerW"), None) else {
        return;
    };
    if GetWindow(worker, GW_HWNDNEXT).is_ok_and(|h| !h.is_invalid()) {
        let _ = SetWindowPos(worker, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Porta la finestra a coprire `screen` (coordinate dello schermo, pixel fisici).
pub unsafe fn fit(hwnd: HWND, parent: HWND, screen: RECT) {
    let mut pr = RECT::default();
    let _ = GetWindowRect(parent, &mut pr);
    let _ = SetWindowPos(
        hwnd,
        None,
        screen.left - pr.left,
        screen.top - pr.top,
        screen.right - screen.left,
        screen.bottom - screen.top,
        SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
}
