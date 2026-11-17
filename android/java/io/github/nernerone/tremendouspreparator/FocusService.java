package io.github.nernerone.tremendouspreparator;

import android.app.AppOpsManager;
import android.app.KeyguardManager;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.app.usage.UsageEvents;
import android.app.usage.UsageStatsManager;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.content.pm.ResolveInfo;
import android.content.pm.ServiceInfo;
import android.graphics.Color;
import android.graphics.PixelFormat;
import android.graphics.drawable.GradientDrawable;
import android.media.AudioManager;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.Process;
import android.provider.AlarmClock;
import android.provider.Settings;
import android.telecom.TelecomManager;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.view.WindowManager;
import android.widget.Button;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.HashSet;
import java.util.Locale;
import java.util.Set;

/**
 * Modalità concentrazione: servizio in primo piano che guarda quale app è davanti (accesso
 * all'utilizzo, letto solo sul telefono) e, se non è permessa, mostra l'avviso sovrapposto e
 * riporta all'app. Gira in un processo a parte («:focus», vedi patch_manifest.py), così non
 * tiene in vita quello dell'app: l'app scrive lo stato nelle SharedPreferences «focus» e lo
 * manda al servizio nell'intent di avvio; il servizio ne tiene la sua copia («focus_svc») per
 * ripartire da solo (blocco che continua anche se l'app viene chiusa, e dopo un riavvio).
 */
public class FocusService extends Service {
    static final String PREFS = "focus";
    private static final String SVC_PREFS = "focus_svc";
    private static final String CHANNEL = "concentrazione";
    private static final int NOTIFICATION_ID = 4320;
    private static final long POLL_MS = 700;
    /** Durata dell'avviso sovrapposto. */
    private static final long WARNING_MS = 9000;

    static final String WARNING_TITLE = "Rimani nell'app!";
    static final String WARNING_TEXT =
        "Se devi usare un app adibita allo studio includila alla lista prima dell'avvio della sessione o stoppa la sessione";

    private final Handler handler = new Handler(Looper.getMainLooper());
    private String foreground = "";
    private long lastQuery = 0;
    private long lastBounce = 0;
    private View overlay;
    private Set<String> always = new HashSet<>();

    // ---------- stato (scritto dall'app, letto dal servizio) ----------

