package io.github.nernerone.tremendouspreparator;

import java.util.Set;

/**
 * Regole della modalità concentrazione, senza dipendenze da Android (provate con
 * android/test/FocusPolicyTest.java).
 *
 * Il blocco vale fino a {@code until} (scelto all'avvio, non si annulla) e, dopo, finché la
 * sessione di studio è in corso: in pista o al pit stop. Libera il telefono solo «Termina»
 * (dopo la scadenza), come dice l'avviso: «… includila alla lista prima dell'avvio della
 * sessione o stoppa la sessione».
 */
final class FocusPolicy {
    private FocusPolicy() {}

    /** Il pit stop tiene il telefono bloccato come la pista (solo «Termina» lo libera). */
    static final boolean PIT_HOLDS = true;

    static final int IDLE = 0;
    static final int STUDYING = 1;
    static final int PIT = 2;

    /** Il blocco è attivo. {@code armed} = la concentrazione è stata avviata e la sessione non è finita. */
    static boolean enforcing(long now, long until, boolean armed, int session) {
        if (now < until) return true;
        if (!armed) return false;
        return session == STUDYING || (session == PIT && PIT_HOLDS);
    }

    /**
     * Va riportata l'app in primo piano: blocco attivo, schermo sbloccato, nessuna chiamata,
     * e l'app davanti non è né questa, né una scelta dall'utente, né una di quelle sempre
     * permesse (telefono, sveglia, interfaccia di sistema).
     */
    static boolean shouldBlock(long now, long until, boolean armed, int session, String foreground,
                               boolean keyguardLocked, boolean inCall, String self,
                               Set<String> allowed, Set<String> always) {
        if (!enforcing(now, until, armed, session)) return false;
        if (keyguardLocked || inCall) return false;
        if (foreground == null || foreground.isEmpty()) return false;
        if (foreground.equals(self)) return false;
        if (allowed.contains(foreground) || always.contains(foreground)) return false;
        return true;
    }
}
