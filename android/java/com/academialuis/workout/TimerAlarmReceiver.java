package com.academialuis.workout;

import android.app.Notification;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/**
 * Recebe o alarme do cronômetro quando o app está em segundo plano (ou foi encerrado)
 * e mostra a notificação de alerta. O fim do descanso apenas avisa: não conclui série
 * nem inicia outra (RN-06).
 */
public class TimerAlarmReceiver extends BroadcastReceiver {

    static PendingIntent openAppIntent(Context ctx) {
        Intent open = new Intent(ctx, MainActivity.class);
        open.setFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_SINGLE_TOP);
        return PendingIntent.getActivity(ctx, 0, open, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    @Override
    public void onReceive(Context ctx, Intent intent) {
        int id = intent.getIntExtra("id", 2001);
        String title = intent.getStringExtra("title");
        String text = intent.getStringExtra("text");
        boolean sound = intent.getBooleanExtra("sound", true);
        boolean vibrate = intent.getBooleanExtra("vibrate", true);
        MainActivity.createChannels(ctx);
        if (vibrate) MainActivity.vibrateCtx(ctx, 600);
        NotificationManager nm = (NotificationManager) ctx.getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm == null || !nm.areNotificationsEnabled()) return;
        try {
            Notification n = new Notification.Builder(ctx, sound ? MainActivity.CH_ALERTS_SOUND : MainActivity.CH_ALERTS_SILENT)
                    .setSmallIcon(R.drawable.ic_stat_timer)
                    .setContentTitle(title == null ? "Cronômetro" : title)
                    .setContentText(text == null ? "" : text)
                    .setCategory(Notification.CATEGORY_ALARM)
                    .setAutoCancel(true)
                    .setContentIntent(openAppIntent(ctx))
                    .build();
            nm.notify(id, n);
            nm.cancel(MainActivity.NOTIF_PROGRESS);
        } catch (SecurityException ignored) {
            // Permissão de notificação negada: o cronômetro continua funcionando na tela.
        }
    }
}
