/// A maven coordinate `group:artifact:version[:classifier][@extension]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl MavenCoord {
    pub fn parse(name: &str) -> Option<Self> {
        let (coords, extension) = match name.split_once('@') {
            Some((c, e)) => (c, e.to_string()),
            None => (name, "jar".to_string()),
        };
        let parts: Vec<&str> = coords.split(':').collect();
        if !(3..=4).contains(&parts.len()) || parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        Some(Self {
            group: parts[0].to_string(),
            artifact: parts[1].to_string(),
            version: parts[2].to_string(),
            classifier: parts.get(3).map(|c| c.to_string()),
            extension,
        })
    }

    /// Repository-relative path, always with `/` separators.
    pub fn path(&self) -> String {
        let classifier = self
            .classifier
            .as_ref()
            .map(|c| format!("-{c}"))
            .unwrap_or_default();
        format!(
            "{}/{}/{}/{}-{}{}.{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            self.artifact,
            self.version,
            classifier,
            self.extension
        )
    }

    /// `group:artifact[:classifier]`: two libraries with the same identity are the same library at different versions.
    pub fn identity(&self) -> String {
        match &self.classifier {
            Some(c) => format!("{}:{}:{}", self.group, self.artifact, c),
            None => format!("{}:{}", self.group, self.artifact),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_builds_paths() {
        let c = MavenCoord::parse("org.ow2.asm:asm:9.8").unwrap();
        assert_eq!(c.path(), "org/ow2/asm/asm/9.8/asm-9.8.jar");
        assert_eq!(c.identity(), "org.ow2.asm:asm");
        let n = MavenCoord::parse("org.lwjgl:lwjgl:3.3.1:natives-windows").unwrap();
        assert_eq!(
            n.path(),
            "org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar"
        );
        assert_eq!(n.identity(), "org.lwjgl:lwjgl:natives-windows");
        let z =
            MavenCoord::parse("de.oceanlabs.mcp:mcp_config:1.20.1-20230612.114412@zip").unwrap();
        assert_eq!(z.path(), "de/oceanlabs/mcp/mcp_config/1.20.1-20230612.114412/mcp_config-1.20.1-20230612.114412.zip");
    }

    #[test]
    fn rejects_malformed() {
        assert!(MavenCoord::parse("just-a-name").is_none());
        assert!(MavenCoord::parse("a::1").is_none());
        assert!(MavenCoord::parse("a:b:c:d:e").is_none());
    }
}
