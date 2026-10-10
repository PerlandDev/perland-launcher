use std::collections::HashSet;

use super::mojang::{ArgValue, Library, Rule, RuleAction};
use crate::platform::{Arch, Platform};

/// What the rules are evaluated against: this computer, plus enabled launcher features.
#[derive(Debug, Clone)]
pub struct RuleContext {
    pub platform: Platform,
    pub features: HashSet<String>,
}

impl RuleContext {
    pub fn new(platform: Platform) -> Self {
        Self {
            platform,
            features: HashSet::new(),
        }
    }

    pub fn with_feature(mut self, name: &str) -> Self {
        self.features.insert(name.to_string());
        self
    }
}

pub fn rules_allow(rules: Option<&[Rule]>, ctx: &RuleContext) -> bool {
    let Some(rules) = rules else { return true };
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule, ctx) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

fn rule_matches(rule: &Rule, ctx: &RuleContext) -> bool {
    if let Some(os) = &rule.os {
        if os
            .name
            .as_deref()
            .is_some_and(|n| n != ctx.platform.mojang_os_name())
        {
            return false;
        }
        if os
            .arch
            .as_deref()
            .is_some_and(|a| a != mojang_arch(ctx.platform.arch))
        {
            return false;
        }
    }
    if let Some(features) = &rule.features {
        if features
            .iter()
            .any(|(name, wanted)| ctx.features.contains(name) != *wanted)
        {
            return false;
        }
    }
    true
}

fn mojang_arch(arch: Arch) -> &'static str {
    match arch {
        Arch::X64 => "x86_64",
        Arch::X86 => "x86",
        Arch::Arm64 => "arm64",
    }
}

/// LWJGL 3 ships one library per arch (`natives-windows`, `-x86`, `-arm64`) under a single os rule.
pub fn natives_arch_ok(library_name: &str, arch: Arch) -> bool {
    let Some(classifier) = library_name.split(':').nth(3) else {
        return true;
    };
    if !classifier.starts_with("natives-") {
        return true;
    }
    if classifier.ends_with("-x86") {
        arch == Arch::X86
    } else if classifier.ends_with("-arm64") || classifier.ends_with("-aarch_64") {
        arch == Arch::Arm64
    } else if classifier.ends_with("-arm32") {
        false
    } else {
        arch == Arch::X64
    }
}

pub fn library_applies(lib: &Library, ctx: &RuleContext) -> bool {
    rules_allow(lib.rules.as_deref(), ctx) && natives_arch_ok(&lib.name, ctx.platform.arch)
}

/// Flatten argument templates, keeping only those whose rules allow this context.
pub fn resolve_args(args: &[ArgValue], ctx: &RuleContext) -> Vec<String> {
    let mut out = Vec::new();
    for arg in args {
        match arg {
            ArgValue::Plain(s) => out.push(s.clone()),
            ArgValue::Conditional { rules, value } => {
                if rules_allow(Some(rules.as_slice()), ctx) {
                    out.extend(value.to_vec());
                }
            }
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{version, LINUX_X64, WINDOWS_X64};

    #[test]
    fn library_counts_match_mojang_rules() {
        let v = version("1.20.1.json");
        let win = RuleContext::new(WINDOWS_X64);
        let linux = RuleContext::new(LINUX_X64);
        assert_eq!(
            v.libraries
                .iter()
                .filter(|l| library_applies(l, &win))
                .count(),
            50
        );
        assert_eq!(
            v.libraries
                .iter()
                .filter(|l| library_applies(l, &linux))
                .count(),
            52
        );
    }

    #[test]
    fn native_classifier_arch_filter() {
        assert!(natives_arch_ok(
            "org.lwjgl:lwjgl:3.3.1:natives-windows",
            Arch::X64
        ));
        assert!(!natives_arch_ok(
            "org.lwjgl:lwjgl:3.3.1:natives-windows-x86",
            Arch::X64
        ));
        assert!(!natives_arch_ok(
            "org.lwjgl:lwjgl:3.3.1:natives-windows-arm64",
            Arch::X64
        ));
        assert!(natives_arch_ok(
            "org.lwjgl:lwjgl:3.3.1:natives-windows-arm64",
            Arch::Arm64
        ));
        assert!(!natives_arch_ok(
            "org.lwjgl:lwjgl:3.3.1:natives-linux-arm32",
            Arch::X64
        ));
        assert!(natives_arch_ok("org.lwjgl:lwjgl:3.3.1", Arch::X86));
    }

    #[test]
    fn jvm_args_follow_os_and_arch_rules() {
        let v = version("1.20.1.json");
        let jvm = resolve_args(
            &v.arguments.as_ref().unwrap().jvm,
            &RuleContext::new(WINDOWS_X64),
        );
        assert!(jvm.iter().any(|a| a.starts_with("-XX:HeapDumpPath=")));
        assert!(!jvm.contains(&"-XstartOnFirstThread".to_string()));
        assert!(!jvm.contains(&"-Xss1M".to_string()));
        assert!(jvm.contains(&"${classpath}".to_string()));
    }

    #[test]
    fn game_args_follow_feature_rules() {
        let v = version("1.20.1.json");
        let game_args = &v.arguments.as_ref().unwrap().game;
        let plain = resolve_args(game_args, &RuleContext::new(WINDOWS_X64));
        assert!(!plain.contains(&"--demo".to_string()));
        assert!(!plain.contains(&"--width".to_string()));
        let sized = resolve_args(
            game_args,
            &RuleContext::new(WINDOWS_X64).with_feature("has_custom_resolution"),
        );
        assert!(sized.contains(&"--width".to_string()));
    }

    #[test]
    fn last_matching_rule_wins() {
        let rules: Vec<Rule> = serde_json::from_str(
            r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#,
        )
        .unwrap();
        assert!(rules_allow(
            Some(rules.as_slice()),
            &RuleContext::new(WINDOWS_X64)
        ));
        let mac = Platform {
            os: crate::platform::Os::MacOs,
            arch: Arch::Arm64,
        };
        assert!(!rules_allow(Some(rules.as_slice()), &RuleContext::new(mac)));
        assert!(rules_allow(None, &RuleContext::new(mac)));
    }
}
