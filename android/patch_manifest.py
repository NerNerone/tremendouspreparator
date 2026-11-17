#!/usr/bin/env python3
"""Aggiunge al manifest generato da cargo-apk il receiver del promemoria giornaliero e
gli intent-filter per aprire il pacchetto contenuti con «Apri con Tremendous Preparator»
(cargo-apk non sa dichiararli). Uso: patch_manifest.py <in.xml> <out.xml>"""
import re
import sys

RECEIVER = (
    '<receiver android:name="io.github.nernerone.tremendouspreparator.ReminderReceiver" android:exported="true">'
    '<intent-filter>'
    '<action android:name="android.intent.action.BOOT_COMPLETED"/>'
    '<action android:name="android.intent.action.MY_PACKAGE_REPLACED"/>'
    '</intent-filter></receiver>'
)

# pacchetto contenuti (.json.gz) da Download, browser o condivisione
MIMES = "".join(
    f'<data android:mimeType="{m}"/>'
    for m in ("application/gzip", "application/x-gzip", "application/octet-stream", "application/json")
)
# inviti ai gruppi di studio: link alla web app (si aprono anche nell'app, se l'utente lo sceglie)
INVITE_FILTER = (
    '<intent-filter><action android:name="android.intent.action.VIEW"/>'
    '<category android:name="android.intent.category.DEFAULT"/>'
    '<category android:name="android.intent.category.BROWSABLE"/>'
    '<data android:scheme="https" android:host="nernerone.github.io" android:pathPrefix="/tremendouspreparator"/>'
    "</intent-filter>"
)

PACK_FILTERS = (
    '<intent-filter><action android:name="android.intent.action.VIEW"/>'
    '<category android:name="android.intent.category.DEFAULT"/>'
    '<data android:scheme="content"/><data android:scheme="file"/>' + MIMES + "</intent-filter>"
    '<intent-filter><action android:name="android.intent.action.SEND"/>'
    '<category android:name="android.intent.category.DEFAULT"/>' + MIMES + "</intent-filter>"
)

# Bluetooth tra telefoni vicini (Android 12+): non serve la posizione
BT_PERMS = (
    # fino ad Android 11: permessi Bluetooth classici e posizione per cercare i dispositivi
    '<uses-permission android:name="android.permission.BLUETOOTH" android:maxSdkVersion="30"/>'
    '<uses-permission android:name="android.permission.BLUETOOTH_ADMIN" android:maxSdkVersion="30"/>'
    '<uses-permission android:name="android.permission.ACCESS_FINE_LOCATION" android:maxSdkVersion="30"/>'
    '<uses-permission android:name="android.permission.BLUETOOTH_CONNECT"/>'
    '<uses-permission android:name="android.permission.BLUETOOTH_ADVERTISE"/>'
    '<uses-permission android:name="android.permission.BLUETOOTH_SCAN" android:usesPermissionFlags="neverForLocation"/>'
)

# modalità concentrazione: servizio in primo piano che guarda l'app in primo piano (accesso
# all'utilizzo) e riporta all'app con un avviso sovrapposto (sovrapposizione ad altre app)
FOCUS_PERMS = (
    '<uses-permission android:name="android.permission.PACKAGE_USAGE_STATS"/>'
    '<uses-permission android:name="android.permission.SYSTEM_ALERT_WINDOW"/>'
    '<uses-permission android:name="android.permission.FOREGROUND_SERVICE"/>'
    '<uses-permission android:name="android.permission.FOREGROUND_SERVICE_SPECIAL_USE"/>'
)
# app avviabili (per scegliere quelle permesse) e app del telefono (chiamate sempre possibili)
QUERIES = (
    "<queries>"
    '<intent><action android:name="android.intent.action.MAIN"/>'
    '<category android:name="android.intent.category.LAUNCHER"/></intent>'
    '<intent><action android:name="android.intent.action.DIAL"/></intent>'
    "</queries>"
)
FOCUS_SERVICE = (
    '<service android:name="io.github.nernerone.tremendouspreparator.FocusService" android:exported="false" android:process=":focus" '
    'android:foregroundServiceType="specialUse">'
    '<property android:name="android.app.PROPERTY_SPECIAL_USE_FGS_SUBTYPE" '
    'android:value="Blocco delle app non permesse durante una sessione di studio scelta dall\'utente"/>'
    "</service>"
)

src, dst = sys.argv[1:3]
xml = open(src, encoding="utf-8").read()
for need in ('android:hasCode="true"', 'io.github.nernerone.tremendouspreparator.MainActivity', "POST_NOTIFICATIONS"):
    if need not in xml:
        sys.exit(f"manifest inatteso: manca {need}")
if "BLUETOOTH_CONNECT" not in xml:
    assert xml.count("<application") == 1
    xml = xml.replace("<application", BT_PERMS + "<application", 1)
if "PACKAGE_USAGE_STATS" not in xml:
    xml = xml.replace("<application", FOCUS_PERMS + QUERIES + "<application", 1)
if "FocusService" not in xml:
    xml = xml.replace("</application>", FOCUS_SERVICE + "</application>", 1)
if "ReminderReceiver" not in xml:
    assert xml.count("</application>") == 1
    xml = xml.replace("</application>", RECEIVER + "</application>")
if "android.intent.action.SEND" not in xml:
    m = re.search(r'<activity\b[^>]*io\.github\.nernerone\.tremendouspreparator\.MainActivity[^>]*>', xml)
    assert m, "activity principale non trovata"
    tag = m.group(0)
    # una sola istanza: un file aperto da un'altra app arriva a quella già aperta (onNewIntent)
    if "launchMode" not in tag:
        tag2 = tag[:-1].rstrip("/") + ' android:launchMode="singleTask">'
        xml = xml.replace(m.group(0), tag2, 1)
    end = xml.index("</activity>", xml.index("io.github.nernerone.tremendouspreparator.MainActivity"))
    xml = xml[:end] + PACK_FILTERS + INVITE_FILTER + xml[end:]
for need in ("<queries>", 'foregroundServiceType="specialUse"', "PROPERTY_SPECIAL_USE_FGS_SUBTYPE", "SYSTEM_ALERT_WINDOW"):
    assert need in xml, f"manifest: manca {need}"
assert xml.index("<queries>") < xml.index("<application"), "<queries> va prima di <application>"
open(dst, "w", encoding="utf-8").write(xml)
print(xml)
