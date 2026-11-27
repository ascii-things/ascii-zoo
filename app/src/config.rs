//! Impostazioni salvate in %APPDATA%\AsciiZoo\config.json

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// lunghezza massima del nome di un animale (lettere)
pub const NOME_MAX: usize = 16;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    /// 0 = a rotazione, 1.. = uno sfondo fisso: la sua posizione in shared::specie() (gli
    /// animali di serie, poi i paesaggi, poi le scene dell'utente)
    pub animale: u32,
    /// quanti animali di serie c'erano quando si sono salvate le posizioni di `animale` e
    /// `animale_schermi`: con un animale nuovo i paesaggi e le scene dopo di lui si spostano
    /// (vedi `migra`). Manca nei file fino alla 0.8: erano 9
    #[serde(default = "animali_fino_alla_08")]
    pub animali_di_serie: u32,
    /// lo stesso per i paesaggi (vengono dopo gli animali e prima delle scene dell'utente):
    /// manca nei file fino alla 0.9, che ne aveva 6
    #[serde(default = "paesaggi_fino_alla_09")]
    pub paesaggi_di_serie: u32,
    /// con "a rotazione": koi nello stagno quando il PC è in carica
    pub koi_in_carica: bool,
    /// ferma l'animazione quando il PC va a batteria
    pub pausa_batteria: bool,
    /// ferma l'animazione quando un'app (gioco, video) è a schermo intero
    pub pausa_schermo_intero: bool,
    /// ferma l'animazione di uno schermo quando le finestre ne coprono tutto il desktop (su Linux
    /// non si distingue da un'app a schermo intero: basta una delle due)
    pub pausa_coperto: bool,
    /// nome dell'animale più grande, per specie (id della specie → nome; "" = nessuno)
    pub nomi: BTreeMap<String, String>,
    /// ogni quanti secondi cambia animale "a rotazione"
    pub rotazione_secondi: u32,
    /// specie tolte dalla rotazione
    pub rotazione_escluse: Vec<String>,
    /// con due o più schermi, gli sfondi tolti dalla rotazione su uno schermo (id dello schermo
    /// → specie); chi non c'è segue rotazione_escluse
    pub rotazione_escluse_schermi: BTreeMap<String, Vec<String>>,
    /// widget: nota sul desktop ("" = nessuna)
    pub nota: String,
    /// widget: conto alla rovescia, data "aaaa-mm-gg" ("" = spento)
    pub conto_data: String,
    pub conto_etichetta: String,
    /// widget: CPU, RAM e batteria
    pub widget_statistiche: bool,
    /// widget "sistema": le statistiche per nerd accese (id di nerd::VOCI)
    pub widget_nerd: Vec<String>,
    /// aspetto dei widget: "piccoli", "normali", "grandi"
    pub widget_dimensione: String,
    /// cornice dei riquadri: "ascii" (+-|), "linee" (┌─┐), "arrotondata" (╭─╮)
    pub widget_cornice: String,
    /// riquadri con lo sfondo pieno (se no trasparenti, con gli animali che passano dietro)
    pub widget_sfondo_pieno: bool,
    /// widget "In riproduzione" (spento di serie: su macOS chiede il permesso Automazione)
    pub widget_musica: bool,
    /// pomodoro: minuti di lavoro, di pausa e di pausa lunga; pausa lunga ogni quanti
    /// pomodori (0 = mai)
    pub pomodoro_lavoro: u32,
    pub pomodoro_pausa: u32,
    pub pomodoro_lunga: u32,
    pub pomodoro_ogni: u32,
    /// angolo dei widget: "basso-destra", "basso-sinistra", "alto-destra", "alto-sinistra"
    pub widget_angolo: String,
    /// su quale schermo stanno i widget: "" = il principale, "*" = tutti, se no l'id dello schermo
    pub widget_schermo: String,
    /// animale per schermo (id dello schermo → come "animale"); chi non c'è segue "animale"
    pub animale_schermi: BTreeMap<String, u32>,
    /// un clic su una zona vuota del desktop arriva agli animali (su Linux serve l'estensione
    /// di GNOME Shell in linux/gnome)
    pub clic_desktop: bool,
    /// l'orologio con la data al centro dello schermo (spento: gli animali e la scenografia
    /// usano anche il centro)
    pub orologio: bool,
    /// dal 1° dicembre al 6 gennaio un po' di neve cade su tutti gli sfondi (la pagina guarda la data)
    pub neve_feste: bool,
    /// tema dello sfondo: "scuro", "chiaro" o "sistema" (segue il tema chiaro o scuro del sistema)
    pub tema: String,
    /// fotogrammi al secondo dello sfondo: 30, 20 o 15 (vedi shared::FOTOGRAMMI)
    pub fotogrammi: u32,
    /// solo Linux: minuti senza toccare mouse e tastiera prima che parta lo screensaver
    /// (0 = spento, di serie: un aggiornamento non lo accende a chi non l'ha chiesto). Su
    /// Windows l'attesa la decide Windows.
    pub salvaschermo_minuti: u32,
    /// solo Linux: la scena dello screensaver, un id di ascii_dreams::scene() oppure "rotazione"
    pub salvaschermo_scena: String,
    /// una volta al giorno si chiede a GitHub se c'è una versione nuova (vedi aggiornamenti.rs)
    pub controlla_aggiornamenti: bool,
    /// quando lo si è chiesto l'ultima volta (secondi dal 1970, 0 = mai)
    pub aggiornamenti_ultimo: u64,
    /// l'ultima versione trovata su GitHub ("" = non si sa): resta nel menu anche dopo un
    /// riavvio, finché questa non è più vecchia
    pub versione_disponibile: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            animale: 0,
            animali_di_serie: crate::shared::SPECIE.len() as u32,
            paesaggi_di_serie: crate::shared::PAESAGGI.len() as u32,
            koi_in_carica: true,
            pausa_batteria: true,
            pausa_schermo_intero: true,
            pausa_coperto: true,
            nomi: BTreeMap::from([("koi".to_string(), "KARPISA".to_string())]),
            rotazione_secondi: 120,
            rotazione_escluse: Vec::new(),
            rotazione_escluse_schermi: BTreeMap::new(),
            nota: String::new(),
            conto_data: String::new(),
            conto_etichetta: String::new(),
            widget_statistiche: false,
            widget_nerd: Vec::new(),
            widget_dimensione: "normali".into(),
            widget_cornice: "ascii".into(),
            widget_sfondo_pieno: false,
            widget_musica: false,
            pomodoro_lavoro: 25,
            pomodoro_pausa: 5,
            pomodoro_lunga: 15,
            pomodoro_ogni: 4,
            widget_angolo: "basso-destra".into(),
            widget_schermo: String::new(),
            animale_schermi: BTreeMap::new(),
            clic_desktop: true,
            orologio: true,
            neve_feste: true,
            tema: "scuro".into(),
            fotogrammi: 30,
            salvaschermo_minuti: 0,
            salvaschermo_scena: "rotazione".into(),
            controlla_aggiornamenti: true,
            aggiornamenti_ultimo: 0,
            versione_disponibile: String::new(),
        }
    }
}

