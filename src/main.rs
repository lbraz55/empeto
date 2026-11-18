//! Execução em desktop para desenvolvimento e testes da interface.
//! Dados ficam em ./workout-data (ou WORKOUT_DATA_DIR).

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    #[cfg(not(target_os = "android"))]
    {
        let platform = workout::platform::desktop::DesktopPlatform::new();
        if let Err(e) = workout::app::run(Box::new(platform)) {
            eprintln!("erro: {e}");
            std::process::exit(1);
        }
    }
}
