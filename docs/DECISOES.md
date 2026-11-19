# Decisões de produto adotadas

A documentação v0.2 lista decisões pendentes (seção 7.1) com **recomendações provisórias**. Esta
implementação seguiu exatamente essas recomendações; todas podem ser revistas.

| Tema | Recomendação do documento | Implementado |
|---|---|---|
| Calendário | Calendário interno + exportação ICS; integração nativa depois | Calendário interno (mês/semana) + exportação `.ics` pelo seletor de arquivos. **Sem** `READ_CALENDAR`/`WRITE_CALENDAR`. |
| Cronômetro | Manual como base + automático opcional | Manual por padrão. Opção "Iniciar descanso automaticamente" ao registrar a série. Ação opcional ao fim do descanso: contagem de preparação com alerta de início de série. A série nunca é concluída automaticamente (RN-06). |
| Fichas | Suportar dias da semana e sequência A/B/C | Tipo de agenda por ficha. Dias da semana: cada dia pode ter vários dias da semana. Sequência: o próximo dia é calculado pela última sessão concluída. |
| Navegação | Engrenagem global + 4 destinos | Barra inferior: Calendário, Treinos, Home, Histórico. Engrenagem no cabeçalho de todas as telas. |
| Nome | WORKOUT provisório | Rótulo "Workout"; pacote `com.academialuis.workout`. |
| Unidades | kg, com unidade configurável | Cargas gravadas em **gramas inteiros** (sem ponto flutuante). Exibição em kg (padrão) ou lb. |
| Android mínimo | Definir após protótipo | **API 26 (Android 8.0)**: canais de notificação nativos, `VibrationEffect`, ícone adaptativo. targetSdk 35. |

## Cronômetro em segundo plano

- O tempo é calculado pelo instante de término em `CLOCK_BOOTTIME` (= `SystemClock.elapsedRealtime`),
  que continua contando com o aparelho suspenso e não muda com o relógio do sistema. Após
  reinicialização do aparelho (detectada pelo `boot_id` do kernel), usa o relógio de parede.
- Ao ir para segundo plano, o app agenda `AlarmManager.setAndAllowWhileIdle(ELAPSED_REALTIME_WAKEUP)`
  (não exige `SCHEDULE_EXACT_ALARM`) e mostra uma notificação discreta com contagem regressiva. O
  `TimerAlarmReceiver` vibra (se ativado) e mostra a notificação de alerta.
- Ao voltar, os alarmes são cancelados e o tempo é recalculado; alertas já emitidos não se repetem.
- Se a notificação for negada, o cronômetro segue correto na tela e o app explica a limitação.
- Reinício do aparelho: os alarmes do Android são perdidos (não usamos `RECEIVE_BOOT_COMPLETED`, fora
  da matriz de permissões); ao abrir o app, o estado é restaurado corretamente.

## Outras decisões

- **Uma sessão em andamento por vez** (índice único no banco).
- **Finalizar sessão** remove séries não registradas: o histórico guarda o que foi realizado; o
  snapshot guarda o que foi planejado.
- **Descartar sessão** apaga uma sessão em andamento (nunca uma concluída).
- **Histórico imutável no banco**: gatilhos SQLite impedem alterar/excluir sessões concluídas;
  apenas a desvinculação automática de fichas/exercícios excluídos é permitida.
- **Exercícios** do catálogo são arquivados, não apagados.
- **Importação** nunca sobrescreve sem escolha explícita: padrão cria cópia "(importada)"; substituir
  exige ligar a opção na prévia. Avisos exigem confirmação "Conferi os avisos".
- **Backup em arquivo** pode ter senha (AES-256 no banco dentro do zip); o arquivo fica onde o usuário escolher.
- **Backups locais** ficam na área privada do app, sem senha (só o app acessa), e são apagados se o app
  for desinstalado — por isso o lembrete de cópia externa. O backup automático roda ao abrir o app (não
  há tarefa em segundo plano, que exigiria `RECEIVE_BOOT_COMPLETED`/agendamentos).
- **Widget** atualiza quando o app atualiza a Home (`updatePeriodMillis=0`): sem serviço em segundo plano.
- **Gestos** são atalhos: nunca a única forma de fazer algo.
- **Sem backup em nuvem do Android** (`allowBackup=false`, regras de extração vazias).

## ABIs do APK

O APK inclui **arm64-v8a** (praticamente todos os celulares Android atuais) e **x86_64** (emuladores).
`armeabi-v7a` (32 bits) não é incluído por padrão: o backend Android do Slint usa Skia, que não tem
binários pré-compilados para essa arquitetura; é possível compilar com `ABIS="armeabi-v7a"` e
`ANDROID_NDK` definido (compilação longa do Skia).
