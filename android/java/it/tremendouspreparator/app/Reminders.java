package it.tremendouspreparator.app;

import android.app.AlarmManager;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import java.io.File;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.ZoneId;
import org.json.JSONArray;
import org.json.JSONObject;

/**
 * Promemoria in quattro fasce (mattina, dopo pranzo, pomeriggio, sera) e notifica fissa con
 * gli obiettivi del giorno. L'app (Rust) scrive files/reminder.json con gli orari e i testi
 * pronti per oggi e domani (ReminderState::compute_slots in crates/prep-core/src/reminder.rs):
 * qui si programma l'allarme della prossima fascia e si mostra il testo di quella scattata.
 */
final class Reminders {
    static final String ACTION = "it.tremendouspreparator.app.PROMEMORIA";
    static final String ACTION_CLEAR = "it.tremendouspreparator.app.MEZZANOTTE";
    static final int NOTIFICATION_ID = 1;
    static final int GOALS_ID = 2;
    private static final String CHANNEL = "promemoria";
    private static final String CHANNEL_GOALS = "obiettivi";
    private static final String[] DEFAULT_TIMES = {"08:30", "13:45", "17:30", "21:00"};

    private Reminders() {}

    private static JSONObject config(Context c) {
        try {
            File f = new File(c.getFilesDir(), "reminder.json");
            return new JSONObject(new String(Files.readAllBytes(f.toPath()), StandardCharsets.UTF_8));
        } catch (Exception e) {
            return null; // app mai aperta o file illeggibile: valori predefiniti
        }
    }

