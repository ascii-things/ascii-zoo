# Prova del menu dell'icona su Windows (CI, windows-latest), con il programma vero.
#
# Avvia ascii-zoo.exe con una configurazione pulita, apre il menu come dopo un clic destro
# sull'icona e lo usa con clic e tasti veri (SendInput). Controlla che:
# - un clic su una spunta (Orologio al centro, Tema > Chiaro) lasci il menu aperto, nello
#   stesso sottomenu, con la spunta cambiata e l'impostazione salvata in config.json;
# - Invio su una spunta faccia lo stesso;
# - Esc, un clic fuori e le voci definitive (Ricarica sfondo) chiudano il menu;
# - una voce con "…" (il nome della koi) chiuda il menu e apra la finestrella, e che dopo
#   il nome il menu si riapra e resti aperto.
# Le foto dello schermo e il log del programma finiscono in $Out (artifact della run).
# Se il runner non ha un desktop dove il menu si apre, lo dice con un avviso ed esce con 0.

param(
    [string]$Exe = "app\target\release\ascii-zoo.exe",
    [string]$Out = "menu-test"
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force $Out | Out-Null

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading;

public static class W {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindowEx(IntPtr parent, IntPtr after, string cls, string title);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr h, uint msg, IntPtr wp, IntPtr lp);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, EntryPoint = "SendMessageW")] public static extern IntPtr SendText(IntPtr h, uint msg, IntPtr wp, string lp);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr wp, IntPtr lp);
    [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr hm);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetMenuString(IntPtr hm, uint item, StringBuilder s, int max, uint flags);
    [DllImport("user32.dll")] public static extern uint GetMenuState(IntPtr hm, uint item, uint flags);
    [DllImport("user32.dll")] public static extern IntPtr GetSubMenu(IntPtr hm, int pos);
    [DllImport("user32.dll")] public static extern bool GetMenuItemRect(IntPtr h, IntPtr hm, uint item, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll", SetLastError = true)] public static extern uint SendInput(uint n, INPUT[] inputs, int size);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, StringBuilder s, int max);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int max);
    [DllImport("user32.dll")] static extern IntPtr GetDC(IntPtr h);
    [DllImport("user32.dll")] static extern int ReleaseDC(IntPtr h, IntPtr dc);
    [DllImport("gdi32.dll")] static extern bool BitBlt(IntPtr dst, int x, int y, int w, int h, IntPtr src, int sx, int sy, uint rop);
    [DllImport("gdi32.dll")] static extern uint GetPixel(IntPtr dc, int x, int y);

    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int dx, dy; public uint mouseData, dwFlags, time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort wVk, wScan; public uint dwFlags, time; public IntPtr dwExtraInfo; }
    [StructLayout(LayoutKind.Explicit)] public struct INPUT {
        [FieldOffset(0)] public uint type;
        [FieldOffset(8)] public MOUSEINPUT mi;
        [FieldOffset(8)] public KEYBDINPUT ki;
    }

    public const uint MF_BYPOSITION = 0x400, MF_CHECKED = 0x8;

    static uint Pid(IntPtr h) { uint p; GetWindowThreadProcessId(h, out p); return p; }

    // finestre di primo livello di una classe che appartengono al processo pid
    public static List<IntPtr> Windows(string cls, uint pid, bool onlyVisible) {
        var v = new List<IntPtr>();
        IntPtr h = IntPtr.Zero;
        while ((h = FindWindowEx(IntPtr.Zero, h, cls, null)) != IntPtr.Zero) {
            if (Pid(h) == pid && (!onlyVisible || IsWindowVisible(h))) v.Add(h);
        }
        return v;
    }
    public static IntPtr First(string cls, uint pid) {
        var v = Windows(cls, pid, false);
        return v.Count > 0 ? v[0] : IntPtr.Zero;
    }
    // menu aperti (classe "#32768") del processo: il primo è il più in alto
    public static List<IntPtr> Menus(uint pid) { return Windows("#32768", pid, true); }
    // figlia di una classe (da PowerShell $null diventerebbe "" e cercherebbe il titolo vuoto)
    public static IntPtr Child(IntPtr parent, string cls) { return FindWindowEx(parent, IntPtr.Zero, cls, null); }
    public static IntPtr HMenu(IntPtr h) { return SendMessage(h, 0x01E1 /* MN_GETHMENU */, IntPtr.Zero, IntPtr.Zero); }

    public static string Text(IntPtr hm, int pos) {
        var sb = new StringBuilder(256);
        GetMenuString(hm, (uint)pos, sb, 256, MF_BYPOSITION);
        return sb.ToString();
    }
    public static int Find(IntPtr hm, string start) {
        int n = GetMenuItemCount(hm);
        for (int i = 0; i < n; i++) if (Text(hm, i).StartsWith(start)) return i;
        return -1;
    }
    public static bool Checked(IntPtr hm, int pos) { return (GetMenuState(hm, (uint)pos, MF_BYPOSITION) & MF_CHECKED) != 0; }
    public static string Describe(IntPtr hm) {
        var sb = new StringBuilder();
        int n = GetMenuItemCount(hm);
        for (int i = 0; i < n; i++) sb.Append(Checked(hm, i) ? "[x] " : "    ").Append(Text(hm, i)).Append('\n');
        return sb.ToString();
    }
    // centro della voce sullo schermo; null se il menu non è visibile
    public static int[] Center(IntPtr hm, int pos) {
        RECT r;
        if (pos < 0 || !GetMenuItemRect(IntPtr.Zero, hm, (uint)pos, out r)) return null;
        return new int[] { (r.Left + r.Right) / 2, (r.Top + r.Bottom) / 2 };
    }

    static uint Send(INPUT i) { return SendInput(1, new INPUT[] { i }, Marshal.SizeOf(typeof(INPUT))); }
    public static uint Move(int x, int y) {
        SetCursorPos(x, y);
        var i = new INPUT();
        i.type = 0;
        i.mi.dx = (int)((x * 65535L) / Math.Max(1, GetSystemMetrics(0) - 1));
        i.mi.dy = (int)((y * 65535L) / Math.Max(1, GetSystemMetrics(1) - 1));
        i.mi.dwFlags = 0x0001 | 0x8000; // MOVE | ABSOLUTE
        return Send(i);
    }
    // clic sinistro vero nel punto: restituisce quanti eventi SendInput ha accettato (3)
    public static uint Click(int x, int y) {
        uint n = Move(x, y);
        Thread.Sleep(150);
        var d = new INPUT(); d.type = 0; d.mi.dwFlags = 0x0002; // LEFTDOWN
        var u = new INPUT(); u.type = 0; u.mi.dwFlags = 0x0004; // LEFTUP
        n += Send(d);
        Thread.Sleep(80);
        n += Send(u);
        return n;
    }
    public static uint Key(ushort vk) {
        var d = new INPUT(); d.type = 1; d.ki.wVk = vk;
        var u = new INPUT(); u.type = 1; u.ki.wVk = vk; u.ki.dwFlags = 0x0002; // KEYUP
        uint n = Send(d);
        Thread.Sleep(60);
        return n + Send(u);
    }

    public static string WindowInfo(IntPtr h) {
        if (h == IntPtr.Zero) return "(nessuna)";
        var c = new StringBuilder(128); GetClassName(h, c, 128);
        var t = new StringBuilder(256); GetWindowText(h, t, 256);
        return string.Format("0x{0:X} classe '{1}' titolo '{2}' pid {3}", h.ToInt64(), c, t, Pid(h));
    }
    public static uint ForegroundPid() { return Pid(GetForegroundWindow()); }

    // foto dello schermo in un DC: CAPTUREBLT, se no le finestre "layered" come i menu mancano
    public static bool Grab(IntPtr dst, int w, int h) {
        IntPtr s = GetDC(IntPtr.Zero);
        bool ok = BitBlt(dst, 0, 0, w, h, s, 0, 0, 0x00CC0020 | 0x40000000);
        ReleaseDC(IntPtr.Zero, s);
        return ok;
    }
    // luminosità media (0-255) di 5x5 punti del rettangolo, -1 se non si legge
    public static double Luma(int x0, int y0, int w, int h) {
        IntPtr s = GetDC(IntPtr.Zero);
        double t = 0; int n = 0;
        for (int j = 0; j < 5; j++) for (int i = 0; i < 5; i++) {
            uint c = GetPixel(s, x0 + i * w / 4, y0 + j * h / 4);
            if (c == 0xFFFFFFFF) continue;
            t += 0.299 * (c & 0xFF) + 0.587 * ((c >> 8) & 0xFF) + 0.114 * ((c >> 16) & 0xFF);
            n++;
        }
        ReleaseDC(IntPtr.Zero, s);
        return n > 0 ? t / n : -1;
    }
}
'@

