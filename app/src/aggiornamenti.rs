//! Avviso di aggiornamento: al massimo una volta al giorno si chiede a GitHub qual è l'ultima
//! versione pubblicata; se è più nuova di questa, in cima al menu compare una voce che apre la
//! pagina per scaricarla. Non si scarica e non si installa niente da soli.
//!
//! La richiesta la fa `curl` (c'è di serie su Windows 10 e 11, su macOS e quasi sempre su
//! Linux; se manca, `wget` o python3): niente librerie di rete nel programma. A GitHub arriva solo la
//! richiesta della pagina pubblica dell'ultima versione, con "ascii-zoo/<versione>" come
//! nome del programma.

use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// la pagina dell'ultima versione (quella che si apre dal menu)
pub const PAGINA: &str = "https://github.com/ascii-things/ascii-zoo/releases/latest";
const API: &str = "https://api.github.com/repos/ascii-things/ascii-zoo/releases/latest";
/// ogni quanto si chiede (anche con il programma sempre acceso)
pub const OGNI: u64 = 24 * 3600;
/// dopo l'avvio si aspetta un po': all'accensione la rete può non esserci ancora, e lo
/// sfondo ha la precedenza
const DOPO_L_AVVIO: Duration = Duration::from_secs(45);
/// se non si è riusciti (niente rete, niente curl), si riprova dopo
const RIPROVA: Duration = Duration::from_secs(3600);

/// "v0.10.2" o "0.10.2" → (0, 10, 2); None se non è una versione
pub fn versione(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches(['v', 'V']);
    // "0.10.0-beta" vale come "0.10.0"
    let s = s.split(['-', '+']).next()?;
    let mut p = s.split('.').map(|x| x.parse::<u32>().ok());
    let v = (p.next()??, p.next().unwrap_or(Some(0))?, p.next().unwrap_or(Some(0))?);
    p.next().is_none().then_some(v)
}

/// la versione di questo programma
pub fn questa() -> (u32, u32, u32) {
    versione(env!("CARGO_PKG_VERSION")).unwrap_or((0, 0, 0))
}

/// `trovata` (com'è scritta su GitHub, es. "0.10.0") è più nuova di questa?
pub fn piu_nuova(trovata: &str) -> bool {
    versione(trovata).is_some_and(|v| v > questa())
}

/// secondi dal 1970 (l'ora del sistema)
pub fn adesso() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// il nome dell'ultima versione nella risposta di GitHub (il campo "tag_name")
fn da_risposta(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let tag = v.get("tag_name")?.as_str()?;
    let (a, b, c) = versione(tag)?;
    Some(format!("{a}.{b}.{c}"))
}

fn comando(prog: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut c = std::process::Command::new(prog);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // niente finestra nera della console
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c.stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    c
}

/// curl di sistema: quello di Windows sta in System32 (non lo si cerca nel PATH)
fn curl() -> String {
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        format!(r"{root}\System32\curl.exe")
    }
    #[cfg(target_os = "macos")]
    {
        "/usr/bin/curl".into()
    }
    #[cfg(target_os = "linux")]
    {
        "curl".into()
    }
}

/// la stessa richiesta in python3 (indirizzo e nome del programma come argomenti)
#[cfg(target_os = "linux")]
const PYTHON: &str = "import sys, urllib.request as u\n\
r = u.Request(sys.argv[1], headers={'User-Agent': sys.argv[2], 'Accept': 'application/vnd.github+json'})\n\
sys.stdout.write(u.urlopen(r, timeout=20).read().decode())\n";
#[cfg(not(target_os = "linux"))]
const PYTHON: &str = "";

/// chiede a GitHub l'ultima versione (bloccante, fino a ~20 secondi)
pub fn chiedi() -> Result<String, String> {
    // prova: una versione finta, senza rete (per vedere la voce che arriva a programma avviato)
    if let Ok(v) = std::env::var("ASCII_ZOO_AGGIORNAMENTI_FINTA") {
        return Ok(v);
    }
    let agente = format!("ascii-zoo/{}", env!("CARGO_PKG_VERSION"));
    let mut errori = Vec::new();
    let uscita = comando(&curl())
        .args(["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github+json", "-A", &agente, API])
        .output();
    match uscita {
        Ok(o) if o.status.success() => {
            return da_risposta(&String::from_utf8_lossy(&o.stdout)).ok_or_else(|| "risposta senza versione".into())
        }
        Ok(o) => errori.push(format!("curl: {}", o.status)),
        Err(e) => errori.push(format!("curl: {e}")),
    }
    // Linux senza curl: wget, se no python3 (c'è in tutte le Ubuntu e le Fedora)
    if cfg!(target_os = "linux") {
        let mut wget = comando("wget");
        wget.args(["-q", "-O", "-", "--timeout=20", "--tries=1", "--header=Accept: application/vnd.github+json"])
            .arg(format!("--user-agent={agente}"))
            .arg(API);
        let mut python = comando("python3");
        python.args(["-I", "-c", PYTHON, API, &agente]);
        for (nome, mut c) in [("wget", wget), ("python3", python)] {
            match c.output() {
                Ok(o) if o.status.success() => {
                    return da_risposta(&String::from_utf8_lossy(&o.stdout)).ok_or_else(|| "risposta senza versione".into())
                }
                Ok(o) => errori.push(format!("{nome}: {}", o.status)),
                Err(e) => errori.push(format!("{nome}: {e}")),
            }
        }
    }
    Err(errori.join(", "))
}

