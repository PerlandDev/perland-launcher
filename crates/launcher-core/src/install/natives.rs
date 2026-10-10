use std::{
    fs, io,
    path::{Path, PathBuf},
};

use super::NativeJar;
use crate::error::{ErrorCode, LauncherError, Result};

/// Records which jars a natives folder was built from.
const MARKER: &str = ".perland-natives";

/// Extract native libraries for versions that still ship them inside jars (1.18 and older).
///
/// The folder is shared by every instance of the version, so this is idempotent: if it was
/// already built from the same jars (same paths and sizes) it is left untouched, which matters
/// while a running game holds its DLLs open. Otherwise it is built in a sibling temp folder and
/// swapped in, so files from a previous version never linger and a half-built folder is never used.
pub fn extract_all(jars: &[NativeJar], dest: &Path) -> Result<()> {
    if jars.is_empty() {
        return Ok(());
    }
    let marker = marker_for(jars)?;
    if fs::read_to_string(dest.join(MARKER)).is_ok_and(|m| m == marker) {
        return Ok(());
    }
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    if tmp.exists() {
        fs::remove_dir_all(&tmp)?;
    }
    if let Err(e) = build(jars, &tmp, &marker) {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if dest.exists() {
        if let Err(e) = fs::remove_dir_all(dest) {
            let _ = fs::remove_dir_all(&tmp);
            return Err(LauncherError::game(
                ErrorCode::VersionInUse,
                "Another copy of this Minecraft version is running",
            )
            .with_fix("Close it and try again")
            .with_detail(e));
        }
    }
    fs::rename(&tmp, dest)?;
    Ok(())
}

/// One line per jar, sorted: its path (the file name encodes the version) and its size.
fn marker_for(jars: &[NativeJar]) -> Result<String> {
    let mut lines = jars
        .iter()
        .map(|n| {
            Ok(format!(
                "{}\t{}\n",
                n.jar.display(),
                fs::metadata(&n.jar)?.len()
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    lines.sort();
    Ok(lines.concat())
}

fn build(jars: &[NativeJar], dir: &Path, marker: &str) -> Result<()> {
    fs::create_dir_all(dir)?;
    for native in jars {
        extract_one(&native.jar, &native.exclude, dir)?;
    }
    fs::write(dir.join(MARKER), marker)?;
    Ok(())
}

fn extract_one(jar: &Path, exclude: &[String], dest: &Path) -> Result<()> {
    let bad = |e: zip::result::ZipError| {
        LauncherError::integrity(
            ErrorCode::ArchiveInvalid,
            format!("{} isn't a valid archive", jar.display()),
        )
        .with_param(
            "file",
            jar.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
        .with_fix("Repair the instance to download it again")
        .with_detail(e)
    };
    let mut archive = zip::ZipArchive::new(fs::File::open(jar)?).map_err(bad)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(bad)?;
        if entry.is_dir() {
            continue;
        }
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if exclude.iter().any(|ex| rel_str.starts_with(ex.as_str())) {
            continue;
        }
        let out = dest.join(&rel);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entry, &mut fs::File::create(&out)?)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_jar(path: &Path) {
        make_jar_with(path, &[]);
    }

    fn make_jar_with(path: &Path, extra: &[(&str, &str)]) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        let base = [
            ("META-INF/MANIFEST.MF", "m"),
            ("lwjgl64.dll", "dll"),
            ("sub/libx.so", "so"),
        ];
        for (name, body) in base.iter().chain(extra) {
            w.start_file(*name, opts).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn same_jars_are_not_extracted_again() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("lwjgl-platform-2.9.4-natives-windows.jar");
        make_jar(&jar);
        let dest = tmp.path().join("natives");
        let jars = [NativeJar {
            jar,
            exclude: vec!["META-INF/".into()],
        }];
        extract_all(&jars, &dest).unwrap();
        fs::write(dest.join("sentinel"), "kept").unwrap();
        extract_all(&jars, &dest).unwrap();
        assert_eq!(fs::read_to_string(dest.join("sentinel")).unwrap(), "kept");
        assert_eq!(fs::read_to_string(dest.join("lwjgl64.dll")).unwrap(), "dll");
    }

    #[test]
    fn changed_jar_is_extracted_again() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("lwjgl-platform-2.9.4-natives-windows.jar");
        make_jar(&jar);
        let dest = tmp.path().join("natives");
        let jars = [NativeJar {
            jar: jar.clone(),
            exclude: vec![],
        }];
        extract_all(&jars, &dest).unwrap();
        fs::write(dest.join("sentinel"), "stale").unwrap();
        make_jar_with(&jar, &[("extra.dll", "new")]);
        extract_all(&jars, &dest).unwrap();
        assert!(!dest.join("sentinel").exists());
        assert_eq!(fs::read_to_string(dest.join("extra.dll")).unwrap(), "new");
        let leftovers: Vec<_> = fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn extracts_respecting_excludes_and_cleans_old_files() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("natives.jar");
        make_jar(&jar);
        let dest = tmp.path().join("natives");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("stale.dll"), "old").unwrap();
        extract_all(
            &[NativeJar {
                jar,
                exclude: vec!["META-INF/".into()],
            }],
            &dest,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(dest.join("lwjgl64.dll")).unwrap(), "dll");
        assert!(dest.join("sub/libx.so").exists());
        assert!(!dest.join("META-INF").exists());
        assert!(!dest.join("stale.dll").exists());
    }

    #[test]
    fn corrupt_jar_is_integrity_error() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("bad.jar");
        fs::write(&jar, "not a zip").unwrap();
        let err = extract_all(
            &[NativeJar {
                jar,
                exclude: vec![],
            }],
            &tmp.path().join("n"),
        )
        .unwrap_err();
        assert_eq!(err.category, crate::ErrorCategory::Integrity);
    }
}
