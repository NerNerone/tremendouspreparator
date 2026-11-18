# Rilasci

## Versione

La versione sta solo nel `Cargo.toml` della radice, con il versionCode Android nel commento:

```toml
version = "0.2.0" # versionCode 16777728
```

Il versionCode è quello che calcola cargo-apk (`1 << 24 | major << 16 | minor << 8 | patch`, ogni parte
< 256): `python3 tools/version_code.py` lo stampa, `--check` (eseguito dalla CI) verifica il commento.

## Novità in app

Ogni cambiamento visibile si aggiunge come punto (`- …`) in
[`crates/prep-app/assets/novita.md`](../crates/prep-app/assets/novita.md), sotto la data del giorno
(sezioni `## <data>`, la più recente in alto). Dopo l'aggiornamento l'app mostra all'avvio i punti
non ancora visti (riconosciuti dal testo: un punto riscritto ricompare); da Opzioni › Novità si
rivedono tutti.

## Pubblicare una versione

1. Aggiorna `version` e il commento in `Cargo.toml`, poi `cargo check` (aggiorna `Cargo.lock`).
2. Scrivi le note in `fastlane/metadata/android/{it-IT,en-US}/changelogs/<versionCode>.txt`
   (al massimo 500 caratteri).
3. Aggiungi la versione in `flatpak/io.github.nernerone.TremendousPreparator.metainfo.xml` (`<releases>`).
4. Commit, poi tag e push:

```bash
git tag v0.2.0
```

```bash
git push origin main v0.2.0
```

Il workflow [android.yml](../.github/workflows/android.yml) costruisce gli APK (universale e uno per
architettura, con `SPLIT=1 tools/build_apk.sh`) e il Flatpak e crea la release
`v0.2.0` con le note italiane. A ogni push su `main` aggiorna invece la pre-release `beta`, firmata con la stessa chiave.

## Firma

Una sola chiave firma tutti gli APK distribuiti da noi (release, anteprime e la build privata). Android
installa un aggiornamento solo se è firmato con la stessa chiave dell'app installata: la chiave **non
si cambia** e **non si perde** (senza, nessun aggiornamento è più possibile: bisogna disinstallare).

- Si crea una volta, sul proprio computer (serve `keytool`, del JDK):

  ```bash
  keytool -genkeypair -v -storetype PKCS12 -keystore tremendous-release.p12 -alias tremendous -keyalg RSA -keysize 4096 -validity 18250 -dname "CN=Tremendous Preparator"
  ```

  La password richiesta vale per archivio e chiave (gli script si aspettano la stessa).
- Va nei secret di ogni repository che costruisce APK (Settings › Secrets and variables › Actions):
  `ANDROID_KEYSTORE_B64` = il file in base64 (`base64 -w0 tremendous-release.p12`) e
  `ANDROID_KEYSTORE_PASSWORD` = la password. Mai nel repository.
- Copie di riserva: il file `.p12` e la password in un gestore di password e su un supporto offline.
- Impronta SHA-256 del certificato: la CI la stampa e la scrive nelle note di ogni release
  (`apksigner verify --print-certs`); serve a verificare che un APK sia autentico.
- Senza la chiave nei secret la build fallisce (release e beta): ogni APK distribuito aggiorna
  quello installato.
- Se la chiave fosse compromessa: lo schema di firma v3 permette di passare a una chiave nuova
  firmata dalla vecchia (Android 9+); su Android 8 servirebbe reinstallare.

## F-Droid (bozza, non inviata)

