package com.academialuis.workout;

import android.app.PendingIntent;
import android.appwidget.AppWidgetManager;
import android.appwidget.AppWidgetProvider;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.widget.RemoteViews;

import java.io.File;
import java.io.FileInputStream;

/**
 * Widget da tela inicial (documento EMPETO, 9.4).
 *
 * Não acessa o banco: apenas exibe o resumo derivado que o app grava em files/widget.txt
 * (src/application/widget.rs). Sem permissões novas e sem segunda fonte de dados.
 * Tocar no widget abre o app.
 */
public class EmpetoWidget extends AppWidgetProvider {
    static final String FILE = "widget.txt";

    @Override
    public void onUpdate(Context ctx, AppWidgetManager mgr, int[] ids) {
        render(ctx, mgr, ids);
    }

    /** Chamado pelo app (MainActivity.updateWidget) depois de regravar o resumo. */
    static void refreshAll(Context ctx) {
        try {
            AppWidgetManager mgr = AppWidgetManager.getInstance(ctx);
            int[] ids = mgr.getAppWidgetIds(new ComponentName(ctx, EmpetoWidget.class));
            if (ids != null && ids.length > 0) render(ctx, mgr, ids);
        } catch (Exception ignored) {
        }
    }

    static void render(Context ctx, AppWidgetManager mgr, int[] ids) {
        String title = "EMPETO", line1 = "Abra o app para ver seu próximo treino.", line2 = "TREINE. EVOLUA. REPITA.";
        try {
            File f = new File(ctx.getFilesDir(), FILE);
            if (f.exists() && f.length() < 4096) {
                byte[] buf = new byte[(int) f.length()];
                FileInputStream in = new FileInputStream(f);
                int off = 0;
                while (off < buf.length) {
                    int n = in.read(buf, off, buf.length - off);
                    if (n < 0) break;
                    off += n;
                }
                in.close();
                for (String l : new String(buf, 0, off, "UTF-8").split("\n")) {
                    int i = l.indexOf('=');
                    if (i <= 0) continue;
                    String k = l.substring(0, i), v = l.substring(i + 1);
                    if (k.equals("title") && v.length() > 0) title = v;
                    else if (k.equals("line1")) line1 = v;
                    else if (k.equals("line2")) line2 = v;
                }
            }
        } catch (Exception ignored) {
        }
        Intent open = new Intent(ctx, LauncherActivity.class);
        open.setFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        PendingIntent pi = PendingIntent.getActivity(ctx, 0, open,
                PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        for (int id : ids) {
            RemoteViews v = new RemoteViews(ctx.getPackageName(), R.layout.widget_empeto);
            v.setTextViewText(R.id.widget_title, title);
            v.setTextViewText(R.id.widget_line1, line1);
            v.setTextViewText(R.id.widget_line2, line2);
            v.setOnClickPendingIntent(R.id.widget_root, pi);
            mgr.updateAppWidget(id, v);
        }
    }
}
