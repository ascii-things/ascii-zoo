# Firmare ASCII Zoo

Senza firma, al primo avvio Windows mostra **"Windows ha protetto il PC"** e macOS **"Impossibile aprire l'app"**.
La firma toglie questi avvisi, ma richiede account a nome di chi pubblica, quindi la CI non può farla da sola.
È però già tutto pronto: aggiunti i segreti qui sotto, si accende da sola.

## Windows: SignPath Foundation (gratis per i progetti open source)

**Cosa è pronto.**
- Il passo "Firma con SignPath" in [`.github/workflows/build.yml`](../.github/workflows/build.yml) è stato preparato da Daniel il 18/11.
- La sezione "Code signing policy" del README è quella che SignPath Foundation chiede.
- Il passo parte solo con un tag e solo se c'è la variabile `SIGNPATH_ORGANIZATION_ID`.

**Cosa fare.**

1. **Fai domanda** su <https://signpath.org/apply>. I requisiti sono questi, e ci sono già tutti:
   - una licenza approvata da OSI (MIT);
   - il repository pubblico;
   - gli eseguibili costruiti dalla CI;
   - la "Code signing policy" nel README;
   - l'autenticazione a due fattori su GitHub per chi ha accesso in scrittura.
2. **Quando approvano, configura su SignPath:**
   - un progetto con slug **`ascii-zoo`**;
   - una signing policy con slug **`release-signing`**. Devono essere uguali a quelli di `build.yml`;
   - come "artifact configuration", lo zip che la CI carica (l'artefatto `ascii-zoo-windows`, quello del passo `non-firmato`). Dentro ci sono `ascii-zoo.exe`, da firmare con Authenticode, e il suo `.sha256`.
3. **Su GitHub**, in *Settings → Secrets and variables → Actions*:
   - **variabile** `SIGNPATH_ORGANIZATION_ID`: l'ID dell'organizzazione su SignPath;
   - **segreto** `SIGNPATH_API_TOKEN`: il token di un utente CI di SignPath con il permesso di inviare richieste di firma.
4. **A ogni Release** (tag `v…`), il job di Windows aspetta fino a un'ora che qualcuno approvi la richiesta su SignPath. Gli approvatori sono quelli scritti nel README, cioè tu e Daniel.

**Cosa cambia per chi scarica.**
- L'exe è firmato da "SignPath Foundation".
- SmartScreen smette di avvisare man mano che il certificato si fa una reputazione: non per forza dal primo giorno.
- A quel punto vanno corretti i README, dove dicono che l'exe non è firmato.

## macOS: Apple Developer Program (99 $ all'anno)

**Cosa è pronto** (dal 27/11, in `build.yml`). Le due parti si accendono da sole con i segreti, e senza segreti tutto resta come prima, con la firma "ad hoc".
- **La firma.** Nel passo "App universale e DMG", con i segreti `MACOS_CERT_P12` e `MACOS_CERT_PASSWORD`:
  - l'app e lo screensaver si firmano con il certificato "Developer ID Application";
  - la firma usa il runtime protetto (hardened runtime) e l'ora certificata;
  - le eccezioni al runtime protetto sono in [`app/macos/entitlements.plist`](../app/macos/entitlements.plist). C'è solo quella per mandare Apple Events a Musica e Spotify, che serve al widget "In riproduzione".
- **La notarizzazione.** Il passo "Notarizzazione del DMG", solo con un tag e con i segreti della chiave API:
  - firma il DMG e lo manda ad Apple;
  - aspetta la risposta e attacca il "biglietto" al DMG (staple);
  - rifà l'SHA256.
  - Se Apple dice di no, la Release esce lo stesso, con il DMG firmato ma non notarizzato.

**Cosa fare.**

1. **Iscriviti** su <https://developer.apple.com/programs/>, come persona singola. Serve un Apple ID con l'autenticazione a due fattori.
2. **Crea il certificato "Developer ID Application".**
   - Su un Mac: Xcode → *Settings → Accounts* → il tuo team → *Manage Certificates* → **+** → *Developer ID Application*.
   - Poi, dal Portachiavi, esportalo (certificato più chiave privata) come `.p12`, con una password.
3. **Crea la chiave per la notarizzazione.**
   - Su App Store Connect: *Users and Access → Integrations → App Store Connect API* → **+**, con il ruolo *Developer*.
   - Scarica il file `AuthKey_XXXXXXXXXX.p8`. Si scarica una volta sola.
   - Segnati il *Key ID* e l'*Issuer ID*.
4. **Su GitHub**, in *Settings → Secrets and variables → Actions*, aggiungi questi segreti:

   | Segreto | Cosa c'è dentro |
   |---|---|
   | `MACOS_CERT_P12` | il `.p12` in base64: `base64 -i certificato.p12 \| pbcopy` |
   | `MACOS_CERT_PASSWORD` | la password del `.p12` |
   | `MACOS_NOTARY_KEY` | il `.p8` in base64: `base64 -i AuthKey_XXXXXXXXXX.p8 \| pbcopy` |
   | `MACOS_NOTARY_KEY_ID` | il Key ID |
   | `MACOS_NOTARY_ISSUER` | l'Issuer ID |

5. **Prova la firma senza fare una Release.**
   - Basta un push qualsiasi: nel log del passo "App universale e DMG" deve comparire `firma con il certificato Developer ID`.
   - Le prove dello screensaver e dell'app che vengono dopo girano già sull'app firmata.
6. **Al primo tag guarda il passo "Notarizzazione del DMG".** Se Apple rifiuta, il log mostra l'id della richiesta, e `xcrun notarytool log <id> --key … --key-id … --issuer …` dice perché.

**Non provato.** Questi passi non sono mai girati, perché servono i segreti. In particolare:
- il runtime protetto con WKWebView e con lo screensaver dentro l'app;
- l'installazione dello screensaver firmato in `~/Library/Screen Savers`.

Se qualcosa non va, il primo push con i segreti lo fa vedere nelle prove della CI, prima di una Release.

**Cosa cambia per chi scarica.** Il DMG si apre con un doppio clic, senza passare da *Privacy e sicurezza → Apri comunque*. Poi vanno corretti i README.

## Linux

Non c'è niente da firmare:
- `linux/install.sh` controlla l'SHA256 di quello che scarica;
- ogni Release ha l'attestazione di provenienza di GitHub (`gh attestation verify`).