/// pulisce un nome scritto dall'utente: niente caratteri di controllo, spazi ai lati tolti, al massimo NOME_MAX lettere
pub fn pulisci_nome(s: &str) -> String {
    pulisci_testo(s, NOME_MAX)
}

/// lunghezza massima della nota e dell'etichetta del conto alla rovescia
pub const NOTA_MAX: usize = 200;
pub const ETICHETTA_MAX: usize = 40;

/// toglie i caratteri di controllo e taglia a `max` lettere
pub fn pulisci_testo(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(max).collect()
}

/// Legge una data scritta dall'utente: "25/12/2026", "25-12-2026", "25.12.2026"
/// oppure "2026-12-25". Restituisce "aaaa-mm-gg"; None se non è una data valida.
pub fn leggi_data(s: &str) -> Option<String> {
    let parti: Vec<u32> = s.trim().split(['/', '-', '.']).map(|p| p.trim().parse().ok()).collect::<Option<_>>()?;
    let [a, b, c] = parti[..] else { return None };
    let (g, m, y) = if a > 31 { (c, b, a) } else { (a, b, c) };
    let y = if y < 100 { 2000 + y } else { y };
    let bisestile = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let giorni = [31, if bisestile { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    (1..=12).contains(&m).then_some(())?;
    (1..=giorni[m as usize - 1]).contains(&g).then_some(())?;
    (2000..=2200).contains(&y).then(|| format!("{y:04}-{m:02}-{g:02}"))
}

/// "2026-12-25" -> "25/12/2026", per mostrarla nel menu e nella finestrella
pub fn data_italiana(iso: &str) -> String {
    let p: Vec<&str> = iso.split('-').collect();
    if p.len() == 3 { format!("{}/{}/{}", p[2], p[1], p[0]) } else { iso.to_string() }
}

/// Un eseguibile che non resta dov'è: compilato da cargo (`target/debug`, `target/release`),
/// nella cartella temporanea o, su macOS, aperto dal DMG (`/Volumes`) o "traslocato" da
/// Gatekeeper (`AppTranslocation`). L'avvio automatico non lo si sposta lì da solo.
pub fn provvisorio(exe: &std::path::Path) -> bool {
    let nomi: Vec<String> = exe.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect();
    let build = |n: &String| n == "debug" || n == "release";
    // target/release/ascii-zoo oppure target/<piattaforma>/release/ascii-zoo
    let cargo = nomi.windows(2).any(|w| w[0] == "target" && build(&w[1]))
        || nomi.windows(3).any(|w| w[0] == "target" && build(&w[2]));
    cargo || exe.starts_with(std::env::temp_dir()) || mac_volatile(exe)
}

/// macOS: l'app aperta dal DMG o traslocata da Gatekeeper sparisce o cambia percorso
fn mac_volatile(exe: &std::path::Path) -> bool {
    cfg!(target_os = "macos")
        && (exe.starts_with("/Volumes") || exe.components().any(|c| c.as_os_str() == "AppTranslocation"))
}

/// Windows: %APPDATA%\AsciiZoo\config.json
#[cfg(windows)]
fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("AsciiZoo").join("config.json")
}

/// macOS: ~/Library/Application Support/AsciiZoo/config.json
#[cfg(target_os = "macos")]
fn path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Library").join("Application Support").join("AsciiZoo").join("config.json")
}

