package com.academialuis.workout;

import android.Manifest;
import android.app.Activity;
import android.app.AlarmManager;
import android.app.NativeActivity;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.media.AudioManager;
import android.media.ToneGenerator;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.provider.OpenableColumns;
import android.provider.Settings;
import android.view.WindowManager;

import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Activity nativa (Slint/Rust) + ponte mínima para APIs Android (camada AndroidAdapter).
 *
 * Todas as funções públicas são chamadas pelo Rust via JNI (src/platform/android.rs).
 * Nenhuma permissão além de POST_NOTIFICATIONS e VIBRATE é usada.
 */
public class MainActivity extends NativeActivity {
    static final String CH_ALERTS_SOUND = "timer_alerts_sound";
    static final String CH_ALERTS_SILENT = "timer_alerts_silent";
    static final String CH_PROGRESS = "timer_progress";
    static final int NOTIF_PROGRESS = 1001;
    static final int REQ_OPEN = 501;
    static final int REQ_CREATE = 502;
    static final int REQ_NOTIF_PERMISSION = 601;
    static final long MAX_IMPORT_BYTES = 520L * 1024 * 1024;

    private volatile String fileResult = null;
    private volatile String pendingExportPath = null;
    private volatile String pendingImportName = null;
    private volatile int permissionResult = -1;
    private ToneGenerator toneGenerator;
    private final Handler handler = new Handler(Looper.getMainLooper());

    /** Acrescenta uma linha ao rastro de inicialização (files/startup.log). */
    static void crumb(Context ctx, String msg) {
        try {
            FileOutputStream os = new FileOutputStream(new File(ctx.getFilesDir(), "startup.log"), true);
            String t = new java.text.SimpleDateFormat("HH:mm:ss.SSS", java.util.Locale.US).format(new java.util.Date());
            os.write((t + " [java] " + msg + "\n").getBytes("UTF-8"));
            os.getFD().sync();
            os.close();
        } catch (Exception ignored) {
        }
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        final Context app = getApplicationContext();
        final Thread.UncaughtExceptionHandler previous = Thread.getDefaultUncaughtExceptionHandler();
        Thread.setDefaultUncaughtExceptionHandler(new Thread.UncaughtExceptionHandler() {
            public void uncaughtException(Thread t, Throwable e) {
                try {
                    java.io.StringWriter sw = new java.io.StringWriter();
                    e.printStackTrace(new java.io.PrintWriter(sw));
                    FileOutputStream os = new FileOutputStream(new File(app.getFilesDir(), "crash.txt"), true);
                    os.write(("exceção Java na thread " + t.getName() + ":\n" + sw).getBytes("UTF-8"));
                    os.close();
                } catch (Exception ignored) {
                }
                if (previous != null) previous.uncaughtException(t, e);
            }
        });
        crumb(this, "MainActivity.onCreate: carregando biblioteca nativa");
        super.onCreate(savedInstanceState);
        crumb(this, "MainActivity.onCreate: biblioteca nativa iniciada");
        createChannels(this);
    }

    @Override
    protected void onStart() {
        crumb(this, "onStart");
        super.onStart();
    }

    @Override
    protected void onResume() {
        crumb(this, "onResume (início)");
        super.onResume();
        crumb(this, "onResume (fim)");
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        crumb(this, "onWindowFocusChanged " + hasFocus);
        super.onWindowFocusChanged(hasFocus);
    }

    @Override
    protected void onPause() {
        crumb(this, "onPause");
        super.onPause();
    }

    @Override
    protected void onDestroy() {
        if (toneGenerator != null) {
            toneGenerator.release();
            toneGenerator = null;
        }
        super.onDestroy();
    }

