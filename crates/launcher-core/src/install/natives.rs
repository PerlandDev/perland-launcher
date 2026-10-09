use std::{fs, io, path::Path};

use super::NativeJar;
use crate::error::{LauncherError, Result};

/// Extract native libraries for versions that still ship them inside jars (1.18 and older).
/// The folder is rebuilt from scratch so files from a previous version never linger.
pub fn extract_all(jars: &[NativeJar], dest: &Path) -> Result<()> {
    if jars.is_empty() {
        return Ok(());
    }
    if dest.exists() {
        fs::remove_dir_all(dest)?;
    }
    fs::create_dir_all(dest)?;
    for native in jars {
        extract_one(&native.jar, &native.exclude, dest)?;
    }
    Ok(())
}

fn extract_one(jar: &Path, exclude: &[String], dest: &Path) -> Result<()> {
    let bad = |e: zip::result::ZipError| {
        LauncherError::integrity(format!("{} isn't a valid archive", jar.display()))
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
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let opts = zip::write::SimpleFileOptions::default();
        for (name, body) in [
            ("META-INF/MANIFEST.MF", "m"),
            ("lwjgl64.dll", "dll"),
            ("sub/libx.so", "so"),
        ] {
            w.start_file(name, opts).unwrap();
            w.write_all(body.as_bytes()).unwrap();
        }
        w.finish().unwrap();
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