/// Linux: $XDG_CONFIG_HOME/AsciiZoo/config.json (di solito ~/.config/AsciiZoo/config.json)
#[cfg(target_os = "linux")]
fn path() -> PathBuf {
    linux_dir::config().join("AsciiZoo").join("config.json")
}

/// cartelle dell'utente su Linux, secondo XDG
#[cfg(target_os = "linux")]
pub mod linux_dir {
    use std::path::PathBuf;

    fn home() -> PathBuf {
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
    }

    /// $XDG_CONFIG_HOME, se no ~/.config
    pub fn config() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home().join(".config"))
    }
}

fn animali_fino_alla_08() -> u32 {
    9
}

fn paesaggi_fino_alla_09() -> u32 {
    6
}

impl Config {
    pub fn load() -> Self {
        Self::da_json(&std::fs::read_to_string(path()).unwrap_or_default())
    }

    /// le impostazioni scritte in config.json; quelle di serie se il file non si legge
    fn da_json(testo: &str) -> Self {
        // un file salvato con il BOM (es. da Blocco note) non deve azzerare tutto
        let mut cfg: Self = serde_json::from_str(testo.trim_start_matches('\u{feff}')).unwrap_or_default();
        cfg.migra();
        cfg
    }

    /// Posizioni salvate con meno animali o paesaggi di serie (una versione vecchia): un
    /// paesaggio va avanti di quanti animali sono arrivati, una scena dell'utente di quanti
    /// animali e paesaggi sono arrivati, così resta lo stesso sfondo. Gli animali restano
    /// dov'erano: i nuovi vengono dopo, come i paesaggi nuovi dopo gli altri paesaggi.
    fn migra(&mut self) {
        let (a1, p1) = (crate::shared::SPECIE.len() as u32, crate::shared::PAESAGGI.len() as u32);
        let (a0, p0) = (self.animali_di_serie, self.paesaggi_di_serie);
        // uguale, o scritto da una versione più nuova: non si tocca
        if a0 > a1 || p0 > p1 || (a0 == a1 && p0 == p1) {
            return;
        }
        let sposta = |p: &mut u32| {
            if *p > a0 + p0 {
                *p += (a1 - a0) + (p1 - p0);
            } else if *p > a0 {
                *p += a1 - a0;
            }
        };
        sposta(&mut self.animale);
        self.animale_schermi.values_mut().for_each(sposta);
        self.animali_di_serie = a1;
        self.paesaggi_di_serie = p1;
    }