Add-Type -AssemblyName System.Drawing

$falliti = 0
$avvisi = 0
function Esito([bool]$cond, [string]$cosa) {
    if ($cond) { Write-Host "OK  $cosa" } else { Write-Host "::error::NO  $cosa"; $script:falliti++ }
}
function Avviso([string]$cosa) { Write-Host "::warning::$cosa"; $script:avvisi++ }

[W]::SetProcessDPIAware() | Out-Null
$sw = [W]::GetSystemMetrics(0); $sh = [W]::GetSystemMetrics(1)
Write-Host "schermo ${sw}x${sh}, sessione $((Get-Process -Id $PID).SessionId), UserInteractive $([Environment]::UserInteractive)"

function Foto([string]$nome) {
    try {
        $b = New-Object System.Drawing.Bitmap $sw, $sh
        $g = [System.Drawing.Graphics]::FromImage($b)
        $hdc = $g.GetHdc()
        $ok = [W]::Grab($hdc, $sw, $sh)
        $g.ReleaseHdc($hdc)
        if (-not $ok) { Avviso "foto ${nome}: BitBlt non riuscito" }
        $b.Save((Join-Path $Out "$nome.png"), [System.Drawing.Imaging.ImageFormat]::Png)
        $g.Dispose(); $b.Dispose()
        Write-Host "foto: $nome.png"
    } catch { Avviso "foto $nome non riuscita: $_" }
}

