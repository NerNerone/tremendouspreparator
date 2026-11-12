package it.tremendouspreparator.app;

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
        if (checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
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

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PICK_PACK && resultCode == RESULT_OK && data != null && data.getData() != null) {
            receive(data.getData());
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
            uri = intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri.class);
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
    public void lockStudy() {
        runOnUiThread(() -> {
            try {
                startLockTask();
            } catch (Exception e) {
                // non disponibile
            }
        });
    }

    public void unlockStudy() {
        runOnUiThread(() -> {
            try {
                stopLockTask();
            } catch (Exception e) {
                // non era fissato
            }
        });
    }

    /**
     * Colori di sistema (Material You): 5 tavolozze × 13 tonalità (accent1..3, neutral1..2;
     * 0, 10, 50, 100 … 900, 1000), lette da Rust per il tema e la monoposto.
     */
    public int[] systemTones() {
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
        final File dir = new File(getFilesDir(), "import");
        new Thread(() -> {
            dir.mkdirs();
            File tmp = new File(dir, "pack.tmp");
            File dst = new File(dir, "pack-" + System.currentTimeMillis() + ".json.gz");
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
