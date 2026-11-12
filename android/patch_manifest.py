#!/usr/bin/env python3
"""Aggiunge al manifest generato da cargo-apk il receiver del promemoria giornaliero e
gli intent-filter per aprire il pacchetto contenuti con «Apri con Tremendous Preparator»
(cargo-apk non sa dichiararli). Uso: patch_manifest.py <in.xml> <out.xml>"""
import re
import sys

RECEIVER = (
    '<receiver android:name="it.tremendouspreparator.app.ReminderReceiver" android:exported="true">'
    '<intent-filter>'
    '<action android:name="android.intent.action.BOOT_COMPLETED"/>'
    '<action android:name="android.intent.action.MY_PACKAGE_REPLACED"/>'
    '</intent-filter></receiver>'
)

# pacchetto contenuti (SemestreAperto-contenuti.json.gz) da Download, browser o condivisione
MIMES = "".join(
    f'<data android:mimeType="{m}"/>'
    for m in ("application/gzip", "application/x-gzip", "application/octet-stream", "application/json")
)
PACK_FILTERS = (
    '<intent-filter><action android:name="android.intent.action.VIEW"/>'
    '<category android:name="android.intent.category.DEFAULT"/>'
    '<data android:scheme="content"/><data android:scheme="file"/>' + MIMES + "</intent-filter>"
    '<intent-filter><action android:name="android.intent.action.SEND"/>'
    '<category android:name="android.intent.category.DEFAULT"/>' + MIMES + "</intent-filter>"
)

src, dst = sys.argv[1:3]
xml = open(src, encoding="utf-8").read()
for need in ('android:hasCode="true"', 'it.tremendouspreparator.app.MainActivity', "POST_NOTIFICATIONS"):
    if need not in xml:
        sys.exit(f"manifest inatteso: manca {need}")
if "ReminderReceiver" not in xml:
    assert xml.count("</application>") == 1
    xml = xml.replace("</application>", RECEIVER + "</application>")
if "android.intent.action.SEND" not in xml:
    m = re.search(r'<activity\b[^>]*it\.tremendouspreparator\.app\.MainActivity[^>]*>', xml)
    assert m, "activity principale non trovata"
    tag = m.group(0)
    # una sola istanza: un file aperto da un'altra app arriva a quella già aperta (onNewIntent)
    if "launchMode" not in tag:
        tag2 = tag[:-1].rstrip("/") + ' android:launchMode="singleTask">'
        xml = xml.replace(m.group(0), tag2, 1)
    end = xml.index("</activity>", xml.index("it.tremendouspreparator.app.MainActivity"))
    xml = xml[:end] + PACK_FILTERS + xml[end:]
open(dst, "w", encoding="utf-8").write(xml)
print(xml)