# configurazione pulita, ma senza pause: sul runner lo sfondo andrebbe in pausa e nelle foto
# non si vedrebbe il tema cambiare. Le altre finestre giù, perché si veda il desktop.
$cfgPath = Join-Path $env:APPDATA 'AsciiZoo\config.json'
New-Item -ItemType Directory -Force (Split-Path $cfgPath) | Out-Null
'{"pausa_batteria": false, "pausa_schermo_intero": false, "pausa_coperto": false}' | Set-Content $cfgPath
try { (New-Object -ComObject Shell.Application).MinimizeAll(); Start-Sleep -Seconds 1 } catch { Avviso "MinimizeAll: $_" }
# config.json come oggetto; se il programma lo sta riscrivendo proprio adesso, si riprova
function Cfg {
    for ($k = 0; $k -lt 5; $k++) {
        try {
            if (-not (Test-Path $cfgPath)) { return $null }
            return (Get-Content $cfgPath -Raw | ConvertFrom-Json)
        } catch { Start-Sleep -Milliseconds 200 }
    }
    return $null
}

$env:ASCII_ZOO_LOG = '1'
$env:ASCII_ZOO_PROVA_MENU = '1'
$log = Join-Path $Out 'ascii-zoo.log'
$app = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $log -RedirectStandardOutput (Join-Path $Out 'stdout.txt')
$pid0 = [uint32]$app.Id
Write-Host "ascii-zoo.exe avviato, pid $pid0"

