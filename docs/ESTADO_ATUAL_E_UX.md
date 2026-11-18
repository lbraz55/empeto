# Workout — Documentação técnica do estado atual e propostas de UX/UI

Versão do app: **0.1.4** · Schema do banco: **3** · Data: 18/11/2026
Plataforma: Android 8.0+ (API 26), arm64-v8a e x86_64 · Validado em aparelho: Xiaomi 23122PCD1G, Android 16.

---

## 1. Resumo

O Workout é um app Android pessoal e offline para planejar fichas de treino, executar sessões
registrando cargas e repetições reais, cronometrar descansos e acompanhar a evolução. Todos os dados
ficam no armazenamento privado do aparelho (SQLite). Não há internet, conta, anúncios nem nuvem.

| Indicador | Valor |
|---|---|
| Código Rust (lógica, banco, ponte Android) | ~8.100 linhas |
| Interface Slint (telas, componentes, diálogos) | ~3.000 linhas |
| Ponte Java (Activity, alarmes, diagnóstico) | ~690 linhas |
| Migrações SQL | 3 |
| Testes automatizados | 51 (unitários + integração), todos passando |
| Permissões Android | 2 (`POST_NOTIFICATIONS`, `VIBRATE`) |
| Tamanho do APK | ~19 MB (duas arquiteturas) |

---

## 2. Arquitetura

```
┌──────────────── Interface (Slint) ────────────────┐
│ app.slint · screens/* · dialogs · components/*    │
│ Bridge (global): ~150 propriedades e 96 callbacks │
└─────────────────────────┬─────────────────────────┘
                          │
                src/app (Controller)        navegação, transições, diálogos, formatação
                          │
  ┌─────────┬─────────┬───┴──────┬───────────────┬──────────────┬──────────────┐
training  history  calendar  import_export   application     platform
fichas,   sessões,  planejado XLSX, backup    preferências    Android (JNI) / desktop
sessões   evolução  × feito   (AES), ICS                      + rastros de diagnóstico
  └─────────┴─────────┴──────────┴───────────────┘
                          │
              domain (entidades, regras, cronômetro)
                          │
           persistence (SQLite, migrações, gatilhos de imutabilidade)
```

### 2.1 Stack

| Camada | Tecnologia | Observação |
|---|---|---|
| Linguagem | Rust (edition 2021) | toda a lógica e o acesso a dados |
| UI | Slint 1.19 | backend `android-activity` (NativeActivity), renderização Skia/OpenGL ES |
| Banco | SQLite (`rusqlite`, embutido) | WAL, chaves estrangeiras, CHECKs, gatilhos |
| Planilhas | `calamine` (leitura) · `rust_xlsxwriter` (escrita) | XLSX/ODS/XLS |
| Backup | `zip` com deflate + AES-256 opcional · `sha2` | manifesto + banco |
| Android | Java mínimo (Activity, BroadcastReceiver), JNI (`jni` 0.21) | sem Gradle |
| Build | `cargo-ndk`, `aapt2`, `d8`, `zipalign`, `apksigner` | `scripts/build-apk.sh` |
| CI | GitHub Actions (Node 24) | testes → APK → Release automática `v<versão>` |

### 2.2 Fluxo de inicialização no Android

1. `LauncherActivity` (Java) — se a abertura anterior falhou, mostra o **relatório de diagnóstico**
   (erro + etapas + log) com Compartilhar/Copiar; senão grava um marcador e abre a `MainActivity`.
2. `MainActivity` (NativeActivity) carrega `libworkout.so`; registra cada etapa em `files/startup.log`.
3. `android_main` (Rust) instala captura de *panic* e de sinais nativos, inicia o Slint e o controlador.
4. Ao exibir a interface, o marcador é removido. Qualquer falha antes disso aparece na próxima abertura.

### 2.3 Banco de dados (schema 3)

| Tabela | Papel |
|---|---|
| `exercise` | biblioteca (nome, músculo, tipo, equipamento, padrão de movimento, arquivado) |
| `exercise_alternative` | alternativas: semente automática (mesmo grupo e padrão) + cadastradas pelo usuário |
| `workout_plan` / `plan_schedule` / `plan_exercise` | ficha, dias (dias da semana em bitmask ou ordem na sequência), exercícios com alvos |
| `workout_session` | sessão + **snapshot JSON** da ficha |
| `session_exercise` | exercício original × realizado, tipo de substituição |
| `session_set` | planejado × realizado por série, horário, **descanso cumprido** (`rest_taken_s`, novo) |
| `app_preference` | preferências e estado do cronômetro |

