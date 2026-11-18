package io.github.nernerone.tremendouspreparator;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.graphics.drawable.Icon;
import java.text.SimpleDateFormat;
import java.util.ArrayList;
import java.util.Date;
import java.util.List;
import java.util.Locale;
import org.json.JSONObject;

/**
 * Notifica fissa della sessione di studio: cronometro, cosa si studia e avanzamento nel colore
 * dell'app. Su Android 16+ è un «aggiornamento in tempo reale» (ProgressStyle, come la
 * navigazione di Maps): segmenti da 25 minuti, uno per scatto del ritmo. Usa lo stesso numero
 * della notifica del servizio di concentrazione, così ce n'è una sola.
 */
final class StudyNotify {
    private StudyNotify() {}

    static final String CHANNEL = "studio";
    static final int ID = 4320;

    /** Canale visibile ma senza suono né vibrazione (aggiornamenti continui). */
    static void channel(Context c) {
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        if (nm == null) return;
        NotificationChannel ch = new NotificationChannel(CHANNEL, "Sessione di studio", NotificationManager.IMPORTANCE_DEFAULT);
        ch.setSound(null, null);
        ch.enableVibration(false);
        ch.setShowBadge(false);
        nm.createNotificationChannel(ch);
    }

    static int smallIcon(Context c) {
        int icon = c.getResources().getIdentifier("ic_notify", "drawable", c.getPackageName());
        return icon != 0 ? icon : c.getApplicationInfo().icon;
    }

    private static PendingIntent open(Context c) {
        Intent i = c.getPackageManager().getLaunchIntentForPackage(c.getPackageName());
        if (i != null) i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
        return PendingIntent.getActivity(c, 7, i, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    /**
     * Stato dal lato Rust: {"state":0|1|2, "title", "text", "since":ms, "progress", "max",
     * "segment", "color":argb, "focus_until":ms}. Stato 0 = nessuna sessione: si toglie.
     */
    static void show(Context c, String json) {
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        if (nm == null) return;
        JSONObject j;
        try {
            j = new JSONObject(json);
        } catch (Exception e) {
            return;
        }
        if (j.optInt("state", 0) == 0) {
            // durante la concentrazione la notifica è del servizio: resta
            if (!FocusService.enforcing(c)) nm.cancel(ID);
            return;
        }
        channel(c);
        nm.notify(ID, build(c, j));
    }

    static Notification build(Context c, JSONObject j) {
        int color = (int) j.optLong("color", 0xff1565c0L);
        long since = j.optLong("since", System.currentTimeMillis());
        int max = Math.max(1, j.optInt("max", 200));
        int progress = Math.max(0, Math.min(max, j.optInt("progress", 0)));
        int segment = Math.max(1, j.optInt("segment", 25));
        long until = j.optLong("focus_until", 0);
        Notification.Builder b = new Notification.Builder(c, CHANNEL)
            .setSmallIcon(smallIcon(c))
            .setContentTitle(j.optString("title", "Sessione di studio"))
            .setContentText(j.optString("text", ""))
            .setContentIntent(open(c))
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setCategory(Notification.CATEGORY_PROGRESS)
            .setColor(color)
            .setShowWhen(true)
            .setWhen(since)
            .setUsesChronometer(true);
        if (until > System.currentTimeMillis()) {
            b.setSubText("Concentrazione fino alle " + new SimpleDateFormat("HH:mm", Locale.ITALY).format(new Date(until)));
            // notifica del servizio in primo piano: può avere lo sfondo nel colore dell'app
            b.setColorized(true);
        }
        boolean colorized = until > System.currentTimeMillis();
        if (!progressStyle(c, b, progress, max, segment, color, colorized)) {
            b.setProgress(max, progress, false);
        }
        // Android 16: promossa ad aggiornamento in tempo reale (barra di stato, schermata di blocco)
        try {
            Notification.Builder.class.getMethod("setRequestPromotedOngoing", boolean.class).invoke(b, true);
        } catch (Exception e) {
            // versione precedente
        }
        return b.build();
    }

    /** ProgressStyle (Android 16+), per riflessione: compila con la piattaforma attuale. */
    private static boolean progressStyle(Context c, Notification.Builder b, int progress, int max, int segment, int color,
                                         boolean colorized) {
        if (android.os.Build.VERSION.SDK_INT < 36) return false;
        try {
            Class<?> ps = Class.forName("android.app.Notification$ProgressStyle");
            Class<?> seg = Class.forName("android.app.Notification$ProgressStyle$Segment");
            Object style = ps.getConstructor().newInstance();
            List<Object> segs = new ArrayList<>();
            int faded = (color & 0x00ffffff) | 0x55000000;
            for (int done = 0; done < max; done += segment) {
                int len = Math.min(segment, max - done);
                Object s = seg.getConstructor(int.class).newInstance(len);
                seg.getMethod("setColor", int.class).invoke(s, done < progress ? color : faded);
                segs.add(s);
            }
            ps.getMethod("setProgressSegments", List.class).invoke(style, segs);
            ps.getMethod("setProgress", int.class).invoke(style, progress);
            ps.getMethod("setStyledByProgress", boolean.class).invoke(style, true);
            // la monoposto (muso nel verso della barra) al posto dell'icona dell'app
            int car = c.getResources().getIdentifier("ic_car", "drawable", c.getPackageName());
            Icon tracker = Icon.createWithResource(c, car != 0 ? car : smallIcon(c));
            // su fondo nel colore dell'app resta bianca, altrimenti prende il colore dell'app
            if (!colorized) tracker.setTint(color);
            ps.getMethod("setProgressTrackerIcon", Icon.class).invoke(style, tracker);
            b.setStyle((Notification.Style) style);
            return true;
        } catch (Exception e) {
            return false;
        }
    }
}