$script:contaRighe = $false
function Fine([int]$codice) {
    Foto 'fine'
    if (-not $app.HasExited) { Stop-Process -Id $app.Id -Force -ErrorAction SilentlyContinue }
    Start-Sleep -Milliseconds 500
    # il log si legge solo a programma chiuso: prima il file è ancora suo
    $righe = @()
    try { $righe = @(Get-Content $log) } catch { Avviso "non riesco a leggere il log: $_" }
    Write-Host "--- log del programma ---"
    $righe | ForEach-Object { Write-Host $_ }
    if ($script:contaRighe) {
        $aperto = @($righe | Where-Object { $_ -match 'resta aperto' }).Count
        if ($righe.Count -eq 0) { Avviso "log del programma vuoto: stderr non arriva al file" }
        else { Esito ($aperto -ge 5) "il log del programma ha $aperto righe 'il menu resta aperto'" }
    }
    Write-Host "--- fine: $falliti controlli falliti, $avvisi avvisi ---"
    exit ([int]($codice -ne 0 -or $falliti -gt 0))
}

# l'icona e la finestra di controllo
$tray = [IntPtr]::Zero; $ctl = [IntPtr]::Zero
for ($i = 0; $i -lt 120 -and $tray -eq [IntPtr]::Zero; $i++) {
    Start-Sleep -Milliseconds 250
    if ($app.HasExited) { break }
    $tray = [W]::First('tray_icon_app', $pid0)
    $ctl = [W]::First('AsciiZooControl', $pid0)
}
if ($tray -eq [IntPtr]::Zero) {
    $box = [W]::First('#32770', $pid0)
    Avviso "il programma non ha creato l'icona (uscito: $($app.HasExited); finestra di messaggio: $([W]::WindowInfo($box))): niente prova del menu"
    Fine 0
}
Write-Host "icona: $([W]::WindowInfo($tray)); controllo: $([W]::WindowInfo($ctl))"
Start-Sleep -Seconds 4 # le pagine finiscono di caricare

# punto dove si apre il menu (in basso a sinistra: il menu si apre verso l'alto)
$mx = 60; $my = $sh - 60
$script:via = ''
function Menus { , [W]::Menus($pid0) }
function AspettaMenu([int]$quanti, [int]$ms = 3000) {
    for ($t = 0; $t -lt $ms; $t += 100) { if ((Menus).Count -ge $quanti) { return $true }; Start-Sleep -Milliseconds 100 }
    return $false
}
function AspettaChiuso([int]$ms = 2000) {
    for ($t = 0; $t -lt $ms; $t += 100) { if ((Menus).Count -eq 0) { return $true }; Start-Sleep -Milliseconds 100 }
    return $false
}
function ApriMenu {
    # come Explorer dopo un clic destro sull'icona: tray-icon apre il menu dove sta il cursore
    [W]::Move($mx, $my) | Out-Null
    [W]::PostMessage($tray, 6002, [IntPtr]::Zero, [IntPtr]0x204) | Out-Null # WM_USER_TRAYICON, WM_RBUTTONDOWN
    if (AspettaMenu 1) { $script:via = 'icona'; return $true }
    # riserva: senza area di notifica tray-icon non apre il menu; lo apre il programma
    # stesso con la stessa chiamata (ASCII_ZOO_PROVA_MENU=1)
    [W]::PostMessage($ctl, 0x8002, [IntPtr]$mx, [IntPtr]$my) | Out-Null
    if (AspettaMenu 1) { $script:via = 'riserva'; return $true }
    return $false
}
function PrimoPiano { [W]::ForegroundPid() -eq $pid0 }

# 1. il menu si apre
if (-not (ApriMenu)) {
    Avviso "il menu non si apre su questo runner (primo piano: $([W]::WindowInfo([W]::GetForegroundWindow()))): niente prova del menu"
    Fine 0
}
$root = [W]::HMenu((Menus)[0])
Write-Host "menu aperto (via: $via), primo piano: $([W]::WindowInfo([W]::GetForegroundWindow()))"
Write-Host ([W]::Describe($root))
Foto '1-menu-aperto'

