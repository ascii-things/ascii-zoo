//! ASCII Zoo: sfondo animato per Windows, macOS e Linux.
//!
//! - lo sfondo è l'HTML in assets/, incorporato nell'eseguibile
//! - su ogni monitor una finestra con dentro una WebView viene messa dietro le icone del desktop
//! - un'icona (area di notifica su Windows, barra dei menu su macOS, barra in alto su
//!   Linux) ha tutte le opzioni
//!
//! La parte comune (specie, menu, impostazioni) è in `shared`; quella che mette le
//! finestre sul desktop è in `win` (Windows), `mac` (macOS) e `linux` (Linux).

#![cfg_attr(windows, windows_subsystem = "windows")]

mod aggiornamenti;
mod config;
mod foto;
mod nerd;
mod pomodoro;
mod riproduzione;
mod scene;
mod shared;

#[cfg_attr(target_os = "macos", path = "dialog_mac.rs")]
#[cfg_attr(target_os = "linux", path = "dialog_linux.rs")]
mod dialog;
#[cfg_attr(target_os = "macos", path = "power_mac.rs")]
#[cfg_attr(target_os = "linux", path = "power_linux.rs")]
mod power;
#[cfg_attr(target_os = "macos", path = "musica_mac.rs")]
#[cfg_attr(target_os = "linux", path = "musica_linux.rs")]
mod musica;
#[cfg_attr(target_os = "macos", path = "gpu_mac.rs")]
#[cfg_attr(target_os = "linux", path = "gpu_linux.rs")]
mod gpu;
#[cfg_attr(target_os = "macos", path = "stats_mac.rs")]
#[cfg_attr(target_os = "linux", path = "stats_linux.rs")]
mod stats;
#[cfg_attr(target_os = "macos", path = "tema_mac.rs")]
#[cfg_attr(target_os = "linux", path = "tema_linux.rs")]
mod tema;

#[cfg(windows)]
mod desktop;
#[cfg(windows)]
mod win;

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
compile_error!("ASCII Zoo per ora funziona su Windows, macOS e Linux");

fn main() {
    shared::segna_avvio();
    // "ascii-zoo --version" scrive la versione ed esce: linux/install.sh lo usa per
    // controllare che il programma scaricato parta su quel sistema, librerie comprese
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("ascii-zoo {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    // "ascii-zoo --avvio on|off": accende o spegne l'avvio automatico come la voce del menu
    // (per le prove in CI e per chi installa da uno script); esce con 0 se ora è come chiesto
    if std::env::args().nth(1).as_deref() == Some("--avvio") {
        let on = std::env::args().nth(2).as_deref() == Some("on");
        config::autostart::set(on);
        std::process::exit(if config::autostart::enabled() == on { 0 } else { 1 });
    }
    if std::env::args().nth(1).as_deref() == Some("--statistiche") {
        nerd::stampa();
        return;
    }
    // Linux: lo screensaver, che l'app lancia da sé quando non si usa il PC (vedi linux.rs)
    #[cfg(target_os = "linux")]
    if std::env::args().nth(1).as_deref() == Some("--salvaschermo") {
        let args: Vec<String> = std::env::args().skip(2).collect();
        linux::salvaschermo(&args);
        return;
    }
    // Windows lancia lo screensaver (ascii-zoo.scr, cioè questo stesso programma)
    // con /s, /p o /c: lo fa il motore nativo di dreams/, non lo sfondo
    #[cfg(windows)]
    {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if ascii_dreams::chiamata_da_salvaschermo(&args) {
            ascii_dreams::avvia(&args);
            return;
        }
    }
    #[cfg(windows)]
    win::run();
    #[cfg(target_os = "macos")]
    mac::run();
    #[cfg(target_os = "linux")]
    linux::run();
}
