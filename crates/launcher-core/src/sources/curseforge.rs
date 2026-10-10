//! CurseForge files referenced by packwiz packs (spec §3.4). Only the public CDN is used, and
//! there is no API key. Addresses checked against real files on 2026-10-10:
//! `edge.forgecdn.net` redirects here and answers range requests with 404, and a raw `+` in a
//! file name gets 403, so names are fully percent-encoded.
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};

/// Bytes a CDN file name keeps as they are; everything else is escaped.
const FILE_NAME: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub fn cdn_url(cdn: &str, file_id: u64, file_name: &str) -> String {
    format!(
        "{}/files/{}/{}/{}",
        cdn.trim_end_matches('/'),
        file_id / 1000,
        file_id % 1000,
        utf8_percent_encode(file_name, FILE_NAME)
    )
}

/// The project's page, where the player can download a file whose author blocks launchers.
pub fn page_url(site: &str, project_id: u64) -> String {
    format!("{}/projects/{project_id}", site.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdn_urls_split_the_id_and_escape_the_name() {
        let cdn = "https://mediafilez.forgecdn.net";
        assert_eq!(
            cdn_url(cdn, 8977190, "music_disc_maker-3.0.5+forge-1.20.1.jar"),
            "https://mediafilez.forgecdn.net/files/8977/190/music_disc_maker-3.0.5%2Bforge-1.20.1.jar"
        );
        assert_eq!(
            cdn_url(cdn, 5512063, "a b.jar"),
            "https://mediafilez.forgecdn.net/files/5512/63/a%20b.jar"
        );
        assert_eq!(
            page_url("https://www.curseforge.com", 825733),
            "https://www.curseforge.com/projects/825733"
        );
    }
}