Regras garantidas pelo próprio banco: sessão concluída é imutável (gatilhos), uma sessão em
andamento por vez (índice único), excluir ficha não apaga histórico (`ON DELETE SET NULL`). Antes de
cada migração de um banco existente é feita uma cópia `workout.pre-migration-vN.db`.

### 2.4 Cronômetro

Instante de término em relógio monotônico com suspensão (`CLOCK_BOOTTIME`) e relógio de parede
(fallback após reinício, detectado pelo `boot_id`). Em segundo plano: `AlarmManager.setAndAllowWhileIdle`
+ notificação com contagem regressiva; ao voltar, alarmes cancelados e tempo recalculado. Novo: o
descanso fica **ligado à série** registrada e o tempo efetivamente cumprido (sem pausas) é gravado
quando o descanso termina, é reiniciado ou a próxima série é registrada.

---

## 3. Funcionalidades — situação atual

### 3.1 Requisitos funcionais

| ID | Funcionalidade | Situação |
|---|---|---|
| RF-01 | Criar, editar, duplicar, excluir fichas | ✅ |
| RF-02 | Dias da semana e/ou sequência A/B/C | ✅ |
| RF-03 | Exercício com músculo, tipo, equipamento, séries, reps, carga-alvo, descanso | ✅ |
| RF-04/05 | Iniciar sessão e registrar carga/reps por série | ✅ |
| RF-06 | Observações (exercício e sessão), concluídos | ✅ |
| RF-07 | Snapshot para histórico fiel | ✅ |
| RF-08 | Substituição só na sessão ou na ficha | ✅ |
| RF-09 | Alternativas pesquisáveis + **cadastro manual** (novo, nos dois sentidos) | ✅ |
| RF-10/11/12 | Cronômetro em tela própria, controles, alertas, pré-alerta, vibração | ✅ |
| RF-13 | Calendário mês/semana + **"Adicionar à agenda" do aparelho** (novo, sem permissão) | ✅ |
| RF-14 | Histórico com detalhe; mostra **descanso cumprido** (novo) ou intervalo entre séries | ✅ |
| RF-15 | Evolução por exercício (carga máx., volume, 1RM estimada, reps) com gráfico | ✅ |
| RF-16 | Importação XLSX com prévia, mapeamento, avisos; **todas as abas de uma vez** (novo) | ✅ |
| RF-17 | Exportação XLSX (fichas/histórico, com coluna de descanso realizado) e ICS | ✅ |
| RF-18 | Backup/restauração validados; **senha opcional AES-256** (novo) | ✅ |
| RF-19 | Home com cartões clicáveis | ✅ |
| RF-20 | Preferências por categoria; **diagnóstico técnico** em Sobre (novo) | ✅ |

### 3.2 Novidades da versão 0.1.4

- **Transições de tela**: as telas entram deslizando 36 px e esmaecendo (220 ms, *ease-out*). A direção
  segue a navegação: avançar entra pela direita, voltar pela esquerda, e entre abas segue a ordem da
  barra inferior (Calendário → Treinos → Home → Histórico). Diálogos sobem e esmaecem (180 ms);
  avisos (toast) sobem suavemente.
- **Alternativas manuais** no diálogo do exercício (Treinos › Exercícios › editar).
- **Descanso real** por série no histórico e na exportação.
- **Senha no backup** (opcional; o banco dentro do `.zip` é criptografado; senha errada é recusada sem
  alterar dados).
- **Importar todas as abas** de uma planilha, juntando fichas e dias de mesmo nome.
- **Adicionar à agenda** do aparelho a partir do calendário interno (Intent `ACTION_INSERT`).
- **Diagnóstico técnico** em Configurações › Sobre.

### 3.3 O que ainda falta

| Item | Motivo / proposta |
|---|---|
| Importar `.ics` | baixa prioridade; agenda do aparelho já recebe eventos via "Agenda" |
| Rearmar alarme após reiniciar o aparelho | exigiria `RECEIVE_BOOT_COMPLETED`, fora da matriz de permissões |
| armeabi-v7a (32 bits) | Skia sem binário pronto; exige compilação própria |
| APK assinado com chave de release | criar keystore e configurar *secrets* do GitHub |
| Testes automatizados de UI / instrumentados | hoje só lógica e banco; ver seção 5 |
| Licença do Slint | confirmar antes de distribuir além do uso pessoal |

---

## 4. Qualidade, build e operação

- `cargo test`: 51 testes (CT-01 a CT-12 do documento, imutabilidade, migrações, backup com senha,
  importação de várias abas, alternativas, descanso real, manifesto mínimo).