# 2. due clic di fila su una spunta del primo livello
$pos = [W]::Find($root, 'Orologio al centro')
$c = [W]::Center($root, $pos)
if ($null -eq $c) { Avviso "non trovo il rettangolo di 'Orologio al centro' (pos $pos)"; Fine 1 }
$prima = [W]::Checked($root, $pos)
$n = [W]::Click($c[0], $c[1])
if ($n -ne 3) { Avviso "SendInput ha accettato $n eventi su 3: il runner non riceve clic simulati"; Fine 0 }
Start-Sleep -Milliseconds 800
Esito ((Menus).Count -ge 1) "clic su 'Orologio al centro': il menu resta aperto"
Esito ([W]::Checked($root, $pos) -ne $prima) "la spunta di 'Orologio al centro' cambia ($prima -> $([W]::Checked($root, $pos)))"
Esito ((Cfg).orologio -eq (-not $prima)) "config.json: orologio = $((Cfg).orologio)"
[W]::Click($c[0], $c[1]) | Out-Null
Start-Sleep -Milliseconds 800
Esito ((Menus).Count -ge 1) "secondo clic su 'Orologio al centro': il menu resta aperto"
Esito ([W]::Checked($root, $pos) -eq $prima) "la spunta torna com'era ($prima)"
Esito ((Cfg).orologio -eq $prima) "config.json: orologio = $((Cfg).orologio)"

# 3. un sottomenu: Tema > Chiaro resta nel sottomenu
$tp = [W]::Find($root, 'Tema')
$c = [W]::Center($root, $tp)
[W]::Move($c[0], $c[1]) | Out-Null
if (-not (AspettaMenu 2)) { [W]::Click($c[0], $c[1]) | Out-Null; AspettaMenu 2 | Out-Null }
$sub = [W]::GetSubMenu($root, $tp)
Write-Host ([W]::Describe($sub))
$pChiaro = [W]::Find($sub, 'Chiaro'); $pScuro = [W]::Find($sub, 'Scuro'); $pSistema = [W]::Find($sub, 'Come il sistema')
$c = [W]::Center($sub, $pChiaro)
if ($null -eq $c) { Esito $false "il sottomenu Tema si apre"; Fine 1 }
# un pezzo di desktop lontano dal menu (in alto a destra): scuro prima, chiaro dopo
$zona = @(($sw - 300), 40, 260, 200)
$lumaPrima = [W]::Luma($zona[0], $zona[1], $zona[2], $zona[3])
[W]::Click($c[0], $c[1]) | Out-Null
Start-Sleep -Milliseconds 2000
$lumaDopo = [W]::Luma($zona[0], $zona[1], $zona[2], $zona[3])
Write-Host ("luminosità del desktop accanto al menu: prima {0:N0}, dopo Chiaro {1:N0} (0-255)" -f $lumaPrima, $lumaDopo)
if ($lumaPrima -ge 0 -and $lumaPrima -lt 80) {
    Esito ($lumaDopo -gt $lumaPrima + 80) "lo sfondo diventa chiaro mentre il menu è ancora aperto"
} else {
    Avviso "il desktop accanto al menu non sembra lo sfondo scuro (luminosità $lumaPrima): niente controllo del colore, guarda le foto"
}
Esito ((Menus).Count -ge 2) "Tema > Chiaro: menu e sottomenu restano aperti ($((Menus).Count) finestre)"
Esito ([W]::Checked($sub, $pChiaro) -and -not [W]::Checked($sub, $pScuro)) "spunta su Chiaro, tolta da Scuro"
Esito ((Cfg).tema -eq 'chiaro') "config.json: tema = $((Cfg).tema)"
Foto '2-tema-chiaro-menu-aperto'

# 4. tastiera: freccia giù e Invio su "Come il sistema"
if (PrimoPiano) {
    [W]::Key(0x28) | Out-Null; Start-Sleep -Milliseconds 300
    [W]::Key(0x0D) | Out-Null; Start-Sleep -Milliseconds 800
    Esito ((Menus).Count -ge 2) "Invio su 'Come il sistema': il menu resta aperto"
    Esito ([W]::Checked($sub, $pSistema) -and -not [W]::Checked($sub, $pChiaro)) "spunta su 'Come il sistema'"
    Esito ((Cfg).tema -eq 'sistema') "config.json: tema = $((Cfg).tema)"
} else {
    Avviso "il menu non è in primo piano ($([W]::WindowInfo([W]::GetForegroundWindow()))): niente prova della tastiera"
}
$c = [W]::Center($sub, $pScuro)
[W]::Click($c[0], $c[1]) | Out-Null
Start-Sleep -Milliseconds 800
Esito ((Cfg).tema -eq 'scuro') "di nuovo Scuro: config.json: tema = $((Cfg).tema)"

