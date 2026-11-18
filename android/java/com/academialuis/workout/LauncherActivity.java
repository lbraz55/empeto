package com.academialuis.workout;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.graphics.Color;
import android.graphics.Typeface;
import android.os.Build;
import android.os.Bundle;
import android.view.View;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;

import java.io.BufferedReader;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.InputStreamReader;

/**
 * Ponto de entrada do app. Abre a interface nativa (MainActivity) e, se a abertura anterior não
 * chegou a mostrar a interface (falha ou travamento), exibe um relatório de diagnóstico que o
 * usuário pode copiar/compartilhar. Não usa nenhuma permissão.
 */
public class LauncherActivity extends Activity {
    static final String MARKER = "startup.marker";
    static final String CRASH = "crash.txt";
    static final String TRACE = "startup.log";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        File marker = new File(getFilesDir(), MARKER);
        File crash = new File(getFilesDir(), CRASH);
        if (marker.exists() || crash.exists()) {
            showReport(marker, crash);
        } else {
            launch(marker);
        }
    }

    private void launch(File marker) {
        new File(getFilesDir(), TRACE).delete();
        MainActivity.crumb(this, "LauncherActivity: abrindo MainActivity");
        try {
            FileOutputStream os = new FileOutputStream(marker);
            os.write(("iniciando " + System.currentTimeMillis()).getBytes("UTF-8"));
            os.close();
        } catch (Exception ignored) {
        }
        Intent i = new Intent(this, MainActivity.class);
        i.addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION);
        startActivity(i);
        finish();
    }

    private static String readFile(File f) {
        StringBuilder sb = new StringBuilder();
        try (BufferedReader r = new BufferedReader(new InputStreamReader(new FileInputStream(f), "UTF-8"))) {
            String line;
            while ((line = r.readLine()) != null) sb.append(line).append('\n');
        } catch (Exception e) {
            sb.append("(não foi possível ler: ").append(e).append(")\n");
        }
        return sb.toString();
    }

    private static String readLogcat() {
        StringBuilder sb = new StringBuilder();
        try {
            // Um app só enxerga os registros do próprio app.
            Process p = Runtime.getRuntime().exec(new String[]{"logcat", "-d", "-v", "time", "-t", "400"});
            BufferedReader r = new BufferedReader(new InputStreamReader(p.getInputStream(), "UTF-8"));
            String line;
            while ((line = r.readLine()) != null) sb.append(line).append('\n');
            r.close();
        } catch (Exception e) {
            sb.append("(logcat indisponível: ").append(e).append(")\n");
        }
        return sb.toString();
    }

    private void showReport(final File marker, final File crash) {
        String version = "?";
        try {
            version = getPackageManager().getPackageInfo(getPackageName(), 0).versionName;
        } catch (Exception ignored) {
        }
        final String report = "Workout " + version + " — relatório de falha na abertura\n"
                + "Android " + Build.VERSION.RELEASE + " (API " + Build.VERSION.SDK_INT + "), "
                + Build.MANUFACTURER + " " + Build.MODEL + ", ABI " + Build.SUPPORTED_ABIS[0] + "\n\n"
                + "== Erro registrado ==\n" + (crash.exists() ? readFile(crash) : "(nenhum: o app pode ter travado)\n")
                + "\n== Etapas da última abertura ==\n"
                + (new File(getFilesDir(), TRACE).exists() ? readFile(new File(getFilesDir(), TRACE)) : "(sem registro)\n")
                + "\n== Log do app ==\n" + readLogcat();

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.BLACK);
        int pad = (int) (16 * getResources().getDisplayMetrics().density);
        root.setPadding(pad, pad * 2, pad, pad);

        TextView title = new TextView(this);
        title.setText("O Workout não abriu corretamente da última vez");
        title.setTextColor(Color.WHITE);
        title.setTextSize(20);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        root.addView(title);

        TextView hint = new TextView(this);
        hint.setText("Envie este relatório ao desenvolvedor (botão Compartilhar ou Copiar). Ele não contém seus treinos.");
        hint.setTextColor(Color.LTGRAY);
        hint.setPadding(0, pad / 2, 0, pad / 2);
        root.addView(hint);

        LinearLayout buttons = new LinearLayout(this);
        buttons.setOrientation(LinearLayout.HORIZONTAL);
        Button share = new Button(this);
        share.setText("Compartilhar");
        share.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                Intent s = new Intent(Intent.ACTION_SEND);
                s.setType("text/plain");
                s.putExtra(Intent.EXTRA_SUBJECT, "Relatório de falha — Workout");
                s.putExtra(Intent.EXTRA_TEXT, report);
                startActivity(Intent.createChooser(s, "Compartilhar relatório"));
            }
        });
        Button copy = new Button(this);
        copy.setText("Copiar");
        copy.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                ClipboardManager cm = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
                if (cm != null) cm.setPrimaryClip(ClipData.newPlainText("Relatório Workout", report));
            }
        });
        Button retry = new Button(this);
        retry.setText("Abrir app");
        retry.setOnClickListener(new View.OnClickListener() {
            public void onClick(View v) {
                crash.delete();
                marker.delete();
                launch(marker);
            }
        });
        buttons.addView(share);
        buttons.addView(copy);
        buttons.addView(retry);
        root.addView(buttons);

        ScrollView sv = new ScrollView(this);
        TextView body = new TextView(this);
        body.setText(report);
        body.setTextColor(Color.WHITE);
        body.setTextSize(11);
        body.setTypeface(Typeface.MONOSPACE);
        body.setTextIsSelectable(true);
        sv.addView(body);
        root.addView(sv, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f));
        setContentView(root);
    }
}