- `scripts/build-apk.sh` **falha** se a biblioteca nativa tiver símbolos C++ não resolvidos (causa
  da falha de abertura da 0.1.0–0.1.2, corrigida fixando o NDK 30).
- CI: cada push gera APK e atualiza a Release `v<versão do Cargo.toml>`.
- Diagnóstico em produção: relatório automático na abertura seguinte a uma falha.

---

## 5. Propostas de UX/UI

Priorizadas por impacto no uso real (na academia, com atenção dividida e uma mão livre).

### 5.1 Alta prioridade (execução do treino)

1. **Modo "foco" na sessão** — uma série por vez em tela cheia: carga e reps grandes, botão ✓ ocupando
   a base da tela (zona do polegar), deslizar para a próxima série/exercício. Reduz toques e leitura.
2. **Cronômetro flutuante na sessão** — um anel de progresso no rodapé da tela da sessão (não só o chip
   do cabeçalho), tocável para abrir o cronômetro completo; contagem visível sem trocar de tela.
3. **Registrar com um toque o planejado** — gesto "✓ longo" ou botão "Repetir última" que copia carga/reps
   da série anterior. Hoje o pré-preenchimento já ajuda; o gesto elimina a digitação.
4. **Teclado numérico próprio** — teclado grande do app (0–9, vírgula, ±incremento) em vez do teclado
   do sistema, que cobre metade da tela e muda conforme o fabricante.
5. **Feedback háptico** em ações-chave (registrar série, fim do descanso, desfazer).
6. **Desfazer em toast** — após registrar/excluir, um toast com "Desfazer" por 5 s, além do botão atual.

### 5.2 Média prioridade (planejamento e consulta)

7. **Reordenar por arrastar** (dias e exercícios) no lugar das setas ↑↓, com alça visível.
8. **Indicador de progresso da sessão** — barra segmentada por exercício no topo (cheio/parcial/vazio)
   que também serve de navegação direta entre exercícios.
9. **Recordes pessoais (PR)** — selo "Novo recorde" ao registrar uma série acima do melhor histórico;
   cartão "Recordes recentes" na Home.
10. **Gráfico de evolução interativo** — tocar no ponto mostra data/valor; linha de tendência; eixo com
    proporção correta (hoje o gráfico preserva proporção quadrada).
11. **Heatmap de frequência** no calendário (intensidade pelo volume do dia) como visão opcional.
12. **Estados vazios ilustrados** com ação direta ("Criar primeira ficha", "Importar planilha").
13. **Busca global** (exercícios, fichas, sessões) a partir do cabeçalho.

### 5.3 Visual e acessibilidade

14. **Tema claro opcional** e acompanhamento do tema do sistema (o tema atual é escuro monocromático).
15. **Cor de destaque configurável** mantendo o monocromático como padrão (ex.: um único tom para
    estados ativos), sempre com redundância por forma/texto (RNF-06).
16. **Tipografia em três tamanhos** (normal, grande, extragrande) em vez do liga/desliga atual, e
    respeito à escala de fonte do Android.
17. **Animações respeitando a preferência do sistema** ("remover animações"): desativar transições
    quando o usuário pedir — o Slint expõe se animações estão habilitadas.
18. **Revisão com TalkBack** de todas as telas (rótulos já existem nos componentes principais).

### 5.4 Transições — próximos passos

- Transição compartilhada do cartão da ficha para o editor (o cartão "cresce" até virar a tela).
- Contador do cronômetro com animação de dígitos e anel que se esvazia.
- Animação de "check" ao registrar série (o botão vira ✓ com leve escala).
- Saída animada das telas (hoje só a entrada é animada, porque a tela anterior é removida ao trocar).

### 5.5 Funcionalidades complementares

19. **Modelos de fichas** prontos (ABC, Full body, Push/Pull/Legs) para começar rápido.
20. **Planejamento de progressão** — sugerir +incremento quando todas as séries atingirem o topo da
    faixa de repetições (apenas sugestão, sem prescrição).
21. **Backup automático local** periódico (ex.: semanal, últimas 4 cópias) na pasta privada, com
    lembrete para exportar uma cópia para fora do aparelho.
22. **Widget** de tela inicial com o treino do dia e atalho para iniciar.

---

## 6. Como reproduzir e testar

```bash
cargo test
```

```bash
cargo run --example seed_demo -- workout-data/workout.db
```

```bash
cargo run --bin workout-desktop
```

```bash
ANDROID_HOME=$HOME/Android/Sdk ./scripts/build-apk.sh
```

Checklist em aparelho: `docs/RASTREABILIDADE.md`.