    static void createChannels(Context ctx) {
        NotificationManager nm = (NotificationManager) ctx.getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm == null) return;
        NotificationChannel sound = new NotificationChannel(CH_ALERTS_SOUND,
                ctx.getString(R.string.channel_alerts), NotificationManager.IMPORTANCE_HIGH);
        sound.setDescription(ctx.getString(R.string.channel_alerts_desc));
        sound.enableVibration(false); // vibração controlada pela preferência do app
        nm.createNotificationChannel(sound);
        NotificationChannel silent = new NotificationChannel(CH_ALERTS_SILENT,
                ctx.getString(R.string.channel_alerts) + " (sem som)", NotificationManager.IMPORTANCE_HIGH);
        silent.setDescription(ctx.getString(R.string.channel_alerts_desc));
        silent.setSound(null, null);
        silent.enableVibration(false);
        nm.createNotificationChannel(silent);
        NotificationChannel progress = new NotificationChannel(CH_PROGRESS,
                ctx.getString(R.string.channel_progress), NotificationManager.IMPORTANCE_LOW);
        progress.setDescription(ctx.getString(R.string.channel_progress_desc));
        progress.setSound(null, null);
        progress.enableVibration(false);
        nm.createNotificationChannel(progress);
    }

    // ------------------------------------------------------------------
    // Arquivos: Storage Access Framework (sem permissão de armazenamento)
    // ------------------------------------------------------------------

    /** Abre o seletor de documentos. Resultado: pollFileResult() -> "open|ok|<caminho local>|<nome>". */
    public void openDocument(final String mimeTypesCsv) {
        handler.post(new Runnable() { public void run() {
            Intent i = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType("*/*");
            String[] mimes = mimeTypesCsv.split(",");
            i.putExtra(Intent.EXTRA_MIME_TYPES, mimes);
            try {
                startActivityForResult(i, REQ_OPEN);
            } catch (Exception e) {
                fileResult = "open|error|Nenhum seletor de arquivos disponível.";
            }
        }});
    }

    /** Pede ao usuário onde salvar; copia srcPath (arquivo temporário privado) para o destino. */
    public void createDocument(final String suggestedName, final String mime, final String srcPath) {
        pendingExportPath = srcPath;
        handler.post(new Runnable() { public void run() {
            Intent i = new Intent(Intent.ACTION_CREATE_DOCUMENT);
            i.addCategory(Intent.CATEGORY_OPENABLE);
            i.setType(mime);
            i.putExtra(Intent.EXTRA_TITLE, suggestedName);
            try {
                startActivityForResult(i, REQ_CREATE);
            } catch (Exception e) {
                fileResult = "create|error|Nenhum seletor de arquivos disponível.";
            }
        }});
    }

    public String pollFileResult() {
        String r = fileResult;
        fileResult = null;
        return r;
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQ_OPEN && requestCode != REQ_CREATE) return;
        final String kind = requestCode == REQ_OPEN ? "open" : "create";
        if (resultCode != Activity.RESULT_OK || data == null || data.getData() == null) {
            if (requestCode == REQ_CREATE) deleteQuietly(pendingExportPath);
            fileResult = kind + "|cancel|";
            return;
        }
        final Uri uri = data.getData();
        new Thread(new Runnable() { public void run() {
            if (requestCode == REQ_OPEN) {
                fileResult = copyIn(uri);
            } else {
                fileResult = copyOut(uri);
            }
        }}).start();
    }

    private String displayName(Uri uri) {
        try (Cursor c = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                String n = c.getString(0);
                if (n != null) return n.replace('|', '_');
            }
        } catch (Exception ignored) {
        }
        return "arquivo";
    }

    private String copyIn(Uri uri) {
        String name = displayName(uri);
        File dir = new File(getFilesDir(), "tmp");
        dir.mkdirs();
        File out = new File(dir, "import-" + System.currentTimeMillis());
        long total = 0;
        try (InputStream in = getContentResolver().openInputStream(uri);
             OutputStream os = new FileOutputStream(out)) {
            if (in == null) return "open|error|Não foi possível ler o arquivo.";
            byte[] buf = new byte[64 * 1024];
            int n;
            while ((n = in.read(buf)) > 0) {
                total += n;
                if (total > MAX_IMPORT_BYTES) {
                    os.close();
                    out.delete();
                    return "open|error|Arquivo grande demais.";
                }
                os.write(buf, 0, n);
            }
        } catch (Exception e) {
            out.delete();
            return "open|error|Falha ao ler o arquivo: " + e.getMessage();
        }
        return "open|ok|" + out.getAbsolutePath() + "|" + name;
    }

    private String copyOut(Uri uri) {
        String src = pendingExportPath;
        pendingExportPath = null;
        if (src == null) return "create|error|Nada para salvar.";
        try (InputStream in = new FileInputStream(src);
             OutputStream os = getContentResolver().openOutputStream(uri, "wt")) {
            if (os == null) return "create|error|Não foi possível gravar no destino.";
            byte[] buf = new byte[64 * 1024];
            int n;
            while ((n = in.read(buf)) > 0) os.write(buf, 0, n);
        } catch (Exception e) {
            return "create|error|Falha ao salvar: " + e.getMessage();
        } finally {
            deleteQuietly(src);
        }
        return "create|ok||" + displayName(uri);
    }

    private static void deleteQuietly(String p) {
        if (p != null) {
            try {
                new File(p).delete();
            } catch (Exception ignored) {
            }
        }
    }

    // ------------------------------------------------------------------
    // Notificações e permissão (POST_NOTIFICATIONS somente Android 13+)
    // ------------------------------------------------------------------

    public boolean notificationsAllowed() {
        if (Build.VERSION.SDK_INT >= 33
                && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            return false;
        }
        NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
        return nm != null && nm.areNotificationsEnabled();
    }

    /** Solicita a permissão. Resultado: pollPermissionResult() -> 1 concedida, 0 negada, -1 pendente. */
    public void requestNotificationPermission() {
        if (Build.VERSION.SDK_INT < 33) {
            permissionResult = notificationsAllowed() ? 1 : 0;
            return;
        }
        handler.post(new Runnable() { public void run() { requestPermissions(new String[]{Manifest.permission.POST_NOTIFICATIONS}, REQ_NOTIF_PERMISSION); } });
    }

    public int pollPermissionResult() {
        int r = permissionResult;
        permissionResult = -1;
        return r;
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        if (requestCode == REQ_NOTIF_PERMISSION) {
            permissionResult = (grantResults.length > 0 && grantResults[0] == PackageManager.PERMISSION_GRANTED) ? 1 : 0;
        }
    }

    /** Atalho para as configurações de notificação do app (somente quando o usuário pede). */
    public void openNotificationSettings() {
        handler.post(new Runnable() { public void run() {
            try {
                Intent i = new Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS);
                i.putExtra(Settings.EXTRA_APP_PACKAGE, getPackageName());
                i.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                startActivity(i);
            } catch (Exception ignored) {
            }
        }});
    }

    // ------------------------------------------------------------------
    // Alarmes do cronômetro em segundo plano
    // ------------------------------------------------------------------

    private PendingIntent alarmIntent(int id, String title, String text, boolean sound, boolean vibrate) {
        Intent i = new Intent(this, TimerAlarmReceiver.class);
        i.setAction("com.academialuis.workout.TIMER_ALERT_" + id);
        i.putExtra("id", id);
        i.putExtra("title", title);
        i.putExtra("text", text);
        i.putExtra("sound", sound);
        i.putExtra("vibrate", vibrate);
        return PendingIntent.getBroadcast(this, id, i, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
    }

    /**
     * Agenda o alerta para o instante `atElapsedMs` (relógio SystemClock.elapsedRealtime, que inclui
     * o tempo de suspensão). Usa setAndAllowWhileIdle: não exige SCHEDULE_EXACT_ALARM; o sistema pode
     * atrasar alguns instantes em modo de economia (documentado ao usuário).
     */
    public void scheduleAlarm(int id, long atElapsedMs, String title, String text, boolean sound, boolean vibrate) {
        AlarmManager am = (AlarmManager) getSystemService(Context.ALARM_SERVICE);
        if (am == null) return;
        am.setAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, atElapsedMs,
                alarmIntent(id, title, text, sound, vibrate));
    }

    public void cancelAlarm(int id) {
        AlarmManager am = (AlarmManager) getSystemService(Context.ALARM_SERVICE);
        if (am != null) am.cancel(alarmIntent(id, "", "", false, false));
        NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm != null) nm.cancel(id);
    }

    public long elapsedRealtime() {
        return SystemClock.elapsedRealtime();
    }

    /** Notificação discreta com contagem regressiva enquanto o app está em segundo plano. */
    public void showCountdown(long endWallMs, String title, String text) {
        if (!notificationsAllowed()) return;
        NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm == null) return;
        Notification n = new Notification.Builder(this, CH_PROGRESS)
                .setSmallIcon(R.drawable.ic_stat_timer)
                .setContentTitle(title)
                .setContentText(text)
                .setWhen(endWallMs)
                .setShowWhen(true)
                .setUsesChronometer(true)
                .setChronometerCountDown(true)
                .setOngoing(true)
                .setOnlyAlertOnce(true)
                .setTimeoutAfter(Math.max(1000, endWallMs - System.currentTimeMillis()))
                .setContentIntent(TimerAlarmReceiver.openAppIntent(this))
                .build();
        nm.notify(NOTIF_PROGRESS, n);
    }

    public void cancelCountdown() {
        NotificationManager nm = (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
        if (nm != null) nm.cancel(NOTIF_PROGRESS);
    }

    // ------------------------------------------------------------------
    // Som e vibração em primeiro plano
    // ------------------------------------------------------------------

    static int toneFor(int tone) {
        switch (tone) {
            case 1: return ToneGenerator.TONE_PROP_BEEP2;
            case 2: return ToneGenerator.TONE_CDMA_ALERT_CALL_GUARD;
            case 3: return ToneGenerator.TONE_PROP_ACK;
            default: return ToneGenerator.TONE_PROP_BEEP;
        }
    }

    /** Toca um som curto no fluxo de notificação: respeita o volume do sistema. */
    public void playTone(int tone, int durationMs) {
        try {
            if (toneGenerator == null) {
                toneGenerator = new ToneGenerator(AudioManager.STREAM_NOTIFICATION, 90);
            }
            toneGenerator.startTone(toneFor(tone), durationMs);
        } catch (Exception ignored) {
        }
    }

    /** Atualiza o widget depois que o app regravou files/widget.txt. */
    public void updateWidget() {
        EmpetoWidget.refreshAll(getApplicationContext());
    }

    /** Feedback tátil: 1 = série registrada, 2 = fim do descanso, 3 = desfazer. */
    public void haptic(int kind) {
        try {
            Vibrator v = (Vibrator) getSystemService(Context.VIBRATOR_SERVICE);
            if (v == null || !v.hasVibrator()) return;
            VibrationEffect e;
            if (Build.VERSION.SDK_INT >= 29) {
                e = VibrationEffect.createPredefined(kind == 2 ? VibrationEffect.EFFECT_HEAVY_CLICK
                        : kind == 3 ? VibrationEffect.EFFECT_DOUBLE_CLICK : VibrationEffect.EFFECT_CLICK);
            } else {
                e = kind == 3 ? VibrationEffect.createWaveform(new long[]{0, 20, 60, 20}, -1)
                        : VibrationEffect.createOneShot(kind == 2 ? 60 : 25, VibrationEffect.DEFAULT_AMPLITUDE);
            }
            v.vibrate(e);
        } catch (Exception ignored) {
        }
    }

    public void vibrate(long ms) {
        vibrateCtx(this, ms);
    }

    static void vibrateCtx(Context ctx, long ms) {
        try {
            Vibrator v = (Vibrator) ctx.getSystemService(Context.VIBRATOR_SERVICE);
            if (v != null && v.hasVibrator()) {
                v.vibrate(VibrationEffect.createOneShot(ms, VibrationEffect.DEFAULT_AMPLITUDE));
            }
        } catch (Exception ignored) {
        }
    }

    // ------------------------------------------------------------------
    // Diversos
    // ------------------------------------------------------------------

    public void keepScreenOn(final boolean on) {
        handler.post(new Runnable() { public void run() {
            if (on) getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            else getWindow().clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        }});
    }

    /** Abre a agenda do aparelho com o evento preenchido. Não exige READ/WRITE_CALENDAR. */
    public boolean addCalendarEvent(String title, String description, long beginMs, long endMs, boolean allDay) {
        final Intent i = new Intent(Intent.ACTION_INSERT)
                .setData(android.provider.CalendarContract.Events.CONTENT_URI)
                .putExtra(android.provider.CalendarContract.EXTRA_EVENT_BEGIN_TIME, beginMs)
                .putExtra(android.provider.CalendarContract.EXTRA_EVENT_END_TIME, endMs)
                .putExtra(android.provider.CalendarContract.EXTRA_EVENT_ALL_DAY, allDay)
                .putExtra(android.provider.CalendarContract.Events.TITLE, title)
                .putExtra(android.provider.CalendarContract.Events.DESCRIPTION, description);
        try {
            handler.post(new Runnable() { public void run() {
                try {
                    startActivity(i);
                } catch (Exception ignored) {
                }
            }});
            return true;
        } catch (Exception e) {
            return false;
        }
    }

    public boolean isNightMode() {
        int m = getResources().getConfiguration().uiMode & android.content.res.Configuration.UI_MODE_NIGHT_MASK;
        return m != android.content.res.Configuration.UI_MODE_NIGHT_NO;
    }

    public boolean animatorsEnabled() {
        return android.animation.ValueAnimator.areAnimatorsEnabled();
    }

    public void moveToBack() {
        handler.post(new Runnable() { public void run() { moveTaskToBack(true); } });
    }

    public String appVersion() {
        try {
            return getPackageManager().getPackageInfo(getPackageName(), 0).versionName;
        } catch (Exception e) {
            return "?";
        }
    }
}