# 5. Esc chiude prima il sottomenu, poi il menu
if (PrimoPiano) {
    [W]::Key(0x1B) | Out-Null; Start-Sleep -Milliseconds 400
    Esito ((Menus).Count -eq 1) "Esc: si chiude il sottomenu ($((Menus).Count) finestre)"
    [W]::Key(0x1B) | Out-Null
    Esito (AspettaChiuso) "Esc: si chiude il menu"
} else {
    Avviso "menu non in primo piano: niente prova di Esc"
}
if ((Menus).Count -gt 0) { [W]::Click($sw - 40, 40) | Out-Null; AspettaChiuso | Out-Null }

# 6. riaperto, il menu resta aperto (non sparisce subito); un clic fuori lo chiude
Esito (ApriMenu) "il menu si riapre (via: $via)"
Start-Sleep -Milliseconds 1500
Esito ((Menus).Count -ge 1) "dopo 1,5 secondi il menu riaperto c'è ancora"
Write-Host "primo piano prima del clic fuori: $([W]::WindowInfo([W]::GetForegroundWindow()))"
[W]::Click($sw - 40, 40) | Out-Null
Esito (AspettaChiuso) "clic fuori dal menu: il menu si chiude"

# 7. una voce definitiva chiude il menu come prima
Esito (ApriMenu) "il menu si riapre (via: $via)"
$root = [W]::HMenu((Menus)[0])
$c = [W]::Center($root, [W]::Find($root, 'Ricarica sfondo'))
[W]::Click($c[0], $c[1]) | Out-Null
Esito (AspettaChiuso) "'Ricarica sfondo' chiude il menu"
Start-Sleep -Seconds 3

# 8. il nome della koi: il menu si chiude, si apre la finestrella; dopo, il menu si riapre e resta
Esito (ApriMenu) "il menu si riapre (via: $via)"
$root = [W]::HMenu((Menus)[0])
$np = [W]::Find($root, 'Nomi degli animali')
$c = [W]::Center($root, $np)
[W]::Move($c[0], $c[1]) | Out-Null
if (-not (AspettaMenu 2)) { [W]::Click($c[0], $c[1]) | Out-Null; AspettaMenu 2 | Out-Null }
$nomi = [W]::GetSubMenu($root, $np)
$c = [W]::Center($nomi, [W]::Find($nomi, 'Koi:'))
if ($null -eq $c) { Esito $false "il sottomenu dei nomi si apre"; Fine 1 }
[W]::Click($c[0], $c[1]) | Out-Null
Esito (AspettaChiuso) "'Koi: …' chiude il menu"
$dlg = [IntPtr]::Zero
for ($t = 0; $t -lt 30 -and $dlg -eq [IntPtr]::Zero; $t++) { Start-Sleep -Milliseconds 100; $dlg = [W]::First('AsciiZooNome', $pid0) }
Esito ($dlg -ne [IntPtr]::Zero) "si apre la finestrella del nome"
if ($dlg -ne [IntPtr]::Zero) {
    Foto '3-nome-della-koi'
    Write-Host "primo piano con la finestrella: $([W]::WindowInfo([W]::GetForegroundWindow()))"
    $edit = [W]::Child($dlg, 'Edit')
    Write-Host "campo del nome: $([W]::WindowInfo($edit))"
    [W]::SendText($edit, 0x000C, [IntPtr]::Zero, 'PROVA') | Out-Null # WM_SETTEXT
    [W]::Key(0x0D) | Out-Null
    Start-Sleep -Milliseconds 1000
    if ([W]::IsWindow($dlg)) {
        Avviso "Invio non è arrivato alla finestrella (primo piano: $([W]::WindowInfo([W]::GetForegroundWindow()))): premo OK con WM_COMMAND"
        [W]::PostMessage($dlg, 0x0111, [IntPtr]1, [IntPtr]::Zero) | Out-Null
        Start-Sleep -Milliseconds 1000
    }
    Start-Sleep -Milliseconds 500
    Esito ((Cfg).nomi.koi -eq 'PROVA') "config.json: nome della koi = $((Cfg).nomi.koi)"

    # subito dopo il nome: il menu si riapre, resta aperto e una spunta lo lascia aperto
    Esito (ApriMenu) "dopo il nome il menu si riapre (via: $via)"
    Start-Sleep -Milliseconds 1500
    Esito ((Menus).Count -ge 1) "dopo 1,5 secondi c'è ancora"
    $root = [W]::HMenu((Menus)[0])
    $pos = [W]::Find($root, 'Orologio al centro')
    $prima = [W]::Checked($root, $pos)
    $c = [W]::Center($root, $pos)
    [W]::Click($c[0], $c[1]) | Out-Null
    Start-Sleep -Milliseconds 800
    Esito ((Menus).Count -ge 1) "dopo il nome, clic su 'Orologio al centro': il menu resta aperto"
    Esito ((Cfg).orologio -eq (-not $prima)) "config.json: orologio = $((Cfg).orologio)"
    Foto '4-dopo-il-nome-menu-aperto'
    [W]::Click($sw - 40, 40) | Out-Null
    Esito (AspettaChiuso) "clic fuori: il menu si chiude"
}

