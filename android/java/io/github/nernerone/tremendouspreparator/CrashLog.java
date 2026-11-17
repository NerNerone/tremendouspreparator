package io.github.nernerone.tremendouspreparator;

import android.content.Context;
import android.os.Build;
import android.util.Log;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.charset.StandardCharsets;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;
import java.util.TimeZone;

/**
 * Eccezioni Java non gestite: il rapporto va in files/crash/ (la stessa cartella dei panic
 * Rust); al successivo avvio l'app propone di segnalarlo. Poi il gestore di sistema chiude
 * l'app come al solito.
 */
final class CrashLog {
    private CrashLog() {}

    static void install(Context c) {
        Context app = c.getApplicationContext();
        Thread.UncaughtExceptionHandler prev = Thread.getDefaultUncaughtExceptionHandler();
        if (prev != null && prev.getClass().getName().startsWith(CrashLog.class.getName())) return;
        Thread.setDefaultUncaughtExceptionHandler((t, e) -> {
            try {
                write(app, t, e);
            } catch (Throwable ignored) {
                // il rapporto non deve impedire la chiusura normale
            }
            if (prev != null) prev.uncaughtException(t, e);
        });
    }

    private static void write(Context c, Thread t, Throwable e) throws Exception {
        SimpleDateFormat utc = new SimpleDateFormat("yyyy-MM-dd HH:mm:ss 'UTC'", Locale.ROOT);
        utc.setTimeZone(TimeZone.getTimeZone("UTC"));
        SimpleDateFormat name = new SimpleDateFormat("'java-'yyyyMMdd'T'HHmmss'.txt'", Locale.ROOT);
        name.setTimeZone(TimeZone.getTimeZone("UTC"));
        Date now = new Date();
        String version = "?";
        try {
            version = c.getPackageManager().getPackageInfo(c.getPackageName(), 0).versionName;
        } catch (Exception ignored) {
            // versione non leggibile
        }
        String text = "Versione: " + version + "\n"
            + "Sistema: Android " + Build.VERSION.RELEASE + " (API " + Build.VERSION.SDK_INT + "), "
            + (Build.SUPPORTED_ABIS.length > 0 ? Build.SUPPORTED_ABIS[0] : "?") + "\n"
            + "Dispositivo: " + Build.MANUFACTURER + " " + Build.MODEL + "\n"
            + "Data: " + utc.format(now) + "\n"
            + "Thread: " + t.getName() + "\n\n"
            + Log.getStackTraceString(e);
        File dir = new File(c.getFilesDir(), "crash");
        if (!dir.isDirectory() && !dir.mkdirs()) return;
        try (FileOutputStream out = new FileOutputStream(new File(dir, name.format(now)))) {
            out.write(text.getBytes(StandardCharsets.UTF_8));
        }
    }
}