/// apre la pagina per scaricare l'ultima versione nel browser
pub fn apri_pagina() {
    #[cfg(windows)]
    let prog = "explorer";
    #[cfg(target_os = "macos")]
    let prog = "open";
    #[cfg(target_os = "linux")]
    let prog = "xdg-open";
    let _ = std::process::Command::new(prog).arg(PAGINA).spawn();
}

/// il controllo in corso o il prossimo: chi lo usa (Core::giro) lo fa avanzare ogni ~2 secondi
pub struct Controllo {
    avvio: Instant,
    /// prima di questo istante non si riprova (dopo un errore)
    non_prima: Option<Instant>,
    in_corso: Option<Receiver<Result<String, String>>>,
}

impl Default for Controllo {
    fn default() -> Self {
        Self { avvio: Instant::now(), non_prima: None, in_corso: None }
    }
}

impl Controllo {
    /// `ultimo`: quando si è chiesto l'ultima volta (secondi dal 1970, 0 = mai). Restituisce
    /// Some(versione trovata) quando una richiesta finisce bene.
    pub fn giro(&mut self, acceso: bool, ultimo: u64) -> Option<String> {
        if let Some(rx) = &self.in_corso {
            let r = match rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                Err(_) => Err("controllo interrotto".into()),
            };
            self.in_corso = None;
            return match r {
                Ok(v) => {
                    crate::shared::zoo_log!("aggiornamenti: ultima versione {v} (questa {})", env!("CARGO_PKG_VERSION"));
                    self.non_prima = None;
                    Some(v)
                }
                Err(e) => {
                    crate::shared::zoo_log!("aggiornamenti: controllo non riuscito ({e}), si riprova tra un'ora");
                    self.non_prima = Some(Instant::now() + RIPROVA);
                    None
                }
            };
        }
        let subito = std::env::var_os("ASCII_ZOO_AGGIORNAMENTI_SUBITO").is_some();
        let ora = adesso();
        // l'orologio tornato indietro (ultimo nel futuro) vale come "è ora"
        let scaduto = ultimo == 0 || ora < ultimo || ora - ultimo >= OGNI;
        if !acceso
            || !scaduto
            || (!subito && self.avvio.elapsed() < DOPO_L_AVVIO)
            || self.non_prima.is_some_and(|t| Instant::now() < t)
        {
            return None;
        }
        let (tx, rx) = channel();
        let partito = std::thread::Builder::new().name("aggiornamenti".into()).spawn(move || {
            let _ = tx.send(chiedi());
        });
        if partito.is_ok() {
            self.in_corso = Some(rx);
        } else {
            self.non_prima = Some(Instant::now() + RIPROVA);
        }
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn versioni() {
        assert_eq!(versione("v0.10.2"), Some((0, 10, 2)));
        assert_eq!(versione("0.9"), Some((0, 9, 0)));
        assert_eq!(versione("1"), Some((1, 0, 0)));
        assert_eq!(versione("v1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(versione("v1.2.3.4"), None);
        assert_eq!(versione("ultima"), None);
        assert_eq!(versione(""), None);
        // numeri, non lettere: 0.10 viene dopo 0.9
        assert!(versione("0.10.0") > versione("0.9.9"));
        assert!(!piu_nuova(env!("CARGO_PKG_VERSION")));
        assert!(piu_nuova("999.0.0"));
        assert!(!piu_nuova("0.1.0"));
        assert!(!piu_nuova("niente"));
    }

    #[test]
    fn risposta() {
        assert_eq!(da_risposta(r#"{"tag_name": "v0.10.0", "draft": false}"#).as_deref(), Some("0.10.0"));
        assert_eq!(da_risposta(r#"{"message": "Not Found"}"#), None);
        assert_eq!(da_risposta("<html>"), None);
    }

    /// la richiesta vera a GitHub: `cargo test -- --ignored aggiornamenti`
    #[test]
    #[ignore]
    fn chiedi_davvero() {
        let v = chiedi().expect("risposta da GitHub");
        assert!(versione(&v).is_some(), "{v}");
    }
}