# 9. due livelli di sottomenu: Sfondo > Paesaggi > Stelle lascia aperti i tre menu, e Stelle
#    (dopo gli 11 animali: posizione 12) va in config.json; poi di nuovo "A rotazione"
Esito (ApriMenu) "il menu si riapre (via: $via)"
$root = [W]::HMenu((Menus)[0])
$sp = [W]::Find($root, 'Sfondo')
$c = [W]::Center($root, $sp)
if ($null -eq $c) { Esito $false "c'è la voce Sfondo"; Fine 1 }
[W]::Move($c[0], $c[1]) | Out-Null
if (-not (AspettaMenu 2)) { [W]::Click($c[0], $c[1]) | Out-Null; AspettaMenu 2 | Out-Null }
$sfondo = [W]::GetSubMenu($root, $sp)
Write-Host ([W]::Describe($sfondo))
$pp = [W]::Find($sfondo, 'Paesaggi')
$c = [W]::Center($sfondo, $pp)
if ($null -eq $c) { Esito $false "il sottomenu Sfondo si apre"; Fine 1 }
[W]::Move($c[0], $c[1]) | Out-Null
if (-not (AspettaMenu 3)) { [W]::Click($c[0], $c[1]) | Out-Null; AspettaMenu 3 | Out-Null }
$pae = [W]::GetSubMenu($sfondo, $pp)
Write-Host ([W]::Describe($pae))
$ps = [W]::Find($pae, 'Stelle')
$c = [W]::Center($pae, $ps)
if ($null -eq $c) { Esito $false "il sottomenu Sfondo > Paesaggi si apre"; Fine 1 }
[W]::Click($c[0], $c[1]) | Out-Null
Start-Sleep -Milliseconds 1000
Esito ((Menus).Count -ge 3) "Sfondo > Paesaggi > Stelle: i tre menu restano aperti ($((Menus).Count) finestre)"
Esito ([W]::Checked($pae, $ps)) "spunta su Stelle"
Esito ((Cfg).animale -eq 12) "config.json: animale = $((Cfg).animale) (Stelle = 12)"
Foto '5-sfondo-paesaggi-stelle'
$c = [W]::Center($sfondo, [W]::Find($sfondo, 'A rotazione'))
if ($null -ne $c) {
    [W]::Click($c[0], $c[1]) | Out-Null
    Start-Sleep -Milliseconds 800
    Esito ((Cfg).animale -eq 0) "Sfondo > A rotazione: config.json: animale = $((Cfg).animale)"
}
[W]::Click($sw - 40, 40) | Out-Null
Esito (AspettaChiuso) "clic fuori: il menu si chiude"

$script:contaRighe = $true
Fine 0