Controllato sulla [Inclusion Policy](https://f-droid.org/docs/Inclusion_Policy/), sulle
[Anti-Features](https://f-droid.org/docs/Anti-Features/) e sulla
[guida all'invio](https://f-droid.org/docs/Submitting_to_F-Droid_Quick_Start_Guide/):

| Requisito | Stato |
| --- | --- |
| Licenza libera, sorgente pubblico | GPL-3.0-or-later; font (OFL, licenza Bitstream/DejaVu) in `crates/prep-app/fonts/LICENSES.txt`, icone Material (Apache-2.0) in `crates/prep-app/ui/icons/LICENSE.txt`, dati della tavola periodica dal pacchetto `mendeleev` (MIT; valori IUPAC/NIST) |
| Solo dipendenze libere, niente Google Play Services/Firebase/analitiche | crate Rust controllati con `cargo metadata` (target Android, 17/11/2026): MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, GPL-3.0 (Slint), CDLA-Permissive-2.0 (certificati di `webpki-roots`); nessuna libreria Java esterna |
| Strumenti di build liberi | Rust (rustup), cargo-apk, JDK libero, Android SDK/NDK (ammessi) |
| applicationId legato a un dominio del progetto | `io.github.nernerone.tremendouspreparator` (GitHub Pages del progetto) |
| Tag per ogni versione | `vX.Y.Z`, uguale alla versione in `Cargo.toml` |
| Metadati fastlane, changelog per versionCode | `fastlane/metadata/android/` (it-IT, en-US; icona e schermate in `en-US/images/`) |
| versionCode leggibile senza eseguire codice | commento `# versionCode N` in `Cargo.toml`, controllato in CI |
| Niente contenuti di terzi | l'app non include materiale di studio; l'esempio è scritto per il progetto |
| Sviluppo attivo da almeno 3 mesi | **non ancora**: il repository pubblico è di novembre 2026, quindi l'invio ha senso da febbraio 2027 (o prima, se l'app ha già utenti) |

Anti-feature: nessuna pubblicità né tracciamento; la rete si usa solo su richiesta (collegare i propri
dispositivi, condividere un esame, entrare in un gruppo). I relay Nostr sono un protocollo aperto e
modificabile dalle Opzioni, quindi non è «Tethered Network Services». I server STUN predefiniti (Google,
Cloudflare) sono servizi proprietari ma non indispensabili (il collegamento ripiega sui relay): da
segnalare ai revisori; se lo chiedessero, si rendono configurabili o si sostituiscono con server liberi.

Firma (scelta: firma di F-Droid): F-Droid firma con la propria chiave, quindi l'APK di F-Droid e quelli
delle release GitHub sono due canali separati che non si aggiornano a vicenda (per passare dall'uno
all'altro si disinstalla). Passare in seguito a una firma unica richiederebbe build riproducibili
(`Binaries:` e `AllowedAPKSigningKeys:`, toolchain fissate e verifica con `fdroid build`) e farebbe
reinstallare gli utenti F-Droid.

Ricetta per [fdroiddata](https://gitlab.com/fdroid/fdroiddata), `metadata/io.github.nernerone.tremendouspreparator.yml`
(mai eseguita: da provare con `fdroid lint` e `fdroid build` nel container di fdroidserver prima di aprire la merge
request; `cmake` serve a `aws-lc-sys`, dipendenza di str0m per WebRTC):

```yaml
Categories:
  - Science & Education
License: GPL-3.0-or-later
SourceCode: https://github.com/NerNerone/tremendouspreparator
IssueTracker: https://github.com/NerNerone/tremendouspreparator/issues

AutoName: Tremendous Preparator

RepoType: git
Repo: https://github.com/NerNerone/tremendouspreparator.git

Builds:
  - versionName: 0.2.0
    versionCode: 16777728
    commit: v0.2.0
    sudo:
      - apt-get update
      - apt-get install -y openjdk-17-jdk-headless python3 cmake
    output: TremendousPreparator.apk
    srclibs:
      - rustup@<tag>
    build:
      - bash $$rustup$$/rustup-init.sh -y --profile minimal --default-toolchain <versione>
      - source $HOME/.cargo/env
      - rustup target add aarch64-linux-android armv7-linux-androideabi
      - cargo install cargo-apk --version 0.10.0 --locked
      - ANDROID_HOME=$$SDK$$ ANDROID_NDK_ROOT=$$NDK$$ tools/build_apk.sh TremendousPreparator.apk
    ndk: <versione>

AutoUpdateMode: Version
UpdateCheckMode: Tags ^v\d+\.\d+\.\d+$
UpdateCheckData: Cargo.toml|versionCode (\d+)|.|^version = "([^"]+)"
CurrentVersion: 0.2.0
CurrentVersionCode: 16777728
```

Prima dell'invio: provare sul telefono la checklist della modalità concentrazione (chiamate, numeri
d'emergenza, sveglia, riavvio) e pubblicare la release `v0.2.0`.
