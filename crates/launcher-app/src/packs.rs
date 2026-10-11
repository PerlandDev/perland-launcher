//! Pack commands other than playing: adding a pack, its status, its mods, manual downloads
//! and art (spec addendum §4.4). Runs and syncs live in `runs.rs`.
use launcher_core::{instances::Instance, sources::PackInput, ErrorCode, LauncherError, Result};

use crate::{app::AppCore, dto::PackAddRequest, events::Host, runner::Runner};

fn link_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackLinkInvalid,
        "That doesn't look like a pack link",
    )
    .with_fix("Pack links start with https:// and usually end in pack.toml")
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// A new instance from a pack link (https `pack.toml` or `modrinth:`) or a `.mrpack` file.
    /// Nothing but the art downloads until its first Play.
    pub async fn pack_add(&self, request: PackAddRequest) -> Result<Instance> {
        let input = match request {
            PackAddRequest::Link { url } => match PackInput::parse(&url)? {
                PackInput::MrpackFile { .. } => return Err(link_invalid()),
                input => input,
            },
            PackAddRequest::File { path } => {
                let is_mrpack = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("mrpack"));
                let is_file = tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_file());
                if !is_mrpack || !is_file {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    return Err(LauncherError::pack(
                        ErrorCode::PackInvalid,
                        format!("{name} isn't a .mrpack file"),
                    )
                    .with_param("file", name));
                }
                PackInput::MrpackFile { path }
            }
        };
        self.runner.add_pack(input).await
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{accounts::BuildKind, instances::SourceRef, sources::PackInput, ErrorCode};

    use crate::{app::tests::core, dto::PackAddRequest};

    #[tokio::test]
    async fn pack_add_takes_links_and_mrpack_files_only() {
        let (core, runner, _host, tmp) = core(BuildKind::Dev);
        let link = |url: &str| PackAddRequest::Link { url: url.into() };
        let url = "https://perlanddev.github.io/asalith-modpack/pack.toml";
        let added = core.pack_add(link(url)).await.unwrap();
        assert_eq!(added.source, SourceRef::Packwiz { url: url.into() });
        core.pack_add(link("modrinth:sky")).await.unwrap();

        let mrpack = tmp.path().join("Sky Islands.MRPACK");
        std::fs::write(&mrpack, b"zip").unwrap();
        core.pack_add(PackAddRequest::File {
            path: mrpack.clone(),
        })
        .await
        .unwrap();
        assert_eq!(
            *runner.added.lock().unwrap(),
            vec![
                PackInput::Packwiz { url: url.into() },
                PackInput::Modrinth {
                    project: "sky".into(),
                    version: None
                },
                PackInput::MrpackFile { path: mrpack },
            ]
        );

        // A link is never a file, and never plain http.
        for bad in [
            "C:/Downloads/pack.mrpack",
            "http://evil.example/pack.toml",
            "pack.toml",
        ] {
            assert_eq!(
                core.pack_add(link(bad)).await.unwrap_err().code,
                ErrorCode::PackLinkInvalid,
                "{bad}"
            );
        }
        // A file is only ever a regular .mrpack file.
        let exe = tmp.path().join("setup.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let folder = tmp.path().join("folder.mrpack");
        std::fs::create_dir_all(&folder).unwrap();
        for bad in [exe, folder, tmp.path().join("missing.mrpack")] {
            assert_eq!(
                core.pack_add(PackAddRequest::File { path: bad.clone() })
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::PackInvalid,
                "{}",
                bad.display()
            );
        }
        assert_eq!(
            runner.added.lock().unwrap().len(),
            3,
            "refused input never reaches the core"
        );
    }
}
