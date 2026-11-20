package com.academialuis.workout;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageInstaller;

/**
 * Resultado da sessão do PackageInstaller (atualização do EMPETO, documento de evolução 7.2).
 * Quando o Android pede a confirmação do usuário, abre a tela do sistema; nunca instala em silêncio.
 */
public class UpdateInstallReceiver extends BroadcastReceiver {
    static volatile String lastStatus = null;

    @Override
    public void onReceive(Context ctx, Intent intent) {
        int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            Intent confirm = (Intent) intent.getParcelableExtra(Intent.EXTRA_INTENT);
            if (confirm != null) {
                confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                try {
                    ctx.startActivity(confirm);
                } catch (Exception e) {
                    lastStatus = "error:confirm";
                }
            }
            return;
        }
        if (status == PackageInstaller.STATUS_SUCCESS) {
            lastStatus = "ok";
        } else if (status == PackageInstaller.STATUS_FAILURE_ABORTED) {
            lastStatus = "cancelled";
        } else {
            String msg = intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE);
            lastStatus = "error:install:" + (msg == null ? status : msg);
        }
    }
}
