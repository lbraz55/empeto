//! WORKOUT — organizador pessoal de treinos, offline-first.
//!
//! Arquitetura (seção 3.2 da documentação):
//! - `domain`        entidades, regras e cálculos (inclui o cronômetro)
//! - `persistence`   SQLite, migrações versionadas, preferências
//! - `training`      fichas, exercícios, sessões em andamento
//! - `history`       sessões concluídas e evolução
//! - `calendar`      datas, planejado × realizado
//! - `import_export` XLSX, backup, ICS
//! - `application`   casos de uso/preferências
//! - `platform`      integração Android (notificações, vibração, alarmes, seletor de arquivos)
//! - `app`           interface Slint

#[macro_use]
pub mod i18n;
pub mod application;
pub mod calendar;
pub mod domain;
pub mod exercise_media;
pub mod history;
pub mod import_export;
pub mod persistence;
pub mod training;
pub mod updater;

pub mod ui {
    slint::include_modules!();
}
pub mod app;
pub mod platform;

/// Ponto de entrada no Android (android-activity / NativeActivity).
#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: slint::android::AndroidApp) {
    use slint::android::android_activity::{MainEvent, PollEvent};
    android_logger::init_once(
        android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("workout"),
    );
    let files = app.internal_data_path().unwrap_or_else(|| std::path::PathBuf::from("/data/local/tmp"));
    let crash_file = files.join("crash.txt");
    platform::crumbs::init(files.clone());
    platform::crumbs::crumb("android_main iniciou");
    platform::crumbs::install_signal_handlers();
    {
        let crash_file = crash_file.clone();
        std::panic::set_hook(Box::new(move |info| {
            let bt = std::backtrace::Backtrace::force_capture();
            let msg = format!("panic: {info}\n\n{bt}");
            log::error!("{msg}");
            let _ = std::fs::write(&crash_file, msg);
        }));
    }
    let fail = |msg: String| {
        log::error!("{msg}");
        let _ = std::fs::write(&crash_file, msg);
    };
    log::info!("android_main: iniciando (dados em {})", files.display());
    let platform = platform::android::AndroidPlatform::new(app.clone());
    platform::crumbs::crumb("iniciando backend Slint");
    if let Err(e) = slint::android::init_with_event_listener(app, |event| match event {
        PollEvent::Main(MainEvent::Pause) => {
            platform::crumbs::crumb("evento: Pause");
            app::lifecycle(false)
        }
        PollEvent::Main(MainEvent::Resume { .. }) => {
            platform::crumbs::crumb("evento: Resume");
            app::lifecycle(true)
        }
        PollEvent::Main(MainEvent::InitWindow { .. }) => platform::crumbs::crumb("evento: InitWindow (superfície criada)"),
        PollEvent::Main(MainEvent::TerminateWindow { .. }) => platform::crumbs::crumb("evento: TerminateWindow"),
        PollEvent::Main(MainEvent::Start) => platform::crumbs::crumb("evento: Start"),
        PollEvent::Main(MainEvent::Destroy) => platform::crumbs::crumb("evento: Destroy"),
        _ => {}
    }) {
        fail(format!("falha ao iniciar Slint: {e}"));
        return;
    }
    platform::crumbs::crumb("backend Slint pronto; abrindo app");
    if let Err(e) = app::run(Box::new(platform)) {
        fail(format!("erro na interface: {e}"));
    }
}