    pub fn save(&self) {
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, s);
        }
    }
}

/// Avvio con Windows: due modi insieme, tutti e due con `--accesso`.
/// - un'attività pianificata "ASCII Zoo" all'accesso dell'utente: parte appena si entra,
///   senza aspettare i secondi che Windows lascia passare prima dei programmi in Run;
/// - il valore "AsciiZoo" nella chiave Run dell'utente corrente, come prima: se l'attività
///   non c'è (Windows non l'ha accettata) o non parte, l'app parte da lì.
///
/// Con `--accesso` una copia che trova l'app già avviata cede il passo e si chiude (win.rs):
/// arrivi prima l'una o l'altra, parte una volta sola.
#[cfg(windows)]
pub mod autostart {
    use std::os::windows::process::CommandExt;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
    use winreg::RegKey;

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const NAME: &str = "AsciiZoo";
    const ATTIVITA: &str = "ASCII Zoo";
    /// l'argomento degli avvii automatici (vedi win.rs)
    pub const ACCESSO: &str = "--accesso";

    fn run_attuale() -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN, KEY_READ).and_then(|k| k.get_value::<String, _>(NAME)).ok()
    }

    /// Il valore nella chiave Run, che si crea se non c'è (su un profilo appena fatto può
    /// mancare: aprirla e basta falliva, e l'avvio con Windows non partiva, senza dire niente)
    fn scrivi_run(valore: &str) {
        match RegKey::predef(HKEY_CURRENT_USER).create_subkey_with_flags(RUN, KEY_WRITE) {
            Ok((key, _)) => {
                if let Err(e) = key.set_value(NAME, &valore.to_string()) {
                    crate::shared::zoo_log!("avvio con Windows: chiave Run non scritta ({e})");
                }
            }
            Err(e) => crate::shared::zoo_log!("avvio con Windows: chiave Run non aperta ({e})"),
        }
    }

    fn valore_run(exe: &std::path::Path) -> String {
        format!("\"{}\" {ACCESSO}", exe.display())
    }

    /// schtasks.exe di System32, senza la finestra nera della console
    fn schtasks() -> std::process::Command {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let mut c = std::process::Command::new(format!(r"{root}\System32\schtasks.exe"));
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        c
    }

    /// l'XML dell'attività, se c'è
    fn attivita() -> Option<String> {
        let o = schtasks().args(["/Query", "/TN", ATTIVITA, "/XML"]).output().ok()?;
        o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
    }

    fn xml(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
    }

    /// L'attività all'accesso di questo utente (un'attività "per tutti" vorrebbe i permessi di
    /// amministratore). Niente limite di durata (di serie sono 3 giorni), parte e resta anche a
    /// batteria, priorità normale (di serie sarebbe più bassa), una sola copia.
    fn testo_attivita(exe: &std::path::Path) -> String {
        let utente = xml(&format!(
            "{}\\{}",
            std::env::var("USERDOMAIN").unwrap_or_default(),
            std::env::var("USERNAME").unwrap_or_default()
        ));
        let cartella = exe.parent().map(|p| xml(&p.display().to_string())).unwrap_or_default();
        let comando = xml(&exe.display().to_string());
        format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>ASCII Zoo: lo sfondo animato, appena si entra in Windows (si toglie dal menu dell'icona, "Avvia con Windows")</Description>
  </RegistrationInfo>
  <Triggers>
    <LogonTrigger>
      <Enabled>true</Enabled>
      <UserId>{utente}</UserId>
    </LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <UserId>{utente}</UserId>
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <IdleSettings>
      <StopOnIdleEnd>false</StopOnIdleEnd>
      <RestartOnIdle>false</RestartOnIdle>
    </IdleSettings>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <RunOnlyIfIdle>false</RunOnlyIfIdle>
    <WakeToRun>false</WakeToRun>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>{comando}</Command>
      <Arguments>{ACCESSO}</Arguments>
      <WorkingDirectory>{cartella}</WorkingDirectory>
    </Exec>
  </Actions>
</Task>
"#
        )
    }

    /// registra (o rifà) l'attività per `exe`; false se Windows non l'accetta
    fn registra(exe: &std::path::Path) -> bool {
        let testo = testo_attivita(exe);
        // schtasks vuole il file in UTF-16 con il BOM
        let mut byte = vec![0xFF, 0xFE];
        for u in testo.encode_utf16() {
            byte.extend_from_slice(&u.to_le_bytes());
        }
        let file = std::env::temp_dir().join(format!("ascii-zoo-attivita-{}.xml", std::process::id()));
        if std::fs::write(&file, byte).is_err() {
            return false;
        }
        let r = schtasks().args(["/Create", "/TN", ATTIVITA, "/F", "/XML"]).arg(&file).output();
        let _ = std::fs::remove_file(&file);
        let ok = r.as_ref().is_ok_and(|o| o.status.success());
        crate::shared::zoo_log!(
            "avvio con Windows: attività pianificata {}",
            if ok { "registrata".to_string() } else { format!("non accettata ({:?}): resta la chiave Run", r.map(|o| o.status)) }
        );
        ok
    }

    fn togli_attivita() {
        let _ = schtasks().args(["/Delete", "/TN", ATTIVITA, "/F"]).output();
    }

    /// c'è l'attività? Letto dal registro (la cache di Task Scheduler, leggibile da tutti):
    /// niente schtasks, che costerebbe un processo mentre si costruisce il menu all'avvio
    fn attivita_registrata() -> bool {
        use winreg::enums::HKEY_LOCAL_MACHINE;
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey_with_flags(format!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\TaskCache\Tree\{ATTIVITA}"), KEY_READ)
            .is_ok()
    }

    pub fn enabled() -> bool {
        run_attuale().is_some() || attivita_registrata()
    }

    pub fn set(on: bool) {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            scrivi_run(&valore_run(&exe));
            registra(&exe);
        } else {
            if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN, KEY_WRITE) {
                let _ = key.delete_value(NAME);
            }
            togli_attivita();
        }
    }

    /// L'exe non si installa: una versione nuova può stare in un'altra cartella. Se l'avvio
    /// con Windows è attivo (in uno dei due modi) ma lancia un altro exe, o è quello di una
    /// versione fino alla 0.9 (solo la chiave Run, senza `--accesso`), si rifanno tutti e due
    /// per questo. Spento resta spento. Chiama schtasks: va fatto fuori dal thread principale.
    pub fn aggiorna() {
        let Ok(exe) = std::env::current_exe() else { return };
        if super::provvisorio(&exe) {
            return;
        }
        let run = run_attuale();
        let att = attivita();
        if run.is_none() && att.is_none() {
            return;
        }
        let giusto = valore_run(&exe);
        if !run.as_deref().is_some_and(|r| r.eq_ignore_ascii_case(&giusto)) {
            scrivi_run(&giusto);
        }
        // l'attività: c'è e lancia questo exe? (confronto senza maiuscole, come i percorsi di Windows)
        let comando = format!("<Command>{}</Command>", xml(&exe.display().to_string())).to_lowercase();
        if !att.is_some_and(|a| a.to_lowercase().contains(&comando)) {
            registra(&exe);
        }
    }

    #[cfg(test)]
    mod test {
        #[test]
        fn xml_attivita() {
            let t = super::testo_attivita(std::path::Path::new(r"C:\Giochi & Co\ascii-zoo.exe"));
            assert!(t.contains(r"<Command>C:\Giochi &amp; Co\ascii-zoo.exe</Command>"), "{t}");
            assert!(t.contains("<WorkingDirectory>C:\\Giochi &amp; Co</WorkingDirectory>"), "{t}");
            assert!(t.contains("<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>"));
            assert!(t.contains("<Arguments>--accesso</Arguments>"));
        }
    }
}

