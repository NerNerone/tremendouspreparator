package it.tremendouspreparator.app;

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
        }
        Reminders.schedule(context);
    }
}
