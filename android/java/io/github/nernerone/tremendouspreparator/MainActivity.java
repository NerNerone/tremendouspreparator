package io.github.nernerone.tremendouspreparator;

import android.Manifest;
import android.app.NativeActivity;
import android.app.NotificationManager;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Bundle;
import java.nio.charset.StandardCharsets;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * NativeActivity (UI in Rust/Slint) con in più il promemoria giornaliero:
 * chiede il permesso per le notifiche e riprogramma l'allarme ogni volta che
 * l'app va in secondo piano (così l'orario scelto in Impostazioni è sempre aggiornato).
 *
 * Pacchetto contenuti: il file scelto con il selettore (pickContentPack, chiamato da
 * Rust via JNI) o aperto con «Apri con Tremendous Preparator» viene copiato in
 * files/import/; la parte Rust lo trova lì, lo importa e lo cancella.
 */
public class MainActivity extends NativeActivity {
    private static final int PICK_PACK = 4242;
    private static final int PICK_PHOTO_USER = 4243;
    private static final int PICK_PHOTO_GROUP = 4244;
    private BtLink bt;

    private BtLink bt() {
        if (bt == null) bt = new BtLink(this);
        return bt;
    }

    // Bluetooth tra telefoni vicini (chiamati da Rust via JNI, vedi BtLink)
    public void btListen() {
        bt().listen();
    }

    public void btScan() {
        bt().scan();
    }

    public void btConnect(String address) {
        bt().connect(address);
    }

    public void btSend(byte[] data) {
        bt().send(data);
    }

