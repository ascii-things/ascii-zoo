#!/usr/bin/env bash
# Misura quanto pesa ASCII Zoo su questo PC Linux e scrive un rapporto da mandare a chi
# lo sviluppa: sistema, scheda video, WebKitGTK, schermi, le impostazioni dell'app che
# contano per le prestazioni (mai la nota, il conto alla rovescia o i nomi), e la CPU di
# ogni processo che c'entra in queste situazioni:
#   1. app chiusa (quanto lavora già GNOME Shell da solo)
#   2. app normale: su GNOME sotto Wayland con l'estensione dello sfondo, senza XWayland
#      (il rapporto dice quale modo ha usato)
#   3. solo se la 2 era senza XWayland: la stessa app su XWayland (ASCII_ZOO_SFONDO=x11), per
#      confrontare i due modi
#   4. app con WebKitGTK tutto in software (WEBKIT_DISABLE_COMPOSITING_MODE=1)
#   5. app normale a 15 e a 5 fotogrammi al secondo (ASCII_ZOO_FOTOGRAMMI, senza cambiare le
#      impostazioni): con le misure a 30 dice, processo per processo, quanto costa comunque
#      (fisso) e quanto ogni fotogramma in più
# Ognuna: 10 secondi per partire, poi 20 di misura, con i fotogrammi che la pagina ha fatto
# davvero. Alla fine l'app riparte come prima.
#
# Uso:  bash prestazioni.sh [percorso di ascii-zoo]
# Durante la prova il desktop deve vedersi: niente finestre a schermo intero o massimizzate
# (lo sfondo coperto va in pausa e la misura non direbbe niente).
set -u

RAPPORTO="$HOME/ascii-zoo-prestazioni.txt"
ATTESA=${ATTESA:-10}
DURATA=${DURATA:-20}
LOG=$(mktemp)
# le misure, una riga per processo: scenario nome core
MISURE=$(mktemp)
trap 'rm -f "$LOG" "$MISURE"' EXIT
SCENARIO=""

# --wayland c'era fino alla 0.6.0 (Wayland nativo come finestra normale): ora è il modo normale
if [ "${1:-}" = --wayland ]; then shift; fi
BIN="${1:-}"
if [ -z "$BIN" ]; then
  pid=$(pgrep -x ascii-zoo | head -1)
  if [ -n "$pid" ]; then BIN=$(readlink "/proc/$pid/exe"); fi
fi
[ -z "$BIN" ] && BIN=$(command -v ascii-zoo || true)
[ -z "$BIN" ] && [ -x "$HOME/.local/bin/ascii-zoo" ] && BIN="$HOME/.local/bin/ascii-zoo"
if [ -z "$BIN" ] || [ ! -x "$BIN" ]; then
  echo "Non trovo ascii-zoo: passa il percorso, es. bash prestazioni.sh ~/.local/bin/ascii-zoo" >&2
  exit 1
fi

NCPU=$(nproc)
HZ=$(getconf CLK_TCK)
PROCESSI="ascii-zoo WebKitWebProcess WebKitNetworkProcess WebKitGPUProcess gnome-shell Xwayland Xorg"

# i processi con quel nome (il kernel tiene solo i primi 15 caratteri: WebKitWebProces)
pid_di() { pgrep -x "${1:0:15}"; }

# secondi di CPU (utente + sistema) di tutti i processi con quel nome
cpu_di() {
  local tot=0 p t
  for p in $(pid_di "$1"); do
    t=$(awk '{print $14 + $15}' "/proc/$p/stat" 2>/dev/null || echo 0)
    tot=$((tot + t))
  done
  echo "$tot"
}

misura() {
  local nome
  declare -A prima
  for nome in $PROCESSI; do prima[$nome]=$(cpu_di "$nome"); done
  sleep "$DURATA"
  for nome in $PROCESSI; do
    pid_di "$nome" >/dev/null || continue
    awk -v a="${prima[$nome]}" -v b="$(cpu_di "$nome")" -v hz="$HZ" -v d="$DURATA" -v n="$NCPU" -v nome="$nome" \
      'BEGIN { c = (b - a) / hz / d; printf "  %-22s %5.2f core  (%4.1f%% di tutta la CPU)\n", nome, c, c * 100 / n }'
    [ -n "$SCENARIO" ] && awk -v a="${prima[$nome]}" -v b="$(cpu_di "$nome")" -v hz="$HZ" -v d="$DURATA" \
      -v s="$SCENARIO" -v nome="$nome" 'BEGIN { printf "%s %s %.4f\n", s, nome, (b - a) / hz / d }' >> "$MISURE"
  done
}