/// Apri al login su macOS: un LaunchAgent dell'utente,
/// ~/Library/LaunchAgents/io.github.tressilloide.asciizoo.plist
#[cfg(target_os = "macos")]
pub mod autostart {
    use std::path::PathBuf;

    const LABEL: &str = "io.github.tressilloide.asciizoo";

    fn plist() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library").join("LaunchAgents").join(format!("{LABEL}.plist")))
    }

    /// i caratteri speciali dell'XML nel percorso dell'app
    fn xml(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    pub fn enabled() -> bool {
        plist().is_some_and(|p| p.exists())
    }

    pub fn set(on: bool) {
        let Some(p) = plist() else { return };
        if !on {
            let _ = std::fs::remove_file(p);
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        // dal DMG o traslocata: al prossimo accesso quel percorso non c'è più
        if super::mac_volatile(&exe) {
            return;
        }
        let body = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>ProcessType</key>
  <string>Interactive</string>
</dict>
</plist>
"#,
            xml(&exe.to_string_lossy())
        );
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, body);
    }

    /// Una versione nuova può stare in un'altra .app: se "Apri al login" è attivo ma il
    /// LaunchAgent lancia un altro programma, lo si fa puntare a questo, la copia appena
    /// avviata. Spento resta spento.
    pub fn aggiorna() {
        let Some(p) = plist() else { return };
        let Ok(attuale) = std::fs::read_to_string(&p) else { return };
        let Ok(exe) = std::env::current_exe() else { return };
        let riga = format!("<string>{}</string>", xml(&exe.to_string_lossy()));
        if !attuale.contains(&riga) && !super::provvisorio(&exe) {
            set(true);
        }
    }
}

