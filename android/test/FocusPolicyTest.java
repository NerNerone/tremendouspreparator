package io.github.nernerone.tremendouspreparator;

import java.util.Set;

/** Prova delle regole: java -cp <classi> io.github.nernerone.tremendouspreparator.FocusPolicyTest */
public final class FocusPolicyTest {
    static int failures = 0;

    static void check(boolean cond, String what) {
        if (!cond) {
            failures++;
            System.out.println("FALLITO: " + what);
        }
    }

    public static void main(String[] args) {
        String self = "io.github.nernerone.tremendouspreparator";
        Set<String> allowed = Set.of("org.wikipedia");
        Set<String> always = Set.of("com.google.android.dialer", "com.android.systemui", "com.google.android.deskclock");
        long now = 1_000_000, before = now + 60_000, after = now - 1;
        String insta = "com.instagram.android", launcher = "com.google.android.apps.nexuslauncher";
        // blocco a tempo attivo, al pit stop: bloccato
        check(FocusPolicy.shouldBlock(now, before, true, FocusPolicy.PIT, insta, false, false, self, allowed, always), "tempo + pit stop");
        // blocco a tempo attivo anche dopo «Termina»
        check(FocusPolicy.shouldBlock(now, before, false, FocusPolicy.IDLE, launcher, false, false, self, allowed, always), "tempo + terminata: anche la home");
        // tempo scaduto, si studia ancora: bloccato
        check(FocusPolicy.shouldBlock(now, after, true, FocusPolicy.STUDYING, insta, false, false, self, allowed, always), "scaduto + in pista");
        // tempo scaduto e sessione terminata: libero
        check(!FocusPolicy.shouldBlock(now, after, false, FocusPolicy.IDLE, insta, false, false, self, allowed, always), "scaduto + terminata");
        // mai avviata: studiare non blocca il telefono
        check(!FocusPolicy.shouldBlock(now, 0, false, FocusPolicy.STUDYING, insta, false, false, self, allowed, always), "mai avviata");
        // app permesse e sempre permesse
        check(!FocusPolicy.shouldBlock(now, before, true, FocusPolicy.STUDYING, "org.wikipedia", false, false, self, allowed, always), "app scelta");
        check(!FocusPolicy.shouldBlock(now, before, true, FocusPolicy.STUDYING, "com.google.android.dialer", false, false, self, allowed, always), "telefono");
        check(!FocusPolicy.shouldBlock(now, before, true, FocusPolicy.STUDYING, self, false, false, self, allowed, always), "se stessa");
        check(!FocusPolicy.shouldBlock(now, before, true, FocusPolicy.STUDYING, "com.google.android.deskclock", false, false, self, allowed, always), "sveglia");
        // in chiamata o a schermo bloccato: nessun intervento
        check(!FocusPolicy.shouldBlock(now, before, true, FocusPolicy.STUDYING, insta, false, true, self, allowed, always), "in chiamata");
        check(!FocusPolicy.shouldBlock(now, before, true, FocusPolicy.STUDYING, insta, true, false, self, allowed, always), "schermo bloccato");
        // nessuna app nota in primo piano: nessun intervento
        check(!FocusPolicy.shouldBlock(now, before, true, FocusPolicy.STUDYING, null, false, false, self, allowed, always), "sconosciuta");
        if (failures > 0) {
            System.out.println(failures + " prove fallite");
            System.exit(1);
        }
        System.out.println("FocusPolicy: tutte le prove superate");
    }
}
