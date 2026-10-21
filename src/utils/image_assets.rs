//! Bounded downloads for profile assets. Stored URLs are not arbitrary fetch targets.
use image::{DynamicImage, ImageReader, Limits};
use std::{io::Cursor, time::Duration};

fn permitted(url: &reqwest::Url, pocketbase: &str) -> bool {
    if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
        return false;
    }
    if let Ok(pb) = reqwest::Url::parse(pocketbase)
        && matches!(url.scheme(), "http" | "https")
        && url.origin() == pb.origin()
    {
        return url.path().starts_with("/api/files/");
    }
    url.scheme() == "https"
        && url.port().is_none()
        && matches!(
            url.host_str(),
            Some("cdn.discordapp.com" | "media.discordapp.net" | "mc-heads.net")
        )
}

pub async fn download(url: &str) -> Option<DynamicImage> {
    const MAX_BYTES: usize = 4 * 1024 * 1024;
    let url = reqwest::Url::parse(url).ok()?;
    if !permitted(&url, &crate::config::Config::global().auth.pb_url) {
        return None;
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .ok()?;
    let mut response = client.get(url).send().await.ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BYTES as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.ok()? {
        if bytes.len().checked_add(chunk.len())? > MAX_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(32 * 1024 * 1024);
    reader.limits(limits);
    reader.decode().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asset_urls_reject_untrusted_origins_credentials_and_internal_targets() {
        let pb = "http://127.0.0.1:8090";
        for url in [
            "http://127.0.0.1:8090/api/files/badges/a/icon.png",
            "https://cdn.discordapp.com/avatars/a.png",
            "https://mc-heads.net/avatar/a/200",
        ] {
            assert!(permitted(&reqwest::Url::parse(url).unwrap(), pb), "{url}");
        }
        for url in [
            "http://169.254.169.254/latest/meta-data/",
            "https://cdn.discordapp.com.attacker.example/a",
            "https://user:pass@cdn.discordapp.com/a",
            "http://127.0.0.1:8090/api/collections/users",
            "https://127.0.0.1/api/files/a",
            "https://cdn.discordapp.com:8443/a",
            "http://cdn.discordapp.com/a",
        ] {
            assert!(!permitted(&reqwest::Url::parse(url).unwrap(), pb), "{url}");
        }
    }
}