    private static PendingIntent alarmIntent(Context c, String action, int code) {
        Intent i = new Intent(c, ReminderReceiver.class).setAction(action);
        return PendingIntent.getBroadcast(c, code, i, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    /** Orari delle fasce, in minuti dalla mezzanotte. */
    private static int[] times(JSONObject j) {
        JSONArray a = j == null ? null : j.optJSONArray("times");
        int n = a == null || a.length() == 0 ? DEFAULT_TIMES.length : a.length();
        int[] out = new int[n];
        for (int i = 0; i < n; i++) {
            String t = a == null || a.length() == 0 ? DEFAULT_TIMES[i] : a.optString(i, "12:00");
            try {
                String[] p = t.trim().split("[:.]");
                out[i] = Integer.parseInt(p[0]) * 60 + Integer.parseInt(p[1]);
            } catch (Exception e) {
                out[i] = 12 * 60;
            }
        }
        return out;
    }

    private static long at(LocalDateTime t) {
        return t.atZone(ZoneId.systemDefault()).toInstant().toEpochMilli();
    }

    /** Programma l'allarme della prossima fascia (e la pulizia di mezzanotte). */
    static void schedule(Context c) {
        AlarmManager am = c.getSystemService(AlarmManager.class);
        if (am == null) return;
        PendingIntent pi = alarmIntent(c, ACTION, 0);
        PendingIntent clear = alarmIntent(c, ACTION_CLEAR, 3);
        am.cancel(pi);
        am.cancel(clear);
        JSONObject j = config(c);
        if (j != null && !j.optBoolean("enabled", true)) return;
        LocalDateTime now = LocalDateTime.now();
        LocalDateTime best = null;
        for (int m : times(j)) {
            LocalDateTime t = now.toLocalDate().atTime(m / 60, m % 60);
            if (!t.isAfter(now.plusSeconds(30))) t = t.plusDays(1);
            if (best == null || t.isBefore(best)) best = t;
        }
        // inesatto ma consentito anche in Doze, e non richiede il permesso per gli allarmi esatti
        if (best != null) am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at(best), pi);
        if (openGoals(j) != null) {
            am.set(AlarmManager.RTC, at(now.toLocalDate().plusDays(1).atTime(0, 1)), clear);
        }
    }

    /** Fascia scattata adesso (l'ultima con orario passato da meno di 3 ore): {titolo, testo}. */
    static String[] dueSlot(JSONObject j, LocalDateTime now) {
        String today = now.toLocalDate().toString();
        int nowMin = now.getHour() * 60 + now.getMinute();
        int[] ts = times(j);
        int k = -1;
        for (int i = 0; i < ts.length; i++) {
            if (ts[i] <= nowMin && nowMin - ts[i] < 180 && (k < 0 || ts[i] > ts[k])) k = i;
        }
        if (k < 0) return null;
        JSONArray slots = j == null ? null : j.optJSONArray("slots");
        for (int i = 0; slots != null && i < slots.length(); i++) {
            JSONObject s = slots.optJSONObject(i);
            if (s == null || !today.equals(s.optString("date"))) continue;
            if (s.optInt("hour") * 60 + s.optInt("minute") != ts[k]) continue;
            String title = s.optString("title");
            return title.isEmpty() ? null : new String[] {title, s.optString("text")};
        }
        // l'app non è stata aperta di recente: testo generico per la fascia
        if (k == 0) return new String[] {"Buongiorno!", "Apri l'app e scegli gli obiettivi di oggi."};
        if (k == ts.length - 1) return new String[] {"Prima di chiudere la giornata", "Un quiz breve o le flashcard: bastano 10 minuti."};
        return new String[] {"Pausa studio?", "Un blocco da 10 domande richiede circa 15 minuti."};
    }

    /** Obiettivi di oggi ancora aperti, oppure null. */
    private static JSONObject openGoals(JSONObject j) {
        if (j == null || !j.optBoolean("enabled", true)) return null;
        JSONObject g = j.optJSONObject("goals");
        if (g == null || !LocalDate.now().toString().equals(g.optString("date"))) return null;
        JSONArray items = g.optJSONArray("items");
        if (items == null || items.length() == 0) return null;
        for (int i = 0; i < items.length(); i++) {
            JSONObject it = items.optJSONObject(i);
            if (it != null && it.optInt("done") < it.optInt("target")) return g;
        }
        return null;
    }

    private static String goalsSummary(JSONObject g) {
        JSONArray items = g.optJSONArray("items");
        StringBuilder sb = new StringBuilder();
        for (int i = 0; items != null && i < items.length(); i++) {
            JSONObject it = items.optJSONObject(i);
            if (it == null) continue;
            int target = it.optInt("target");
            int done = Math.min(it.optInt("done"), target);
            if (sb.length() > 0) sb.append("\n");
            sb.append(it.optString("name")).append(": ").append(done).append("/").append(target);
        }
        return sb.toString();
    }

    private static PendingIntent openApp(Context c) {
        Intent open = c.getPackageManager().getLaunchIntentForPackage(c.getPackageName());
        if (open == null) open = new Intent(c, MainActivity.class);
        open.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_RESET_TASK_IF_NEEDED);
        return PendingIntent.getActivity(c, 1, open, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    private static int smallIcon(Context c) {
        int icon = c.getResources().getIdentifier("ic_notify", "drawable", c.getPackageName());
        return icon != 0 ? icon : c.getApplicationInfo().icon;
    }

    /** Notifica della fascia scattata. */
    static void show(Context c) {
        JSONObject j = config(c);
        if (j != null && !j.optBoolean("enabled", true)) return;
        String[] msg = dueSlot(j, LocalDateTime.now());
        if (msg == null) return;
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        if (nm == null) return;
        nm.createNotificationChannel(
            new NotificationChannel(CHANNEL, "Promemoria di studio", NotificationManager.IMPORTANCE_DEFAULT));
        Notification n = new Notification.Builder(c, CHANNEL)
            .setSmallIcon(smallIcon(c))
            .setContentTitle(msg[0])
            .setContentText(msg[1])
            .setStyle(new Notification.BigTextStyle().bigText(msg[1]))
            .setContentIntent(openApp(c))
            .setAutoCancel(true)
            .build();
        nm.notify(NOTIFICATION_ID, n);
    }

    /** Notifica fissa con l'avanzamento degli obiettivi di oggi (tolta quando sono completati). */
    static void showGoals(Context c) {
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        if (nm == null) return;
        JSONObject g = openGoals(config(c));
        if (g == null) {
            nm.cancel(GOALS_ID);
            return;
        }
        nm.createNotificationChannel(
            new NotificationChannel(CHANNEL_GOALS, "Obiettivi del giorno", NotificationManager.IMPORTANCE_LOW));
        String body = goalsSummary(g);
        Notification n = new Notification.Builder(c, CHANNEL_GOALS)
            .setSmallIcon(smallIcon(c))
            .setContentTitle("Obiettivi di oggi")
            .setContentText(body.replace("\n", " · "))
            .setStyle(new Notification.BigTextStyle().bigText(body))
            .setContentIntent(openApp(c))
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .build();
        nm.notify(GOALS_ID, n);
    }

    static void clearGoals(Context c) {
        NotificationManager nm = c.getSystemService(NotificationManager.class);
        if (nm != null) nm.cancel(GOALS_ID);
    }
}
