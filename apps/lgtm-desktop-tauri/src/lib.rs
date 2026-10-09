pub fn greeting() -> &'static str {
    "hello lgtm-desktop-tauri!"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

#[cfg(test)]
mod tests {
    use super::greeting;

    #[test]
    fn greeting_uses_the_template_name() {
        assert_eq!(greeting(), "hello lgtm-desktop-tauri!");
    }
}