chiudi_app() {
  pkill -x ascii-zoo 2>/dev/null
  for _ in 1 2 3 4 5 6 7 8 9 10; do pgrep -x ascii-zoo >/dev/null || return 0; sleep 0.5; done
  pkill -9 -x ascii-zoo 2>/dev/null
  sleep 1
}

# avvia l'app con le variabili d'ambiente date (env VAR=valore …), con il log acceso
prova() {
  local titolo="$1"; shift
  echo "== $titolo" | tee -a "$RAPPORTO"
  echo "   (in corso: $((ATTESA + DURATA)) secondi)" >&2
  chiudi_app
  : > "$LOG"
  env "$@" ASCII_ZOO_LOG=1 setsid "$BIN" >/dev/null 2>"$LOG" &
  sleep "$ATTESA"
  if ! pgrep -x ascii-zoo >/dev/null; then
    echo "  l'app non è partita; log:" >> "$RAPPORTO"
    tail -5 "$LOG" | sed 's/^/    /' >> "$RAPPORTO"
    return
  fi
  misura | tee -a "$RAPPORTO"
  # le righe del log che dicono in che modo (Wayland o X11) e se lo sfondo era in pausa
  grep -E "sfondo: (Wayland|X11|l'estensione)|pausa|riparte|schermo .*×" "$LOG" | tail -8 | sed 's/^/    log: /' >> "$RAPPORTO"
  # quanti fotogrammi ha fatto davvero (dalla versione dopo la 0.6.0, ogni 10 secondi): se
  # sono meno di quelli scelti nel menu, qualcosa non ce la fa
  grep -E "fotogrammi al secondo|ridisegnata" "$LOG" | tail -2 | sed 's/^/    log: /' >> "$RAPPORTO"
  # statistiche per nerd (con la GPU): ci sono dalla versione dopo la 0.5.0. Una versione
  # che non conosce --statistiche partirebbe e basta: la si riconosce dal nome della
  # libreria NVIDIA che legge la GPU, che c'è nel programma solo da quella versione
  if grep -qa "libnvidia-ml.so.1" "$BIN"; then
    "$BIN" --statistiche 2>/dev/null | tr -d ' \n' | grep -oE '"(gpu|gputemp|vram)":(\[[^]]*\]|[0-9.]+)' | sed 's/^/    /' >> "$RAPPORTO"
  fi
}

: > "$RAPPORTO"
{
  echo "ASCII Zoo, prestazioni — $(date '+%Y-%m-%d %H:%M')"
  echo "programma: $BIN ($("$BIN" --version 2>/dev/null))"
  # compilato da un clone: il commit
  c=$(git -C "$(dirname "$BIN")" rev-parse --short HEAD 2>/dev/null) && echo "commit: $c"
  echo "sistema: $(. /etc/os-release 2>/dev/null; echo "${PRETTY_NAME:-?}"), kernel $(uname -r)"
  echo "sessione: ${XDG_SESSION_TYPE:-?}, desktop ${XDG_CURRENT_DESKTOP:-?}, $(gnome-shell --version 2>/dev/null || echo 'GNOME Shell ?')"
  echo "CPU: $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ //'), $NCPU thread"
  echo "RAM: $(awk '/MemTotal/ {printf "%.1f GB", $2 / 1048576}' /proc/meminfo)"
  echo "scheda video:"
  if command -v lspci >/dev/null; then
    lspci -k 2>/dev/null | grep -A3 -Ei 'vga|3d controller|display controller' | grep -Ei 'vga|3d|display|in use' | sed 's/^/  /'
  fi
  for c in /sys/class/drm/card[0-9]; do
    [ -e "$c/device/driver" ] && echo "  $(basename "$c"): driver $(basename "$(readlink "$c/device/driver")")"
  done
  command -v nvidia-smi >/dev/null && echo "  nvidia: $(nvidia-smi --query-gpu=name,driver_version --format=csv,noheader 2>/dev/null)"
  command -v glxinfo >/dev/null && glxinfo -B 2>/dev/null | grep -E 'OpenGL renderer|OpenGL version' | sed 's/^/  /'
  echo "WebKitGTK: $(dpkg-query -W -f='${Version}' libwebkit2gtk-4.1-0 2>/dev/null || rpm -q webkit2gtk4.1 2>/dev/null || pacman -Q webkit2gtk-4.1 2>/dev/null || echo '?')"
  echo "variabili: $(env | grep -E '^(WEBKIT|GDK|GSK|LIBGL|__GL)' | tr '\n' ' ')"
  echo "schermi collegati:"
  for s in /sys/class/drm/card*-*; do
    [ "$(cat "$s/status" 2>/dev/null)" = connected ] && echo "  ${s##*/}: $(head -1 "$s/modes" 2>/dev/null)"
  done
  echo "  scala GNOME: $(gsettings get org.gnome.desktop.interface scaling-factor 2>/dev/null), testo $(gsettings get org.gnome.desktop.interface text-scaling-factor 2>/dev/null), funzioni sperimentali $(gsettings get org.gnome.mutter experimental-features 2>/dev/null)"
  # l'estensione di GNOME (sfondo senza XWayland e clic): installata, accesa, quella nuova?
  e="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/zoo-click@tressilloide.github.io"
  if [ -d "$e" ]; then
    stato=$(gnome-extensions info zoo-click@tressilloide.github.io 2>/dev/null | grep -iE "state|stato" | sed 's/^ *//')
    echo "estensione di GNOME: installata$( [ -f "$e/sfondo.js" ] && echo ", con lo sfondo senza XWayland" ), ${stato:-stato ?}"
  else
    echo "estensione di GNOME: non installata"
  fi
  echo "impostazioni dell'app (solo quelle che contano per le prestazioni):"
  conf="${XDG_CONFIG_HOME:-$HOME/.config}/AsciiZoo/config.json"
  if [ -f "$conf" ] && command -v python3 >/dev/null; then
    python3 -c '
import json, sys
c = json.load(open(sys.argv[1]))
chiavi = ["animale", "animale_schermi", "tema", "orologio", "clic_desktop", "pausa_batteria",
          "pausa_schermo_intero", "pausa_coperto", "widget_statistiche", "widget_nerd", "widget_musica",
          "widget_sfondo_pieno", "widget_schermo", "salvaschermo_minuti", "fotogrammi"]
for k in chiavi:
    if k in c: print("  %s: %s" % (k, json.dumps(c[k])))
' "$conf"
  else
    echo "  (non trovate)"
  fi
  echo
} >> "$RAPPORTO"

