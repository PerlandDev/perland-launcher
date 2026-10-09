#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
    MacOs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X64,
    X86,
    Arm64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

impl Platform {
    pub fn current() -> Self {
        let os = if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        };
        let arch = if cfg!(target_arch = "x86_64") {
            Arch::X64
        } else if cfg!(target_arch = "aarch64") {
            Arch::Arm64
        } else {
            Arch::X86
        };
        Self { os, arch }
    }

    /// The `os.name` value used in Mojang version JSON rules.
    pub fn mojang_os_name(&self) -> &'static str {
        match self.os {
            Os::Windows => "windows",
            Os::Linux => "linux",
            Os::MacOs => "osx",
        }
    }

    pub fn classpath_separator(&self) -> &'static str {
        match self.os {
            Os::Windows => ";",
            _ => ":",
        }
    }

    /// Key into Mojang's java-runtime `all.json`.
    pub fn java_runtime_key(&self) -> Option<&'static str> {
        match (self.os, self.arch) {
            (Os::Windows, Arch::X64) => Some("windows-x64"),
            (Os::Windows, Arch::X86) => Some("windows-x86"),
            (Os::Windows, Arch::Arm64) => Some("windows-arm64"),
            (Os::Linux, Arch::X64) => Some("linux"),
            (Os::Linux, Arch::X86) => Some("linux-i386"),
            (Os::MacOs, Arch::X64) => Some("mac-os"),
            (Os::MacOs, Arch::Arm64) => Some("mac-os-arm64"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: Platform = Platform {
        os: Os::Windows,
        arch: Arch::X64,
    };
    const LINUX: Platform = Platform {
        os: Os::Linux,
        arch: Arch::X64,
    };

    #[test]
    fn mojang_names_and_separators() {
        assert_eq!(WIN.mojang_os_name(), "windows");
        assert_eq!(LINUX.mojang_os_name(), "linux");
        assert_eq!(
            Platform {
                os: Os::MacOs,
                arch: Arch::Arm64
            }
            .mojang_os_name(),
            "osx"
        );
        assert_eq!(WIN.classpath_separator(), ";");
        assert_eq!(LINUX.classpath_separator(), ":");
    }

    #[test]
    fn java_runtime_keys_match_mojang_index() {
        assert_eq!(WIN.java_runtime_key(), Some("windows-x64"));
        assert_eq!(LINUX.java_runtime_key(), Some("linux"));
        assert_eq!(
            Platform {
                os: Os::Linux,
                arch: Arch::Arm64
            }
            .java_runtime_key(),
            None
        );
    }
}
