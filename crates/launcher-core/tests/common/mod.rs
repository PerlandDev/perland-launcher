//! Shared by the pack integration tests: a tiny packwiz pack served by wiremock, the sources
//! wired to that server, and a zip writer for `.mrpack` files.
#![allow(dead_code)]

use std::{io::Write, path::Path, time::Duration};

use launcher_core::{
    download::{
        cache::ObjectCache,
        hash::{sha1_hex, sha256_hex, sha512_hex},
        Downloader,
    },
    meta::MetaClient,
    sources::{SourceConfig, Sources},
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

pub fn sources(server: &MockServer, root: &Path) -> Sources {
    let http = launcher_core::http::client("test").unwrap();
    Sources {
        http: http.clone(),
        meta: MetaClient::new(http.clone(), root.join("meta")),
        downloader: Downloader::new(http).with_backoff(Duration::from_millis(1)),
        cache: ObjectCache::new(root.join("cache/objects")),
        config: SourceConfig {
            modrinth_api: format!("{}/v2", server.uri()),
            curseforge_cdn: server.uri(),
            curseforge_site: "https://www.curseforge.com".into(),
        },
    }
}

/// How many requests the server saw for `at`.
pub async fn hits(server: &MockServer, at: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == at)
        .count()
}

/// A packwiz pack under `/pack/`, with mod jars under `/files/`.
#[derive(Clone)]
pub struct PackBuilder {
    base: String,
    name: String,
    version: String,
    loader: Option<(String, String)>,
    index: String,
    hosted: Vec<(String, Vec<u8>)>,
    corrupt_index: bool,
}

impl PackBuilder {
    /// Minecraft 1.20.1 with Forge 47.4.22, like Asalith Fields.
    pub fn new(server: &MockServer, name: &str, version: &str) -> Self {
        Self {
            base: server.uri(),
            name: name.into(),
            version: version.into(),
            loader: Some(("forge".into(), "47.4.22".into())),
            index: "hash-format = \"sha256\"\n".into(),
            hosted: Vec::new(),
            corrupt_index: false,
        }
    }

    pub fn vanilla(mut self) -> Self {
        self.loader = None;
        self
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.into();
        self
    }

    pub fn with_loader(mut self, kind: &str, version: &str) -> Self {
        self.loader = Some((kind.into(), version.into()));
        self
    }

    fn entry(&mut self, file: &str, body: &[u8], metafile: bool, preserve: bool) {
        self.index += &format!(
            "\n[[files]]\nfile = \"{file}\"\nhash = \"{}\"\nmetafile = {metafile}\npreserve = {preserve}\n",
            sha256_hex(body)
        );
        self.hosted.push((format!("/pack/{file}"), body.to_vec()));
    }

    /// A plain file (config, script) served next to the pack.
    pub fn file(mut self, path: &str, body: &[u8], preserve: bool) -> Self {
        self.entry(path, body, false, preserve);
        self
    }

    /// A mod downloaded from a URL (Modrinth style, sha512). `optional` is `Some(default)`.
    pub fn url_mod(
        mut self,
        slug: &str,
        file_name: &str,
        body: &[u8],
        side: &str,
        optional: Option<bool>,
    ) -> Self {
        let mut meta = format!(
            "name = \"{slug}\"\nfilename = \"{file_name}\"\nside = \"{side}\"\n\n[download]\n\
             url = \"{}/files/{file_name}\"\nhash-format = \"sha512\"\nhash = \"{}\"\n",
            self.base,
            sha512_hex(body)
        );
        if let Some(default) = optional {
            meta += &format!(
                "\n[option]\noptional = true\ndefault = {default}\ndescription = \"Optional {slug}\"\n"
            );
        }
        self.hosted
            .push((format!("/files/{file_name}"), body.to_vec()));
        self.entry(
            &format!("mods/{slug}.pw.toml"),
            meta.as_bytes(),
            true,
            false,
        );
        self
    }

    /// A CurseForge mod (`metadata:curseforge`). `blocked` leaves it off the CDN, like a mod
    /// whose author blocks other launchers. Use file names that need no URL escaping.
    pub fn cf_mod(
        mut self,
        slug: &str,
        file_name: &str,
        file_id: u64,
        project_id: u64,
        body: &[u8],
        blocked: bool,
    ) -> Self {
        let meta = format!(
            "name = \"{slug}\"\nfilename = \"{file_name}\"\nside = \"both\"\n\n[download]\n\
             hash-format = \"sha1\"\nhash = \"{}\"\nmode = \"metadata:curseforge\"\n\n\
             [update]\n[update.curseforge]\nfile-id = {file_id}\nproject-id = {project_id}\n",
            sha1_hex(body)
        );
        if !blocked {
            self.hosted.push((
                format!("/files/{}/{}/{file_name}", file_id / 1000, file_id % 1000),
                body.to_vec(),
            ));
        }
        self.entry(
            &format!("mods/{slug}.pw.toml"),
            meta.as_bytes(),
            true,
            false,
        );
        self
    }

    /// Any extra file at `at` (an absolute path on the server, like `/pack/icon.png`).
    pub fn host(mut self, at: &str, body: &[u8]) -> Self {
        self.hosted.push((at.into(), body.to_vec()));
        self
    }

    pub fn sidecar(mut self, toml: &str) -> Self {
        self.hosted
            .push(("/pack/launcher.toml".into(), toml.as_bytes().to_vec()));
        self
    }

    /// `pack.toml` names a wrong hash for `index.toml`.
    pub fn corrupt_index(mut self) -> Self {
        self.corrupt_index = true;
        self
    }

    pub fn pack_toml(&self) -> String {
        let index_hash = if self.corrupt_index {
            sha256_hex(b"something else")
        } else {
            sha256_hex(self.index.as_bytes())
        };
        let mut text = format!(
            "name = \"{}\"\nversion = \"{}\"\npack-format = \"packwiz:1.1.0\"\n\n[index]\n\
             file = \"index.toml\"\nhash-format = \"sha256\"\nhash = \"{index_hash}\"\n\n\
             [versions]\nminecraft = \"1.20.1\"\n",
            self.name, self.version
        );
        if let Some((kind, version)) = &self.loader {
            text += &format!("{kind} = \"{version}\"\n");
        }
        text
    }

    /// Serve every file and return the `pack.toml` URL. To publish a new version, call
    /// `server.reset().await` and mount the new builder.
    pub async fn mount(&self, server: &MockServer) -> String {
        let mut all = self.hosted.clone();
        all.push(("/pack/index.toml".into(), self.index.clone().into_bytes()));
        all.push(("/pack/pack.toml".into(), self.pack_toml().into_bytes()));
        for (at, body) in all {
            Mock::given(method("GET"))
                .and(path(at))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(server)
                .await;
        }
        format!("{}/pack/pack.toml", self.base)
    }
}

pub fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, body) in entries {
        w.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(body).unwrap();
    }
    w.finish().unwrap();
}
