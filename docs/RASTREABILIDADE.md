# Rastreabilidade: requisitos → implementação → testes

## Requisitos funcionais

| ID | Implementação | Teste |
|---|---|---|
| RF-01 criar/editar/duplicar/excluir ficha | `training/mod.rs` (`create_plan`, `update_plan`, `duplicate_plan`, `delete_plan`); tela Treinos/Editor | `ct01_*`, `duplicate_plan_copies_everything`, `ct12_*` |
| RF-02 dias da semana e/ou sequência | `plan_schedule.weekdays` + `sequence_order`; `next_sequence_day` | `sequence_plans_rotate`, `calendar_planned_by_weekday` |
| RF-03 nome, músculo, tipo, equipamento, séries, reps, carga-alvo | `exercise`, `plan_exercise`; diálogos de exercício | `ct01_*`, `exercise_library_crud` |
| RF-04 iniciar sessão da ficha | `start_session` | `ct03_*` |
| RF-05 registrar carga/reps por série | `record_set` | `ct03_*` |
| RF-06 observações e concluídos | `set_session_notes`, `set_session_exercise_notes`, `finish_session` | `finish_keeps_only_performed_sets_*` |
| RF-07 snapshot | `PlanSnapshot::build`, `plan_snapshot_json` | `ct02_*` |
| RF-08 substituição temporária/permanente | `substitute_exercise`, `undo_substitution` | `ct04_*`, `ct05_*` |
| RF-09 alternativas pesquisáveis sem equivalência | `list_exercises` (filtros), `suggested_alternatives`; aviso no diálogo | `alternatives_are_suggestions`, `search_ignores_accents` |
| RF-10 cronômetro em tela própria | `TimerScreen`, atalho no cabeçalho e na sessão | manual |
| RF-11 duração, pausar, retomar, reiniciar, ajustar | `TimerState` | `pause_does_not_advance`, `adjust_and_reset` |
| RF-12 alertas sonoros/vibração/pré-alerta | `Controller::timer_tick`, `TimerAlarmReceiver`, preferências | `pre_alert_fires_once` + manual |
| RF-13 calendário mês/semana | `calendar/`, `CalendarScreen` | `grid_has_42_days_*`, `calendar_planned_by_weekday` |
| RF-14 histórico e detalhes | `list_session_summaries`, `SessionDetailScreen` | `ct12_*` |
| RF-15 evolução por exercício | `exercise_evolution`, `EvolutionScreen` | `rf15_evolution_by_exercise` |
| RF-16 importação com prévia/mapeamento | `xlsx_import.rs`, `ImportScreen` | `ct08_*`, `preview_*`, `missing_exercise_column_is_error` |
| RF-17 exportação documentada | `xlsx_export.rs`, `ics.rs`, `FORMATO_ARQUIVOS.md` | `rf17_export_and_round_trip_import` |
| RF-18 backup/restauração | `backup.rs` | `ct10_*`, `restore_rejects_tampered_*` |
| RF-19 Home com cartões clicáveis | `Controller::refresh_home`, `HomeScreen` | manual |
| RF-20 preferências | `application/prefs.rs`, `SettingsScreen` | `prefs::tests::round_trip` |

## Regras de negócio

| ID | Garantia |
|---|---|
| RN-01 sessão é registro histórico | Gatilhos SQLite + checagens na aplicação (`completed_session_is_immutable_at_db_level`) |
| RN-02 planejado ≠ realizado | Só sessões `completed` entram no histórico; finalizar exige ≥ 1 série (`rn02_*`) |
| RN-03 substituição temporária × permanente | `substitute_exercise(…, permanent)` (`ct04`, `ct05`) |
| RN-04 original e substituto no histórico | `original_*` e `performed_*` em `session_exercise` |
| RN-05 realizado separado do planejado | `planned_*` × `actual_*` em `session_set` |
| RN-06 fim do descanso não conclui série | `TimerEvent` só afeta o cronômetro; ação opcional configurável (`finish_does_not_imply_anything_else`) |
| RN-07 cronômetro em segundo plano | Instante de término em `CLOCK_BOOTTIME`; alarme + recálculo ao voltar (`background_recompute_*`, `reboot_falls_back_*`) |
| RN-08 excluir ficha não apaga histórico | `ON DELETE SET NULL` + nomes copiados (`ct12_*`) |
| RN-09 importação não sobrescreve sem confirmação | Prévia sem gravação; `ConflictPolicy::CreateNew` padrão (`ct08_*`) |
| RN-10 avisar campos não reconhecidos/incompletos | `unrecognized_columns`, avisos por linha (`ct08_*`) |

## Não funcionais

| ID | Como |
|---|---|
| RNF-01 offline | Sem `INTERNET` no manifesto (`android_manifest_is_minimal`) |
| RNF-02 privacidade | Banco no diretório privado; `allowBackup=false`; sem coleta |
| RNF-03 integridade | Transações, FKs, CHECKs, validação antes de gravar |
| RNF-06 acessibilidade | Alvos ≥ 48 px, contraste alto, rótulos acessíveis, marcadores por forma (não só cor), texto grande |
| RNF-07 tempo confiável | Instante de término + relógio monotônico |
| RNF-08 portabilidade | XLSX documentado, backup restaurável |
| RNF-10 manutenibilidade | Módulos separados, testes automatizados, migrações versionadas |

## Checklist em aparelho real (CT-06, CT-11 e seção 4.5)

- [ ] Instalar o APK, abrir, criar ficha e sessão em modo avião (CT-11).
- [ ] Iniciar descanso de 30 s, bloquear a tela: alerta/vibração no fim; ao desbloquear, tempo correto (CT-06).
- [ ] Pausar, aguardar, retomar: tempo não avança na pausa (CT-07).
- [ ] Negar notificações: cronômetro segue funcionando na tela, mensagem de limitação aparece.
- [ ] Botão Voltar do Android fecha diálogos e volta telas.
- [ ] Rotacionar a tela durante a sessão.
- [ ] Importar o modelo XLSX baixado pelo próprio app; importar arquivo inválido (CT-08/09).
- [ ] Criar backup, desinstalar, reinstalar, restaurar (CT-10).
