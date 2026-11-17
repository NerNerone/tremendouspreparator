# Rilasci

## Versione

La versione sta solo nel `Cargo.toml` della radice, con il versionCode Android nel commento:

```toml
version = "0.2.0" # versionCode 16777728
```

Il versionCode è quello che calcola cargo-apk (`1 << 24 | major << 16 | minor << 8 | patch`, ogni parte
< 256): `python3 tools/version_code.py` lo stampa, `--check` (eseguito dalla CI) verifica il commento.

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

Il workflow [android.yml](../.github/workflows/android.yml) costruisce APK e Flatpak e crea la release
`v0.2.0` con le note italiane. A ogni push su `main` aggiorna invece la pre-release `anteprima`.

L'APK è firmato con la chiave nei secret `ANDROID_KEYSTORE_B64` (keystore in base64) e
`ANDROID_KEYSTORE_PASSWORD` (stessa password per keystore e chiave); senza, con una chiave temporanea
(installabile, ma non aggiorna le build precedenti).

## F-Droid (bozza, non inviata)

Controllato sulla [Inclusion Policy](https://f-droid.org/docs/Inclusion_Policy/), sulle
[Anti-Features](https://f-droid.org/docs/Anti-Features/) e sulla
[guida all'invio](https://f-droid.org/docs/Submitting_to_F-Droid_Quick_Start_Guide/):

| Requisito | Stato |
| --- | --- |
| Licenza libera, sorgente pubblico | GPL-3.0-or-later; font con licenze libere (`crates/prep-app/fonts/LICENSES.txt`) |
| Solo dipendenze libere, niente Google Play Services/Firebase/analitiche | sì (crate Rust da crates.io, nessuna libreria Java esterna) |
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

Firma: F-Droid firma con la propria chiave, quindi l'APK di F-Droid e quello delle release GitHub non si
aggiornano a vicenda. Una firma unica richiede build riproducibili (`Binaries:` e
`AllowedAPKSigningKeys:`): le build da tag non contengono più dati della CI, ma servono ancora
toolchain fissate (Rust, NDK) e una verifica con `fdroid build`. Per il primo invio va bene la firma di
F-Droid.

Ricetta per [fdroiddata](https://gitlab.com/fdroid/fdroiddata), `metadata/io.github.nernerone.tremendouspreparator.yml`
(da provare con `fdroid lint` e `fdroid build` nel container di fdroidserver prima di aprire la merge request):

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
      - apt-get install -y openjdk-17-jdk-headless python3
    output: TremendousPreparator.apk
    srclibs:
      - rustup@<tag>
    build:
      - bash $$rustup$$/rustup-init.sh -y --profile minimal --default-toolchain <versione>
      - source $HOME/.cargo/env
      - rustup target add aarch64-linux-android
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