/// Apri al login su Linux: ~/.config/autostart/ascii-zoo.desktop (standard XDG,
/// vale per GNOME, KDE, XFCE…). Lo stesso file lo scrive anche linux/install.sh.
#[cfg(target_os = "linux")]
pub mod autostart {
    use std::path::PathBuf;

    fn file() -> PathBuf {
        super::linux_dir::config().join("autostart").join("ascii-zoo.desktop")
    }

    /// argomento di Exec= secondo la specifica Desktop Entry: tra virgolette, con
    /// \ " ` $ preceduti da \ (e il \ va raddoppiato perché il file è un "string")
    fn exec(p: &str) -> String {
        let mut s = String::from("\"");
        for c in p.chars() {
            match c {
                '"' | '`' | '$' => {
                    s.push_str("\\\\");
                    s.push(c);
                }
                '\\' => s.push_str("\\\\\\\\"),
                '%' => s.push_str("%%"), // in Exec= "%" introduce i codici di campo
                '\n' | '\r' => {}
                _ => s.push(c),
            }
        }
        s.push('"');
        s
    }

    pub fn enabled() -> bool {
        file().exists()
    }

    pub fn set(on: bool) {
        let p = file();
        if !on {
            let _ = std::fs::remove_file(p);
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        let body = format!(
            "[Desktop Entry]\nType=Application\nName=ASCII Zoo\nComment=Sfondo animato con animali ASCII\nExec={}\nIcon=ascii-zoo\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            exec(&exe.to_string_lossy())
        );
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, body);
    }

