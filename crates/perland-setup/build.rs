//! Embeds the NSIS installer named by `PERLAND_SETUP_PAYLOAD` and, when building for Windows on
//! Windows, the exe's icon, version info and manifest.

use std::path::PathBuf;

const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3">
    <windowsSettings>
      <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
      <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
    </windowsSettings>
  </application>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
        processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>
"#;

fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_payload)");
    println!("cargo::rerun-if-env-changed=PERLAND_SETUP_PAYLOAD");
    println!("cargo::rerun-if-env-changed=PERLAND_SETUP_INSTALLED_BYTES");
    if let Some(path) = std::env::var_os("PERLAND_SETUP_PAYLOAD").filter(|p| !p.is_empty()) {
        let path = PathBuf::from(path);
        println!("cargo::rerun-if-changed={}", path.display());
        let out = PathBuf::from(std::env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"))
            .join("payload.bin");
        std::fs::copy(&path, &out)
            .unwrap_or_else(|e| panic!("PERLAND_SETUP_PAYLOAD={}: {e}", path.display()));
        println!("cargo::rustc-cfg=has_payload");
    }

    let for_windows = std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows");
    if for_windows && cfg!(windows) {
        println!("cargo::rerun-if-changed=../../src-tauri/icons/icon.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../src-tauri/icons/icon.ico")
            .set("ProductName", "Perland Launcher")
            .set("FileDescription", "Perland Launcher Setup")
            .set("CompanyName", "Perland")
            .set("OriginalFilename", "perland-setup.exe")
            .set("ProductVersion", env!("CARGO_PKG_VERSION"))
            .set_manifest(MANIFEST);
        res.compile().expect("embed the Windows resources");
    } else if for_windows {
        println!("cargo::warning=Not building on Windows: the exe gets no icon, version info or manifest.");
    }
}
