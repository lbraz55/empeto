# Guia de desenvolvimento e reutilização do código

## Pré-requisitos

| Para | Precisa de |
|---|---|
| Testes e versão desktop | Rust estável (`rustup`), e no Linux `libfontconfig1-dev` e `libxkbcommon-dev` |
| APK Android | Android SDK (platform 36, build-tools ≥ 35), **NDK r30+** (o Skia do Slint exige), JDK 17+, alvos Rust Android e `cargo-ndk` |
| Scripts auxiliares | Python 3 |

```bash
rustup target add aarch64-linux-android x86_64-linux-android
```

```bash
cargo install cargo-ndk
```

## Executar

Testes (unitários, integração, catálogos de tradução):

```bash
cargo test
```

Versão desktop (desenvolvimento da interface). `WORKOUT_DATA_DIR` escolhe a pasta dos dados e `WORKOUT_LANG`
simula o idioma do sistema (`pt_BR`, `en_US`, `es_AR`…):

```bash
cargo run --example seed_demo -- workout-data/workout.db
```

```bash
WORKOUT_LANG=en_US cargo run --bin workout-desktop
```

APK (sem `KEYSTORE`, usa uma chave local de depuração em `build/`):

```bash
ANDROID_HOME=$HOME/Android/Sdk ./scripts/build-apk.sh
```

Benchmark de desempenho (100/1.000/10.000 sessões; resultados em [DESEMPENHO.md](DESEMPENHO.md)):

```bash
cargo test --release --test bench -- --ignored --nocapture
```

## Organização

```
ui/                 interface Slint: app.slint (casca), bridge.slint (contrato com o Rust), themes/, components/, screens/
src/
  app/              controlador: liga a UI aos casos de uso (navegação, diálogos, formatação)
  domain/           entidades, validações, cronômetro, teclado, calculadoras (anilhas/aquecimento)
  persistence/      SQLite, migrações versionadas (migrations/*.sql), preferências
  training/         biblioteca, fichas, sessões, modelos, progressão, grupos (supersérie/circuito)
  history/          histórico, evolução, recordes, análises (comparação, relatório mensal, diário)
  calendar/         datas locais, grade mês/semana, planejado × realizado
  import_export/    XLSX, backup (.zip + SHA-256), ICS
  application/      preferências, busca, desfazer, gestos, atalhos, armazenamento/backups locais, widget, FAQ
  exercise_media/   ilustrações e links de vídeo
  updater/          verificação de atualizações (decisão; rede/instalação ficam no Java)
  i18n/             tr()/trf!() — mesmos catálogos .po da interface
  platform/         Android (JNI) e desktop
android/            manifesto, recursos e ponte Java (MainActivity, LauncherActivity, receptores, widget)
resources/i18n/     traduções en/es (texto-fonte em português)
assets/             marca e ilustrações (SVG)
migrations/         SQL versionado
scripts/            build-apk.sh, i18n.py, update-metadata.py, licenses.py, gen_exercise_art.py
tests/              integração, i18n e benchmark
docs/               arquitetura, decisões, manual, formatos, rastreabilidade, desempenho, licenças
```

A arquitetura e o motivo de cada camada estão em [ARQUITETURA.md](ARQUITETURA.md).

## Regras do projeto (resumo)

- **Offline-first**: nada essencial depende de rede. Dados só no aparelho.
- **Camadas**: regras de negócio em Rust; Slint só apresenta; Java só para APIs do Android.
- **Banco**: toda mudança de schema é uma nova migração em `migrations/` (nunca editar as antigas), com teste de
  compatibilidade. O histórico é imutável (gatilhos no SQLite).
- **Textos**: toda frase de interface passa por `@tr("…")` (Slint) ou `tr("…")`/`trf!("…", …)` (Rust).
  Depois de criar frases, rode `python3 scripts/i18n.py update`, traduza em `resources/i18n/{en,es}` e confira com
  `python3 scripts/i18n.py check` (o CI falha se faltar tradução).
- **Permissões**: o teste `android_manifest_is_minimal` impede permissões novas sem revisão.

## Como contribuir

1. Abra uma *issue* descrevendo o problema/ideia.
2. Crie um branch a partir do principal, faça mudanças pequenas e com testes.
3. Rode `cargo test` e `python3 scripts/i18n.py check`.
4. Atualize a documentação afetada e o `CHANGELOG.md`.
5. Abra um *pull request* explicando o que mudou e como foi testado (desktop e, se possível, aparelho real).

## Licenças

Veja [LICENCAS.md](LICENCAS.md) antes de reutilizar o código — a definição da licença do projeto está pendente
de confirmação do autor, e o Slint tem condições próprias.
