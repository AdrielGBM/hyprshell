//! That the settings catalogue resolves, and that switching the locale changes what it answers.

#[cfg(test)]
mod tests {
    #[test]
    fn catalog_translates_and_switches() {
        telar::set_locale("en");
        assert_eq!(telar::t!("settings.title"), "Settings");
        assert_eq!(telar::t!("common.on"), "On");
        telar::set_locale("es");
        assert_eq!(telar::t!("settings.title"), "Ajustes");
        assert_eq!(telar::t!("common.on"), "Sí");
    }
}
