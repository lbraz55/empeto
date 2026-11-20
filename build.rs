fn main() {
    // Traduções (resources/i18n/<idioma>/LC_MESSAGES/workout.po) embutidas no binário; o texto-fonte
    // é português. Sem contexto padrão: o mesmo msgid serve à interface Slint e ao Rust.
    let config = slint_build::CompilerConfiguration::new()
        .with_style("cosmic-dark".into())
        .with_bundled_translations("resources/i18n")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    slint_build::compile_with_config("ui/app.slint", config).expect("falha ao compilar a interface Slint");
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=resources/i18n");
}
