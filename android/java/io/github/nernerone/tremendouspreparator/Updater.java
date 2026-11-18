package io.github.nernerone.tremendouspreparator;

import android.app.Activity;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.content.pm.PackageInstaller;
import android.net.Uri;
import android.os.Build;
import android.provider.Settings;
import java.io.File;
import java.io.FileInputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Aggiornamento dell'app (solo APK distribuiti su GitHub): installa l'APK scaricato con l'installatore di
 * sistema. Android chiede conferma e accetta solo un APK firmato con la stessa chiave.
 */
final class Updater {
    private static final String ACTION = "io.github.nernerone.tremendouspreparator.AGGIORNAMENTO";
    private static boolean registered = false;

    private Updater() {}

    /** Avvia l'installazione; restituisce "" se è partita, altrimenti il motivo. */
    static String install(Activity a, String path) {
        try {
            if (Build.VERSION.SDK_INT >= 26 && !a.getPackageManager().canRequestPackageInstalls()) {
                // permesso «Installa app sconosciute» per questa app: si concede una volta
                Intent s = new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                    Uri.parse("package:" + a.getPackageName()));
                a.startActivity(s);
                return "concedi «Installa app sconosciute» a questa app, poi premi di nuovo Aggiorna";
            }
            File f = new File(path);
            PackageInstaller pi = a.getPackageManager().getPackageInstaller();
            PackageInstaller.SessionParams params =
                new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
            params.setSize(f.length());
            int id = pi.createSession(params);
            try (PackageInstaller.Session session = pi.openSession(id)) {
                try (InputStream in = new FileInputStream(f);
                     OutputStream out = session.openWrite("app.apk", 0, f.length())) {
                    byte[] buf = new byte[1 << 16];
                    int n;
                    while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
                    session.fsync(out);
                }
                listen(a.getApplicationContext());
                Intent result = new Intent(ACTION).setPackage(a.getPackageName());
                int flags = PendingIntent.FLAG_UPDATE_CURRENT
                    | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
                PendingIntent pending = PendingIntent.getBroadcast(a, id, result, flags);
                session.commit(pending.getIntentSender());
            }
            return "";
        } catch (Exception e) {
            return e.toString();
        }
    }

    /** Esito della sessione: se serve la conferma dell'utente si apre la schermata di sistema. */
    private static synchronized void listen(Context c) {
        if (registered) return;
        registered = true;
        BroadcastReceiver r = new BroadcastReceiver() {
            @Override
            public void onReceive(Context ctx, Intent i) {
                int status = i.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
                if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
                    Intent confirm = i.getParcelableExtra(Intent.EXTRA_INTENT);
                    if (confirm != null) {
                        confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                        ctx.startActivity(confirm);
                    }
                }
            }
        };
        IntentFilter f = new IntentFilter(ACTION);
        if (Build.VERSION.SDK_INT >= 33) {
            c.registerReceiver(r, f, Context.RECEIVER_NOT_EXPORTED);
        } else {
            c.registerReceiver(r, f);
        }
    }
}
