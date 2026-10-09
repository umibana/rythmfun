/// Stable keys shared by browser storage and native wallpaper files.
pub fn storage_key(orientation: Option<&str>) -> Result<&'static str, String> {
    match orientation {
        None => Ok("wallpaper"),
        Some("portrait") => Ok("wallpaper-portrait"),
        Some("landscape") => Ok("wallpaper-landscape"),
        Some(_) => Err("orientación de fondo inválida".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::storage_key;

    #[test]
    fn preserves_legacy_storage_and_separates_orientations() {
        assert_eq!(storage_key(None).unwrap(), "wallpaper");
        assert_eq!(storage_key(Some("portrait")).unwrap(), "wallpaper-portrait");
        assert_eq!(storage_key(Some("landscape")).unwrap(), "wallpaper-landscape");
    }

    #[test]
    fn rejects_unknown_or_path_like_orientation() {
        assert!(storage_key(Some("diagonal")).is_err());
        assert!(storage_key(Some("../games")).is_err());
    }
}
