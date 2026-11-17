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
import android.view.ViewGroup;
import android.view.WindowManager;
import android.widget.Button;
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
        "Se devi usare un'app adibita allo studio includila alla lista prima dell'avvio della sessione o stoppa la sessione";

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
    public void onCreate() {
        super.onCreate();
        // processo a parte: serve il suo gestore degli errori
        CrashLog.install(this);
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

    private Button chip(String text, boolean primary, View.OnClickListener l) {
        Button b = new Button(this);
        b.setText(text);
        b.setAllCaps(false);
        b.setSingleLine(true);
        b.setEllipsize(android.text.TextUtils.TruncateAt.END);
        b.setTextColor(primary ? 0xFF202124 : Color.WHITE);
        b.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(primary ? 0xFFE8EAED : 0x33FFFFFF);
        bg.setCornerRadius(dp(20));
        b.setBackground(bg);
        b.setPadding(dp(16), 0, dp(16), 0);
        b.setMinHeight(dp(40));
        b.setMinimumHeight(dp(40));
        b.setMinWidth(0);
        b.setMinimumWidth(0);
        b.setOnClickListener(l);
        return b;
    }

    /** Disposizione a capo: i figli riempiono una riga e poi passano alla successiva. */
    private static final class Flow extends ViewGroup {
        private final int gap;

        Flow(Context c, int gap) {
            super(c);
            this.gap = gap;
        }

        @Override
        protected void onMeasure(int ws, int hs) {
            int max = MeasureSpec.getSize(ws);
            int x = 0, y = 0, rowH = 0, used = 0;
            for (int i = 0; i < getChildCount(); i++) {
                View c = getChildAt(i);
                c.measure(MeasureSpec.makeMeasureSpec(max, MeasureSpec.AT_MOST),
                    MeasureSpec.makeMeasureSpec(0, MeasureSpec.UNSPECIFIED));
                int w = c.getMeasuredWidth();
                if (x > 0 && x + w > max) {
                    x = 0;
                    y += rowH + gap;
                    rowH = 0;
                }
                x += w + gap;
                used = Math.max(used, x - gap);
                rowH = Math.max(rowH, c.getMeasuredHeight());
            }
            setMeasuredDimension(MeasureSpec.getMode(ws) == MeasureSpec.EXACTLY ? max : used, y + rowH);
        }

        @Override
        protected void onLayout(boolean changed, int l, int t, int r, int b) {
            int max = r - l;
            int x = 0, y = 0, rowH = 0;
            for (int i = 0; i < getChildCount(); i++) {
                View c = getChildAt(i);
                int w = c.getMeasuredWidth(), h = c.getMeasuredHeight();
                if (x > 0 && x + w > max) {
                    x = 0;
                    y += rowH + gap;
                    rowH = 0;
                }
                c.layout(x, y, x + w, y + h);
                x += w + gap;
                rowH = Math.max(rowH, h);
            }
        }
    }

    /** Area che scorre in verticale solo oltre un'altezza massima. */
    private static final class Capped extends android.widget.ScrollView {
        private final int max;

        Capped(Context c, int max) {
            super(c);
            this.max = max;
        }

        @Override
        protected void onMeasure(int ws, int hs) {
            super.onMeasure(ws, MeasureSpec.makeMeasureSpec(max, MeasureSpec.AT_MOST));
        }
    }

    private int navBarHeight() {
        int id = getResources().getIdentifier("navigation_bar_height", "dimen", "android");
        return id > 0 ? getResources().getDimensionPixelSize(id) : 0;
    }

    private void showOverlay(Set<String> allowed) {
        if (!hasOverlay(this)) return;
        hideOverlay();
        android.util.DisplayMetrics dm = getResources().getDisplayMetrics();
        // larghezza dello schermo meno i margini, al massimo 560 dp (tablet, orizzontale)
        int width = Math.min(dm.widthPixels - dp(24), dp(560));

        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(dp(20), dp(16), dp(20), dp(16));
        GradientDrawable bg = new GradientDrawable();
        bg.setColor(0xFF202124);
        bg.setCornerRadius(dp(24));
        box.setBackground(bg);
        box.setElevation(dp(8));

        TextView title = new TextView(this);
        title.setText(WARNING_TITLE);
        title.setTextColor(Color.WHITE);
        title.setTextSize(TypedValue.COMPLEX_UNIT_SP, 20);
        title.setTypeface(title.getTypeface(), android.graphics.Typeface.BOLD);
        box.addView(title);

        TextView text = new TextView(this);
        text.setText(WARNING_TEXT);
        text.setTextColor(0xDDFFFFFF);
        text.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14);
        text.setPadding(0, dp(6), 0, dp(14));
        box.addView(text);

        // app permesse e Telefono: vanno a capo; con molte app l'area scorre
        Flow flow = new Flow(this, dp(8));
        PackageManager pm = getPackageManager();
        java.util.List<String[]> apps = new java.util.ArrayList<>();
        for (String pkg : allowed) {
            try {
                apps.add(new String[] {pm.getApplicationLabel(pm.getApplicationInfo(pkg, 0)).toString(), pkg});
            } catch (Exception e) {
                // app disinstallata
            }
        }
        apps.sort((a, b) -> a[0].compareToIgnoreCase(b[0]));
        for (String[] a : apps) {
            flow.addView(chip(a[0], false, v -> {
                launch(this, a[1]);
                hideOverlay();
            }));
        }
        flow.addView(chip("Telefono", false, v -> {
            dial(this);
            hideOverlay();
        }));
        Capped scroll = new Capped(this, (int) (dm.heightPixels * 0.4f));
        scroll.setVerticalScrollBarEnabled(false);
        scroll.addView(flow, new ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));
        box.addView(scroll, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));

        LinearLayout.LayoutParams okp = new LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT);
        okp.gravity = Gravity.END;
        okp.topMargin = dp(14);
        box.addView(chip("Ok", true, v -> hideOverlay()), okp);

        WindowManager.LayoutParams wp = new WindowManager.LayoutParams(
            width, WindowManager.LayoutParams.WRAP_CONTENT,
            WindowManager.LayoutParams.TYPE_APPLICATION_OVERLAY,
            WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN,
            PixelFormat.TRANSLUCENT);
        wp.gravity = Gravity.BOTTOM | Gravity.CENTER_HORIZONTAL;
        wp.x = 0;
        // sopra la barra di navigazione (pulsanti o gesti)
        wp.y = dp(16) + navBarHeight();
        try {
            getSystemService(WindowManager.class).addView(box, wp);
            overlay = box;
            keepOverlay();
        } catch (Exception e) {
            overlay = null;
        }
    }
}