    public void btClose() {
        bt().close();
    }

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (android.os.Build.VERSION.SDK_INT >= 33
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[] {Manifest.permission.POST_NOTIFICATIONS}, 1);
        }
        Reminders.schedule(this);
        if (state == null) handleIntent(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        handleIntent(intent);
    }

    @Override
    protected void onResume() {
        super.onResume();
        // il servizio della concentrazione riparte se il sistema l'aveva fermato
        FocusService.ensure(this);
        // aperta l'app, il promemoria di oggi non serve più
        NotificationManager nm = getSystemService(NotificationManager.class);
        if (nm != null) nm.cancel(Reminders.NOTIFICATION_ID);
    }

    @Override
    protected void onPause() {
        Reminders.schedule(this);
        Reminders.showGoals(this);
        super.onPause();
    }

    /** Apre il selettore di file di sistema (chiamato dal thread Rust). */
    public void pickContentPack() {
        runOnUiThread(() -> {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType("*/*");
            i.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {"application/gzip", "application/x-gzip", "application/octet-stream", "application/json"});
            try {
                startActivityForResult(i, PICK_PACK);
            } catch (Exception e) {
                // nessun selettore disponibile: resta il metodo «Apri con»
            }
        });
    }

    /** Apre il selettore di immagini: foto profilo ("user") o del gruppo ("group"). */
    public void pickPhoto(String kind) {
        runOnUiThread(() -> {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType("image/*");
            try {
                startActivityForResult(i, "group".equals(kind) ? PICK_PHOTO_GROUP : PICK_PHOTO_USER);
            } catch (Exception e) {
                // nessun selettore disponibile
            }
        });
    }

    /** Notifica immediata (avvisi del gruppo di studio). */
    public void notifyNow(String text) {
        String[] parts = text.split("\n", 2);
        Reminders.notifyNow(this, parts[0], parts.length > 1 ? parts[1] : "");
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (resultCode != RESULT_OK || data == null || data.getData() == null) return;
        if (requestCode == PICK_PACK) {
            receive(data.getData());
        } else if (requestCode == PICK_PHOTO_USER) {
            receive(data.getData(), "user", ".photo");
        } else if (requestCode == PICK_PHOTO_GROUP) {
            receive(data.getData(), "group", ".photo");
        }
    }

    private void handleIntent(Intent intent) {
        if (intent == null) return;
        Uri uri = null;
        if (Intent.ACTION_VIEW.equals(intent.getAction())) {
            uri = intent.getData();
            // invito a un gruppo di studio (link alla web app): lo si passa a Rust come file .invite
            if (uri != null && "https".equals(uri.getScheme())) {
                saveInvite(uri.toString());
                return;
            }
        } else if (Intent.ACTION_SEND.equals(intent.getAction())) {
            uri = android.os.Build.VERSION.SDK_INT >= 33
                ? intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri.class)
                : legacyStream(intent);
        }
        if (uri != null) receive(uri);
    }

    private void saveInvite(String link) {
        File dir = new File(getFilesDir(), "import");
        dir.mkdirs();
        File f = new File(dir, "gruppo-" + System.currentTimeMillis() + ".invite");
        try (OutputStream out = new FileOutputStream(f)) {
            out.write(link.getBytes(StandardCharsets.UTF_8));
        } catch (Exception e) {
            f.delete();
        }
    }

    /** Condivisione di sistema di un testo (invito al gruppo), chiamata da Rust via JNI. */
    public void shareText(String text) {
        runOnUiThread(() -> {
            Intent i = new Intent(Intent.ACTION_SEND);
            i.setType("text/plain");
            i.putExtra(Intent.EXTRA_TEXT, text);
            try {
                startActivity(Intent.createChooser(i, "Invita nel gruppo"));
            } catch (Exception e) {
                // nessuna app per condividere
            }
        });
    }

    /** Modalità concentrazione: fissa lo schermo sull'app mentre si studia (Android chiede conferma). */
    // ---------- modalità concentrazione (vedi FocusService) ----------

    /** Permessi e stato: {"usage":…, "overlay":…, "until":ms, "armed":…, "session":n, "allowed":[…]}. */
    public String focusState() {
        try {
            android.content.SharedPreferences p = FocusService.prefs(this);
            org.json.JSONObject j = new org.json.JSONObject();
            j.put("usage", FocusService.hasUsageAccess(this));
            j.put("overlay", FocusService.hasOverlay(this));
            j.put("until", p.getLong("until", 0));
            j.put("armed", p.getBoolean("armed", false));
            j.put("session", p.getInt("session", 0));
            j.put("active", FocusService.enforcing(this));
            org.json.JSONArray a = new org.json.JSONArray();
            for (String s : p.getStringSet("allowed", new java.util.HashSet<>())) a.put(s);
            j.put("allowed", a);
            return j.toString();
        } catch (Exception e) {
            return "{}";
        }
    }

    /** App avviabili (tranne questa e quelle sempre permesse), con icona PNG 64 px in base64. */
    public String focusApps() {
        org.json.JSONArray out = new org.json.JSONArray();
        try {
            PackageManager pm = getPackageManager();
            Intent main = new Intent(Intent.ACTION_MAIN).addCategory(Intent.CATEGORY_LAUNCHER);
            java.util.Set<String> always = FocusService.alwaysAllowed(this);
            java.util.Set<String> seen = new java.util.HashSet<>();
            int px = (int) (64 * getResources().getDisplayMetrics().density / 2);
            for (android.content.pm.ResolveInfo ri : pm.queryIntentActivities(main, 0)) {
                String pkg = ri.activityInfo.packageName;
                if (always.contains(pkg) || !seen.add(pkg)) continue;
                org.json.JSONObject j = new org.json.JSONObject();
                j.put("p", pkg);
                j.put("l", ri.loadLabel(pm).toString());
                try {
                    android.graphics.drawable.Drawable d = ri.loadIcon(pm);
                    android.graphics.Bitmap bm = android.graphics.Bitmap.createBitmap(px, px, android.graphics.Bitmap.Config.ARGB_8888);
                    android.graphics.Canvas cv = new android.graphics.Canvas(bm);
                    d.setBounds(0, 0, px, px);
                    d.draw(cv);
                    java.io.ByteArrayOutputStream bo = new java.io.ByteArrayOutputStream();
                    bm.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, bo);
                    j.put("i", android.util.Base64.encodeToString(bo.toByteArray(), android.util.Base64.NO_WRAP));
                } catch (Exception e) {
                    // senza icona
                }
                out.put(j);
            }
        } catch (Exception e) {
            // elenco vuoto
        }
        return out.toString();
    }

    /** Avvia la concentrazione: {"until":ms, "allowed":[…]} (la sessione di studio parte con essa). */
    public void focusStart(String json) {
        try {
            org.json.JSONObject j = new org.json.JSONObject(json);
            java.util.Set<String> allowed = new java.util.HashSet<>();
            org.json.JSONArray a = j.optJSONArray("allowed");
            if (a != null) for (int i = 0; i < a.length(); i++) allowed.add(a.getString(i));
            FocusService.prefs(this).edit()
                .putLong("until", j.optLong("until", 0))
                .putBoolean("armed", true)
                .putBoolean("used", true)
                .putInt("session", FocusPolicy.STUDYING)
                .putStringSet("allowed", allowed)
                .apply();
            FocusService.ensure(this);
        } catch (Exception e) {
            // dati non validi
        }
    }

    /** Stato della sessione di studio ("0" ferma, "1" in pista, "2" pit stop): «Termina» disarma. */
    public void focusSession(String state) {
        int s;
        try {
            s = Integer.parseInt(state);
        } catch (Exception e) {
            return;
        }
        android.content.SharedPreferences.Editor ed = FocusService.prefs(this).edit().putInt("session", s);
        if (s == FocusPolicy.IDLE) ed.putBoolean("armed", false);
        ed.apply();
        FocusService.ensure(this);
    }

    /** Apre le impostazioni di un permesso: "usage", "overlay" o "appinfo" (impostazioni limitate). */
    public void focusPermission(String kind) {
        runOnUiThread(() -> {
            Intent i;
            Uri pkg = Uri.parse("package:" + getPackageName());
            if ("usage".equals(kind)) {
                i = new Intent(android.provider.Settings.ACTION_USAGE_ACCESS_SETTINGS, pkg);
            } else if ("overlay".equals(kind)) {
                i = new Intent(android.provider.Settings.ACTION_MANAGE_OVERLAY_PERMISSION, pkg);
            } else {
                i = new Intent(android.provider.Settings.ACTION_APPLICATION_DETAILS_SETTINGS, pkg);
            }
            try {
                startActivity(i);
            } catch (Exception e) {
                try {
                    // alcuni telefoni non accettano il pacchetto nell'indirizzo
                    startActivity(new Intent(i.getAction()));
                } catch (Exception e2) {
                    // nessuna schermata
                }
            }
        });
    }

    /** Apre un'app permessa (dal lanciatore dentro l'app). */
    public void launchApp(String pkg) {
        runOnUiThread(() -> FocusService.launch(this, pkg));
    }

    /** Apre il telefono (chiamate e numeri d'emergenza sono sempre possibili). */
    public void dialPhone() {
        runOnUiThread(() -> FocusService.dial(this));
    }

    /**
     * Colori di sistema (Material You): 5 tavolozze × 13 tonalità (accent1..3, neutral1..2;
     * 0, 10, 50, 100 … 900, 1000), lette da Rust per il tema e la monoposto.
     */
    @SuppressWarnings("deprecation")
    private static Uri legacyStream(Intent intent) {
        return intent.getParcelableExtra(Intent.EXTRA_STREAM);
    }

    public int[] systemTones() {
        // i colori di sistema (Material You) ci sono da Android 12
        if (android.os.Build.VERSION.SDK_INT < 31) return new int[0];
        int[][] ids = {
            {android.R.color.system_accent1_0, android.R.color.system_accent1_10, android.R.color.system_accent1_50,
             android.R.color.system_accent1_100, android.R.color.system_accent1_200, android.R.color.system_accent1_300,
             android.R.color.system_accent1_400, android.R.color.system_accent1_500, android.R.color.system_accent1_600,
             android.R.color.system_accent1_700, android.R.color.system_accent1_800, android.R.color.system_accent1_900,
             android.R.color.system_accent1_1000},
            {android.R.color.system_accent2_0, android.R.color.system_accent2_10, android.R.color.system_accent2_50,
             android.R.color.system_accent2_100, android.R.color.system_accent2_200, android.R.color.system_accent2_300,
             android.R.color.system_accent2_400, android.R.color.system_accent2_500, android.R.color.system_accent2_600,
             android.R.color.system_accent2_700, android.R.color.system_accent2_800, android.R.color.system_accent2_900,
             android.R.color.system_accent2_1000},
            {android.R.color.system_accent3_0, android.R.color.system_accent3_10, android.R.color.system_accent3_50,
             android.R.color.system_accent3_100, android.R.color.system_accent3_200, android.R.color.system_accent3_300,
             android.R.color.system_accent3_400, android.R.color.system_accent3_500, android.R.color.system_accent3_600,
             android.R.color.system_accent3_700, android.R.color.system_accent3_800, android.R.color.system_accent3_900,
             android.R.color.system_accent3_1000},
            {android.R.color.system_neutral1_0, android.R.color.system_neutral1_10, android.R.color.system_neutral1_50,
             android.R.color.system_neutral1_100, android.R.color.system_neutral1_200, android.R.color.system_neutral1_300,
             android.R.color.system_neutral1_400, android.R.color.system_neutral1_500, android.R.color.system_neutral1_600,
             android.R.color.system_neutral1_700, android.R.color.system_neutral1_800, android.R.color.system_neutral1_900,
             android.R.color.system_neutral1_1000},
            {android.R.color.system_neutral2_0, android.R.color.system_neutral2_10, android.R.color.system_neutral2_50,
             android.R.color.system_neutral2_100, android.R.color.system_neutral2_200, android.R.color.system_neutral2_300,
             android.R.color.system_neutral2_400, android.R.color.system_neutral2_500, android.R.color.system_neutral2_600,
             android.R.color.system_neutral2_700, android.R.color.system_neutral2_800, android.R.color.system_neutral2_900,
             android.R.color.system_neutral2_1000},
        };
        int[] out = new int[65];
        try {
            for (int p = 0; p < 5; p++) {
                for (int t = 0; t < 13; t++) out[p * 13 + t] = getResources().getColor(ids[p][t], null);
            }
        } catch (Exception e) {
            return new int[0];
        }
        return out;
    }

    /** Copia il file in files/import/ (prima .tmp, poi rinomina: Rust non legge mai a metà). */
    private void receive(Uri uri) {
        receive(uri, "pack", ".json.gz");
    }

    private void receive(Uri uri, String prefix, String ext) {
        final File dir = new File(getFilesDir(), "import");
        new Thread(() -> {
            dir.mkdirs();
            File tmp = new File(dir, prefix + ".tmp");
            File dst = new File(dir, prefix + "-" + System.currentTimeMillis() + ext);
            try (InputStream in = getContentResolver().openInputStream(uri);
                 OutputStream out = new FileOutputStream(tmp)) {
                if (in == null) return;
                byte[] buf = new byte[1 << 16];
                long total = 0;
                int n;
                while ((n = in.read(buf)) > 0) {
                    total += n;
                    if (total > (256L << 20)) throw new java.io.IOException("file troppo grande");
                    out.write(buf, 0, n);
                }
            } catch (Exception e) {
                tmp.delete();
                return;
            }
            if (!tmp.renameTo(dst)) tmp.delete();
        }).start();
    }
}
