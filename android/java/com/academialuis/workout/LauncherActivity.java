package com.academialuis.workout;

import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;

import java.io.File;

/**
 * Ponto de entrada do app: abre sempre a interface (MainActivity). A tela de abertura com o logo é
 * desenhada pelo próprio app. Não há mais tela de "falha na abertura": se algum erro for registrado,
 * ele fica disponível em Configurações › Sobre › Diagnóstico técnico. Não usa nenhuma permissão.
 */
public class LauncherActivity extends Activity {
    static final String TRACE = "startup.log";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        // Restos de versões anteriores (marcador de abertura) não são mais usados.
        new File(getFilesDir(), "startup.marker").delete();
        new File(getFilesDir(), TRACE).delete();
        MainActivity.crumb(this, "LauncherActivity: abrindo MainActivity");
        Intent i = new Intent(this, MainActivity.class);
        i.addFlags(Intent.FLAG_ACTIVITY_NO_ANIMATION);
        startActivity(i);
        overridePendingTransition(0, 0);
        finish();
    }
}
