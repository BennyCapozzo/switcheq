//! Download di testo da AutoEq (solo `raw.githubusercontent.com/jaakkopasanen/AutoEq`).
//!
//! Usa `Windows.Web.Http.HttpClient` (proxy, certificati e TLS di sistema)
//! invece di una libreria HTTP: nessuna dipendenza in più e nessun costo in
//! memoria quando non si scarica niente.

const ALLOWED_PREFIX: &str = "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/";
/// Più grande dell'indice completo (circa 850 KB).
const MAX_BYTES: usize = 4_000_000;

pub fn allowed(url: &str) -> bool {
    url.starts_with(ALLOWED_PREFIX) && !url.contains("..") && url.is_ascii()
}

#[cfg(windows)]
pub fn get_text(url: &str) -> Result<String, String> {
    use windows::core::HSTRING;
    use windows::Foundation::Uri;
    use windows::Web::Http::HttpClient;

    if !allowed(url) {
        return Err(crate::t!("err.net.not_allowed"));
    }
    let err = |e: windows::core::Error| crate::t!("err.net.failed", error = e.message());
    let uri = Uri::CreateUri(&HSTRING::from(url)).map_err(err)?;
    let client = HttpClient::new().map_err(err)?;
    client
        .DefaultRequestHeaders()
        .and_then(|h| h.UserAgent())
        .and_then(|ua| ua.ParseAdd(&HSTRING::from(crate::brand::user_agent())))
        .map_err(err)?;
    let text = client.GetStringAsync(&uri).map_err(err)?.join().map_err(err)?.to_string();
    if text.len() > MAX_BYTES {
        return Err(crate::t!("err.net.too_large"));
    }
    Ok(text)
}

#[cfg(not(windows))]
pub fn get_text(_: &str) -> Result<String, String> {
    Err(crate::t!("err.net.windows_only"))
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn requests_identify_the_app() {
        assert_eq!(crate::brand::user_agent(), format!("SwitchEQ/{}", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn only_autoeq_raw_urls_are_allowed() {
        assert!(allowed(crate::profiles::INDEX_URL));
        for bad in [
            "http://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results/INDEX.md",
            "https://raw.githubusercontent.com/altro/repo/master/x",
            "https://evil.example/https://raw.githubusercontent.com/jaakkopasanen/AutoEq/",
            "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/../../x",
            "file:///C:/Windows/win.ini",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
    }
}