    static SharedPreferences prefs(Context c) {
        return c.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    static boolean enforcing(SharedPreferences p) {
        return FocusPolicy.enforcing(System.currentTimeMillis(), p.getLong("until", 0),
            p.getBoolean("armed", false), p.getInt("session", 0));
    }

    static boolean enforcing(Context c) {
        return enforcing(prefs(c));
    }

    /** Manda lo stato dell'app al servizio: lo avvia se il blocco è attivo, altrimenti lo ferma. */
    static void ensure(Context c) {
        SharedPreferences p = prefs(c);
        // chi non ha mai avviato la concentrazione non deve far partire nessun processo
        if (!p.getBoolean("used", false)) return;
        Intent i = new Intent(c, FocusService.class)
            .putExtra("until", p.getLong("until", 0))
            .putExtra("armed", p.getBoolean("armed", false))
            .putExtra("session", p.getInt("session", 0))
            .putExtra("allowed", new java.util.ArrayList<>(p.getStringSet("allowed", new HashSet<>())));
        try {
            if (enforcing(p)) {
                c.startForegroundService(i);
            } else {
                // se era attivo lo ferma (l'app è in primo piano: un servizio normale può partire)
                c.startService(i);
            }
        } catch (Exception e) {
            // avvio non permesso in questo momento (es. app in background): riprova all'apertura
        }
    }

    /** Dopo un riavvio: la sessione di studio non c'è più, resta solo il blocco a tempo. */
    static void afterBoot(Context c) {
        prefs(c).edit().putInt("session", 0).putBoolean("armed", false).apply();
        ensure(c);
    }

    static boolean hasUsageAccess(Context c) {
        AppOpsManager ops = c.getSystemService(AppOpsManager.class);
        if (ops == null) return false;
        int mode = android.os.Build.VERSION.SDK_INT >= 29
            ? ops.unsafeCheckOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), c.getPackageName())
            : legacyCheckOp(ops, c);
        return mode == AppOpsManager.MODE_ALLOWED;
    }

    @SuppressWarnings("deprecation")
    private static int legacyCheckOp(AppOpsManager ops, Context c) {
        return ops.checkOpNoThrow(AppOpsManager.OPSTR_GET_USAGE_STATS, Process.myUid(), c.getPackageName());
    }

    static boolean hasOverlay(Context c) {
        return Settings.canDrawOverlays(c);
    }

    /** App sempre permesse: telefono e chiamate, sveglia, interfaccia di sistema, permessi. */
    static Set<String> alwaysAllowed(Context c) {
        Set<String> s = new HashSet<>();
        s.add(c.getPackageName());
        String[] fixed = {
            "android", "com.android.systemui", "com.android.phone", "com.android.server.telecom",
            "com.android.emergency", "com.google.android.dialer", "com.android.dialer",
            "com.google.android.permissioncontroller", "com.android.permissioncontroller",
            "com.google.android.deskclock", "com.android.deskclock",
            "com.google.android.documentsui", "com.android.documentsui", "com.android.intentresolver",
            "com.google.android.providers.media.module", "com.android.providers.media.module",
        };
        for (String f : fixed) s.add(f);
        TelecomManager tm = c.getSystemService(TelecomManager.class);
        if (tm != null && tm.getDefaultDialerPackage() != null) s.add(tm.getDefaultDialerPackage());
        PackageManager pm = c.getPackageManager();
        // anche i selettori di sistema usati dall'app (file, foto, condivisione)
        for (String action : new String[] {Intent.ACTION_DIAL, AlarmClock.ACTION_SHOW_ALARMS,
                Intent.ACTION_OPEN_DOCUMENT, Intent.ACTION_GET_CONTENT, android.provider.MediaStore.ACTION_PICK_IMAGES}) {
            try {
                for (ResolveInfo ri : pm.queryIntentActivities(new Intent(action), 0)) {
                    s.add(ri.activityInfo.packageName);
                }
            } catch (Exception e) {
                // nessuno
            }
        }
        return s;
    }

    // ---------- servizio ----------

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }

    private SharedPreferences svc() {
        return getSharedPreferences(SVC_PREFS, Context.MODE_PRIVATE);
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && intent.hasExtra("until")) {
            java.util.ArrayList<String> allowed = intent.getStringArrayListExtra("allowed");
            svc().edit()
                .putLong("until", intent.getLongExtra("until", 0))
                .putBoolean("armed", intent.getBooleanExtra("armed", false))
                .putInt("session", intent.getIntExtra("session", 0))
                .putStringSet("allowed", new HashSet<>(allowed != null ? allowed : new java.util.ArrayList<>()))
                .commit();
        }
        if (!enforcing(svc())) {
            stopSelf();
            return START_NOT_STICKY;
        }
        always = alwaysAllowed(this);
        try {
            if (android.os.Build.VERSION.SDK_INT >= 34) {
                startForeground(NOTIFICATION_ID, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
            } else {
                startForeground(NOTIFICATION_ID, notification());
            }
        } catch (Exception e) {
            // riavvio dal sistema non permesso ora: riparte all'apertura dell'app (onResume)
            stopSelf();
            return START_NOT_STICKY;
        }
        handler.removeCallbacks(poll);
        lastQuery = System.currentTimeMillis() - 5000;
        handler.post(poll);
        return START_STICKY;
    }

    @Override
    public void onDestroy() {
        handler.removeCallbacks(poll);
        hideOverlay();
        super.onDestroy();
    }

    private Notification notification() {
        NotificationManager nm = getSystemService(NotificationManager.class);
        if (nm != null) {
            nm.createNotificationChannel(new NotificationChannel(CHANNEL, "Concentrazione", NotificationManager.IMPORTANCE_LOW));
        }
        long until = svc().getLong("until", 0);
        String text = until > System.currentTimeMillis()
            ? "Solo Tremendous e le app scelte fino alle " + new SimpleDateFormat("HH:mm", Locale.ITALY).format(new Date(until))
            : "Solo Tremendous e le app scelte finché non termini la sessione";
        Intent open = getPackageManager().getLaunchIntentForPackage(getPackageName());
        PendingIntent pi = PendingIntent.getActivity(this, 1, open, PendingIntent.FLAG_IMMUTABLE);
        int icon = getResources().getIdentifier("ic_notify", "drawable", getPackageName());
        return new Notification.Builder(this, CHANNEL)
            .setSmallIcon(icon != 0 ? icon : getApplicationInfo().icon)
            .setContentTitle("Concentrazione attiva")
            .setContentText(text)
            .setOngoing(true)
            .setContentIntent(pi)
            .build();
    }

    private final Runnable poll = new Runnable() {
        @Override
        public void run() {
            if (!enforcing(svc())) {
                stopForeground(STOP_FOREGROUND_REMOVE);
                stopSelf();
                return;
            }
            try {
                check();
            } catch (Exception e) {
                // un controllo fallito non deve fermare il servizio
            }
            handler.postDelayed(this, POLL_MS);
        }
    };

    /** Aggiorna l'app in primo piano dagli eventi d'uso dall'ultimo controllo. */
    private void updateForeground() {
        UsageStatsManager usm = getSystemService(UsageStatsManager.class);
        if (usm == null) return;
        long now = System.currentTimeMillis();
        UsageEvents ev = usm.queryEvents(lastQuery - 1000, now);
        lastQuery = now;
        UsageEvents.Event e = new UsageEvents.Event();
        long last = 0;
        while (ev.hasNextEvent()) {
            ev.getNextEvent(e);
            if (e.getEventType() == UsageEvents.Event.ACTIVITY_RESUMED && e.getTimeStamp() >= last) {
                last = e.getTimeStamp();
                foreground = e.getPackageName();
            }
        }
        // nessun evento = nessun cambiamento: resta quella di prima
    }

    private void check() {
        if (!hasUsageAccess(this)) return;
        updateForeground();
        SharedPreferences p = svc();
        KeyguardManager km = getSystemService(KeyguardManager.class);
        AudioManager am = getSystemService(AudioManager.class);
        boolean locked = km != null && km.isKeyguardLocked();
        int mode = am != null ? am.getMode() : AudioManager.MODE_NORMAL;
        boolean inCall = mode == AudioManager.MODE_IN_CALL || mode == AudioManager.MODE_IN_COMMUNICATION
            || mode == AudioManager.MODE_RINGTONE || mode == AudioManager.MODE_CALL_SCREENING;
        Set<String> allowed = p.getStringSet("allowed", new HashSet<>());
        boolean block = FocusPolicy.shouldBlock(System.currentTimeMillis(), p.getLong("until", 0),
            p.getBoolean("armed", false), p.getInt("session", 0), foreground, locked, inCall,
            getPackageName(), allowed, always);
        if (!block) return;
        long now = System.currentTimeMillis();
        if (now - lastBounce < 1200) return;
        lastBounce = now;
        // prima l'avviso, poi (al controllo successivo, con l'avviso già visibile: è quello che
        // permette di riaprire l'app dal servizio) l'app
        if (overlay == null) {
            showOverlay(allowed);
        } else {
            keepOverlay();
            backToApp(this);
        }
    }

    static void backToApp(Context c) {
        Intent i = c.getPackageManager().getLaunchIntentForPackage(c.getPackageName());
        if (i == null) return;
        i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_REORDER_TO_FRONT);
        try {
            c.startActivity(i);
        } catch (Exception e) {
            // non permesso: resta l'avviso
        }
    }

    static void launch(Context c, String pkg) {
        Intent i = c.getPackageManager().getLaunchIntentForPackage(pkg);
        if (i == null) return;
        i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try {
            c.startActivity(i);
        } catch (Exception e) {
            // app non avviabile
        }
    }

    static void dial(Context c) {
        Intent i = new Intent(Intent.ACTION_DIAL);
        i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try {
            c.startActivity(i);
        } catch (Exception e) {
            // nessun telefono
        }
    }

    // ---------- avviso sovrapposto ----------

    private int dp(float v) {
        return (int) TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, getResources().getDisplayMetrics());
    }

    private final Runnable hide = this::hideOverlay;

    /** L'avviso resta altri WARNING_MS. */
    private void keepOverlay() {
        handler.removeCallbacks(hide);
        handler.postDelayed(hide, WARNING_MS);
    }

    private void hideOverlay() {
        handler.removeCallbacks(hide);
        if (overlay == null) return;
        try {
            getSystemService(WindowManager.class).removeView(overlay);
        } catch (Exception e) {
            // già tolta
        }
        overlay = null;
    }

    private Button chip(String text, View.OnClickListener l) {
        Button b = new Button(this);
        b.setText(text);
        b.setAllCaps(false);
        b.setTextColor(Color.WHITE);
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(0x33FFFFFF);
        bg.setCornerRadius(dp(18));
        b.setBackground(bg);
        b.setPadding(dp(14), 0, dp(14), 0);
        b.setMinHeight(dp(36));
        b.setMinimumHeight(dp(36));
        b.setOnClickListener(l);
        return b;
    }

    private void showOverlay(Set<String> allowed) {
        if (!hasOverlay(this)) return;
        hideOverlay();
        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(dp(18), dp(14), dp(18), dp(14));
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(0xF0202124);
        bg.setCornerRadius(dp(20));
        box.setBackground(bg);

        TextView title = new TextView(this);
        title.setText(WARNING_TITLE);
        title.setTextColor(Color.WHITE);
        title.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18);
        title.setTypeface(title.getTypeface(), android.graphics.Typeface.BOLD);
        box.addView(title);

        TextView text = new TextView(this);
        text.setText("(" + WARNING_TEXT + ")");
        text.setTextColor(0xDDFFFFFF);
        text.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        text.setPadding(0, dp(4), 0, dp(10));
        box.addView(text);

        // app permesse, Telefono, chiudi
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        PackageManager pm = getPackageManager();
        for (String pkg : allowed) {
            String label = pkg;
            try {
                label = pm.getApplicationLabel(pm.getApplicationInfo(pkg, 0)).toString();
            } catch (Exception e) {
                continue;
            }
            Button b = chip(label, v -> {
                launch(this, pkg);
                hideOverlay();
            });
            LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-2, -2);
            lp.setMarginEnd(dp(8));
            row.addView(b, lp);
        }
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(-2, -2);
        lp.setMarginEnd(dp(8));
        row.addView(chip("Telefono", v -> {
            dial(this);
            hideOverlay();
        }), lp);
        row.addView(chip("Ok", v -> hideOverlay()), new LinearLayout.LayoutParams(-2, -2));
        HorizontalScrollView scroll = new HorizontalScrollView(this);
        scroll.setHorizontalScrollBarEnabled(false);
        scroll.addView(row);
        box.addView(scroll);

        WindowManager.LayoutParams wp = new WindowManager.LayoutParams(
            WindowManager.LayoutParams.MATCH_PARENT, WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT);
        wp.gravity = Gravity.BOTTOM;
        wp.x = 0;
        wp.y = dp(24);
        wp.horizontalMargin = 0.03f;
        try {
            getSystemService(WindowManager.class).addView(box, wp);
            overlay = box;
            keepOverlay();
        } catch (Exception e) {
            overlay = null;
        }
    }
}