echo "Misuro per circa 3 minuti e mezzo: lascia il desktop visibile e non usare il PC." >&2
c_era=$(pgrep -x ascii-zoo >/dev/null && echo si || echo no)

chiudi_app
echo "== app chiusa" | tee -a "$RAPPORTO"
sleep 3
misura | tee -a "$RAPPORTO"

SCENARIO=30
prova "app normale (a 30 fotogrammi al secondo)" ASCII_ZOO_FOTOGRAMMI=30
SCENARIO=""
# senza XWayland? allora anche su XWayland, per confrontare
if grep -q "sfondo: Wayland" "$LOG"; then
  prova "la stessa app su XWayland (ASCII_ZOO_SFONDO=x11), per confronto" ASCII_ZOO_SFONDO=x11
fi
prova "WebKitGTK tutto in software" WEBKIT_DISABLE_COMPOSITING_MODE=1
SCENARIO=15
prova "app normale a 15 fotogrammi al secondo" ASCII_ZOO_FOTOGRAMMI=15
SCENARIO=5
prova "app normale a 5 fotogrammi al secondo" ASCII_ZOO_FOTOGRAMMI=5
SCENARIO=""
# fisso e per fotogramma: la retta che passa meglio per i tre punti (30, 15, 5)
{
  echo "== costo fisso e per fotogramma (dalle misure a 30, 15 e 5 fotogrammi al secondo)"
  awk '
    { f[$2] = f[$2] " " $1 ":" $3; nomi[$2] = 1 }
    END {
      for (n in nomi) {
        k = split(f[n], p, " "); sx = sy = sxx = sxy = m = 0
        for (i = 1; i <= k; i++) { split(p[i], q, ":"); x = q[1] + 0; y = q[2] + 0; sx += x; sy += y; sxx += x * x; sxy += x * y; m++ }
        if (m < 2 || m * sxx - sx * sx == 0) continue
        b = (m * sxy - sx * sy) / (m * sxx - sx * sx); a = (sy - b * sx) / m
        printf "  %-22s fisso %5.2f core, ogni fotogramma al secondo in più %.4f core (a 30: %.2f)\n", n, a, b, a + 30 * b
      }
    }' "$MISURE" | sort
} | tee -a "$RAPPORTO"

chiudi_app
if [ "$c_era" = si ]; then
  setsid "$BIN" >/dev/null 2>&1 &
  echo "ASCII Zoo è ripartito come prima." >&2
fi
echo >&2
echo "Fatto. Il rapporto è in $RAPPORTO: mandamelo così com'è." >&2
