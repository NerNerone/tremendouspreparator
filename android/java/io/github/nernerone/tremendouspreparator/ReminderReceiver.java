package io.github.nernerone.tremendouspreparator;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** Allarmi delle fasce, pulizia di mezzanotte, riavvio del telefono, aggiornamento dell'app. */
public class ReminderReceiver extends BroadcastReceiver {
    @Override
    public void onReceive(Context context, Intent intent) {
        String a = intent.getAction();
        if (Reminders.ACTION.equals(a)) {
            Reminders.show(context);
        } else if (Reminders.ACTION_CLEAR.equals(a)) {
            Reminders.clearGoals(context);
        } else {
            Reminders.showGoals(context); // riavvio o aggiornamento dell'app
            FocusService.afterBoot(context); // un blocco a tempo non ancora scaduto riparte
        }
        Reminders.schedule(context);
    }
}