    /// Se "Apri al login" è attivo ma lancia un altro programma (una copia in un'altra
    /// cartella), lo si fa puntare a questo, la copia appena avviata. Spento resta spento.
    pub fn aggiorna() {
        let Ok(attuale) = std::fs::read_to_string(file()) else { return };
        let Ok(exe) = std::env::current_exe() else { return };
        let riga = format!("Exec={}", exec(&exe.to_string_lossy()));
        if !attuale.lines().any(|l| l.trim_end() == riga) && !super::provvisorio(&exe) {
            set(true);
        }
    }
}

/// la cartella delle impostazioni (quella di config.json)
pub fn cartella() -> PathBuf {
    path().parent().map(std::path::Path::to_path_buf).unwrap_or_default()
}

#[cfg(test)]
mod prove {
    use super::*;

    /// prima della 0.9 c'erano 9 animali: un paesaggio o una scena va avanti, un animale no
    #[test]
    fn posizioni_di_una_versione_vecchia() {
        let nuovi = crate::shared::SPECIE.len() as u32 - 9;
        let cfg = Config::da_json(r#"{"animale": 10, "animale_schermi": {"A": 4, "B": 9, "C": 16, "D": 0}}"#);
        assert_eq!(cfg.animale, 10 + nuovi, "Stelle resta Stelle");
        assert_eq!(cfg.animale_schermi["A"], 4, "Koi resta Koi");
        assert_eq!(cfg.animale_schermi["B"], 9, "Anatre resta Anatre");
        let paesaggi = crate::shared::PAESAGGI.len() as u32 - 6;
        assert_eq!(cfg.animale_schermi["C"], 16 + nuovi + paesaggi, "la prima scena dell'utente resta lei");
        assert_eq!(cfg.animale_schermi["D"], 0, "a rotazione resta a rotazione");
        assert_eq!(cfg.animali_di_serie, crate::shared::SPECIE.len() as u32);
    }

    /// un file nuovo o già sistemato non cambia, neanche riletto più volte
    #[test]
    fn posizioni_gia_giuste() {
        assert_eq!(Config::da_json("{}").animale, 0);
        assert_eq!(Config::da_json("non è json").animale, 0);
        let cfg = Config::da_json(r#"{"animale": 3}"#);
        let salvato = serde_json::to_string(&cfg).unwrap();
        let riletto = Config::da_json(&salvato);
        assert_eq!(riletto.animale, 3);
        let cfg = Config::da_json(r#"{"animale": 10}"#);
        let riletto = Config::da_json(&serde_json::to_string(&cfg).unwrap());
        assert_eq!(riletto.animale, cfg.animale, "si sposta una volta sola");
        assert_eq!(Config::default().animali_di_serie, crate::shared::SPECIE.len() as u32);
        assert_eq!(Config::default().paesaggi_di_serie, crate::shared::PAESAGGI.len() as u32);
    }

    /// dalla 0.9 (11 animali, 6 paesaggi): i paesaggi restano, le scene dell'utente vanno avanti
    #[test]
    fn posizioni_della_09() {
        let (a, p) = (crate::shared::SPECIE.len() as u32, crate::shared::PAESAGGI.len() as u32);
        let cfg = Config::da_json(r#"{"animale": 17, "animali_di_serie": 11, "animale_schermi": {"A": 12, "B": 18, "C": 11}}"#);
        assert_eq!(cfg.animale, 17 + (a - 11), "Mare di notte resta Mare di notte");
        assert_eq!(cfg.animale_schermi["A"], 12 + (a - 11), "Stelle resta Stelle");
        assert_eq!(cfg.animale_schermi["B"], 18 + (a - 11) + (p - 6), "la prima scena dell'utente resta lei");
        assert_eq!(cfg.animale_schermi["C"], 11, "Topi resta Topi");
        // scritto da una versione più nuova (più animali di questa): non si tocca
        let cfg = Config::da_json(r#"{"animale": 30, "animali_di_serie": 99, "paesaggi_di_serie": 6}"#);
        assert_eq!(cfg.animale, 30);
    }
}
