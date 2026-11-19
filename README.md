# EMPETO — organizador pessoal de treinos (Android, offline-first)

**TREINE. EVOLUA. REPITA.**

Aplicativo Android privado para organizar fichas de treino, acompanhar a execução na academia e
registrar a evolução ao longo do tempo. Funciona **sem internet**: todos os dados ficam no
armazenamento privado do aparelho (SQLite).

> O nome antigo de trabalho era **Workout**; o identificador do pacote Android
> (`com.academialuis.workout`) foi mantido para que atualizações instalem por cima sem perder dados.

**Stack:** Rust · [Slint](https://slint.dev) (UI nativa) · SQLite (`rusqlite`) · `calamine`/`rust_xlsxwriter` (XLSX) ·
Android SDK/NDK (sem Gradle).

## Funcionalidades

| Área | O que faz |
|---|---|
| **Home** | Até 4 **atalhos personalizáveis**; cartões clicáveis: sessão em andamento, treino de hoje, fichas favoritas, próximos treinos, última sessão, recordes recentes, sessões na semana e lembrete de cópia externa do backup. |
| **Execução** | Modo foco (uma série por vez), cronômetro flutuante, registrar com um toque (anterior/planejado), teclado numérico próprio, vibração ao registrar, desfazer em 5 s, barra de progresso. |
| **Evolução** | Recordes pessoais calculados do histórico, gráfico interativo com tendência, mapa de intensidade no calendário, sugestão de progressão (nunca altera a ficha sozinha), busca global. |
| **Treinos** | Criar, editar, duplicar, excluir e ativar/desativar fichas. Organização por **dias da semana** ou **sequência A/B/C**. Exercícios com séries, faixa de repetições, carga-alvo, descanso e observações. Biblioteca de exercícios pesquisável por grupo muscular, equipamento e padrão de movimento. |
| **Sessão** | Iniciar a partir de um dia da ficha; registrar carga e repetições **reais** por série (separadas do planejado); séries extras; observações; substituição de exercício **só nesta sessão** ou **também na ficha**; snapshot da ficha para histórico imutável. |
| **Cronômetro** | Tela própria; iniciar, pausar, retomar, reiniciar, ajustar ±; durações rápidas; pré-alerta; alertas sonoros (fim do descanso / início de série) e vibração configuráveis; modo manual ou automático; cálculo por instante de término (correto com tela bloqueada, app suspenso, mudança de horário e reinício do aparelho); alerta em segundo plano por notificação. |
| **Calendário** | Interno, visão mês/semana, planejado × realizado por data; exportação `.ics` sem permissões de agenda. |
| **Histórico** | Lista de sessões, detalhe (cargas, séries, repetições, intervalos, substituições, observações) e evolução por exercício (carga máxima, volume, 1RM estimada, repetições) com gráfico. |
| **Configurações** (engrenagem) | Sons e alertas · Cronômetro · Treinamento · Calendário · Interface (tema claro/escuro/sistema, tamanho do texto, densidade, animações e velocidade) · Atalhos da Home · Recursos opcionais · Dados e armazenamento · Backups · Sobre. |
| **Recursos opcionais** | Recordes, mapa de intensidade, sugestão de progressão, gestos, animações decorativas e lembrete de backup podem ser desligados — desligar nunca apaga dados. |
| **Dados** | Painel de espaço usado (banco, backups, temporários, total); importação XLSX com prévia e mapeamento; exportação XLSX; modelo de importação. |
| **Backups** | Backup automático semanal no aparelho (mantém os 4 últimos), backups manuais, lista com data e tamanho, restaurar/excluir (com confirmação)/salvar cópia; backup em arquivo `.zip` com senha opcional (AES-256), validado por SHA-256 e integridade. |
| **Widget** | Widget da tela inicial com o próximo treino (lê apenas um resumo gerado pelo app). |

## Permissões Android (menor privilégio)

Somente `POST_NOTIFICATIONS` (pedida no contexto, ao ativar o alerta em segundo plano) e `VIBRATE`.
Sem `INTERNET`, agenda, armazenamento amplo, alarmes exatos, localização, câmera etc. Arquivos são
escolhidos pelo seletor do sistema (`ACTION_OPEN_DOCUMENT`/`ACTION_CREATE_DOCUMENT`). Backup automático
do Android (nuvem) desativado. Um teste automatizado garante que o manifesto não ganhe permissões novas.

## Baixar o APK

- **GitHub Actions**: cada push gera o artefato `workout-apk` (aba *Actions* › execução › *Artifacts*).
- **Releases**: o workflow publica automaticamente a release `v<versão do Cargo.toml>` com o APK.
- O APK de cada versão também fica em [`dist/`](dist/) quando gerado localmente.

Instalação: copie o `.apk` para o celular e abra-o (permita "instalar apps desconhecidos" para o
gerenciador de arquivos). Requer Android 8.0 (API 26) ou superior.

## Compilar

### Testes (qualquer sistema)

```bash
cargo test
```

### Rodar no desktop (desenvolvimento da interface)

```bash
cargo run --example seed_demo -- workout-data/workout.db   # dados de exemplo (opcional)
cargo run --bin workout-desktop
```

### APK Android

Pré-requisitos: Android SDK (platform 36, build-tools ≥ 35), NDK r30+, JDK 17+, Rust com alvos Android e `cargo-ndk`.

```bash
rustup target add aarch64-linux-android x86_64-linux-android
```

```bash
cargo install cargo-ndk
```

```bash
ANDROID_HOME=$HOME/Android/Sdk ./scripts/build-apk.sh
```

Saída em `dist/`. Sem `KEYSTORE`, o APK é assinado com uma chave local de depuração. Para release:
`KEYSTORE=... KEYSTORE_PASSWORD=... KEY_ALIAS=... KEY_PASSWORD=... ./scripts/build-apk.sh`
(no GitHub, configure os *secrets* `KEYSTORE_BASE64`, `KEYSTORE_PASSWORD`, `KEY_ALIAS`, `KEY_PASSWORD`).
Guarde a keystore fora do repositório.

## Estrutura

```
Cargo.toml  build.rs
ui/                 interface Slint (app.slint, bridge.slint, theme.slint, components/, screens/)
src/
  domain/           entidades, regras, cronômetro
  persistence/      SQLite, migrações versionadas, preferências
  training/         fichas, exercícios, sessões
  history/          histórico e evolução
  calendar/         datas, planejado × realizado
  import_export/    XLSX, backup, ICS
  application/      preferências, busca, desfazer, gestos, atalhos, armazenamento/backups locais, widget
  platform/         Android (JNI) e desktop
  app/              controlador da interface
migrations/         SQL versionado
android/            AndroidManifest.xml, recursos e ponte Java (MainActivity, LauncherActivity, TimerAlarmReceiver, EmpetoWidget)
scripts/build-apk.sh
tests/              testes de integração (CT-01 … CT-12)
docs/               arquitetura, decisões, formatos de arquivo, manual do usuário, rastreabilidade
```

## Documentação

- [Estado atual e propostas de UX/UI](docs/ESTADO_ATUAL_E_UX.md)
- [Manual do usuário](docs/MANUAL_USUARIO.md)
- [Arquitetura e decisões técnicas](docs/ARQUITETURA.md)
- [Decisões de produto adotadas](docs/DECISOES.md)
- [Formato dos arquivos (XLSX, backup, ICS)](docs/FORMATO_ARQUIVOS.md)
- [Rastreabilidade de requisitos e testes](docs/RASTREABILIDADE.md)

## Aviso

O EMPETO é um organizador de treinos. Não é ferramenta de diagnóstico, prescrição ou recomendação
médica e não substitui orientação profissional. Alternativas de exercícios são sugestões
organizacionais, sem garantia de equivalência biomecânica.

## Licenças

O código usa bibliotecas open-source (MIT/Apache-2.0) e o Slint, disponível sob GPLv3 ou sob a
licença royalty-free do Slint. Confirme as condições de licença antes de qualquer distribuição além
do uso pessoal (seção 5.6 da documentação).
