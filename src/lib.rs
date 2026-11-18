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

pub mod application;
pub mod calendar;
pub mod domain;
pub mod history;
pub mod import_export;
pub mod persistence;
pub mod training;

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
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    let platform = platform::android::AndroidPlatform::new(app.clone());
    if let Err(e) = slint::android::init_with_event_listener(app, |event| match event {
        PollEvent::Main(MainEvent::Pause) => app::lifecycle(false),
        PollEvent::Main(MainEvent::Resume { .. }) => app::lifecycle(true),
        _ => {}
    }) {
        log::error!("falha ao iniciar Slint: {e}");
        return;
    }
    if let Err(e) = app::run(Box::new(platform)) {
        log::error!("erro na interface: {e}");
    }
}
