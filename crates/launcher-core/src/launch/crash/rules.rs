//! The crash rule table. Rules are tried in order and the first that matches any source wins,
//! so a cause comes before its consequence: running out of memory mid-load also breaks mixins,
//! and a missing dependency also makes classes fail to load.
//!
//! The loader messages read here are the client's counterparts of those PerlandEngine reads for
//! servers in `internal/minecraft/diagnosis/catalogue.go`; the patterns are written for the
//! launcher, not ported (see NOTICE).
use std::{ops::Range, sync::LazyLock};

use regex::{Captures, Regex};

use super::{Crash, CrashCause, Evidence, ModRef, Vendor};
use crate::error::ErrorCode;

/// Finds the mod whose jar holds a mixin config, by the config's file name.
pub type MixinOwner<'a> = &'a dyn Fn(&str) -> Option<String>;

struct Found {
    cause: CrashCause,
    at: Range<usize>,
}

struct Rule {
    code: ErrorCode,
    find: fn(&str, MixinOwner) -> Option<Found>,
}

const RULES: &[Rule] = &[
    Rule {
        code: ErrorCode::CrashOutOfMemory,
        find: out_of_memory,
    },
    Rule {
        code: ErrorCode::CrashGraphicsDriver,
        find: graphics_driver,
    },
    Rule {
        code: ErrorCode::CrashJavaTooOld,
        find: java_too_old,
    },
    Rule {
        code: ErrorCode::CrashJavaTooNew,
        find: java_too_new,
    },
    Rule {
        code: ErrorCode::CrashModMissingDependency,
        find: missing_dependency,
    },
    Rule {
        code: ErrorCode::CrashModsClash,
        find: mods_clash,
    },
    Rule {
        code: ErrorCode::CrashModWrongVersion,
        find: wrong_version,
    },
    Rule {
        code: ErrorCode::CrashModFailed,
        find: mod_failed,
    },
    Rule {
        code: ErrorCode::CrashModFileDamaged,
        find: mod_file_damaged,
    },
    Rule {
        code: ErrorCode::CrashGameFilesDamaged,
        find: game_files_damaged,
    },
];

/// Every rule's code, in the order the rules are tried.
pub fn rule_codes() -> impl Iterator<Item = ErrorCode> {
    RULES.iter().map(|r| r.code)
}

/// The detail kept with a cause is at most this long.
const MAX_DETAIL: usize = 2048;

/// The first rule that recognises any of `evidence`, with the line it matched as the detail.
pub fn classify(evidence: &[Evidence], owner: MixinOwner) -> Option<Crash> {
    RULES.iter().find_map(|rule| {
        evidence.iter().find_map(|ev| {
            (rule.find)(&ev.text, owner).map(|found| Crash {
                cause: found.cause,
                detail: cap(lines_of(&ev.text, found.at)),
            })
        })
    })
}

fn lines_of(text: &str, at: Range<usize>) -> &str {
    let start = text[..at.start].rfind('\n').map_or(0, |i| i + 1);
    let end = text[at.end..].find('\n').map_or(text.len(), |i| at.end + i);
    text[start..end].trim()
}

fn cap(s: &str) -> String {
    if s.len() <= MAX_DETAIL {
        return s.to_string();
    }
    let mut end = MAX_DETAIL;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &s[..end])
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("crash rule patterns are valid")
}

fn found(cause: CrashCause, caps: &Captures) -> Found {
    let at = caps.get(0).expect("group 0 always matches").range();
    Found { cause, at }
}

fn group(caps: &Captures, name: &str) -> Option<String> {
    caps.name(name).map(|m| m.as_str().to_string())
}

/// A mod as Fabric and Quilt print it: `Mod 'Name' (id) version`. A name may hold an apostrophe
/// ("Xaero's Minimap"), so it ends at the `' (` before the id.
const FABRIC_MOD: &str = r"Mod '(?P<name>.+?)' \((?P<id>[\w.-]+)\) \S+";

fn fabric_mod(caps: &Captures) -> ModRef {
    ModRef {
        id: caps["id"].to_string(),
        name: group(caps, "name"),
        file: None,
    }
}

fn out_of_memory(text: &str, _: MixinOwner) -> Option<Found> {
    // Not "unable to create native thread": more heap doesn't help that one.
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"(?m)java\.lang\.OutOfMemoryError(?:: (?:Java heap space|GC overhead limit exceeded|Metaspace|Direct buffer memory|Requested array size exceeds VM limit))?[ \t]*$",
        )
    });
    RE.captures(text)
        .map(|c| found(CrashCause::OutOfMemory, &c))
}

fn graphics_driver(text: &str, _: MixinOwner) -> Option<Found> {
    // The problematic frame of a JVM fatal error log, when it is in a GPU driver.
    static FRAME: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"(?mi)^#\s+C\s+\[(?:(?P<amd>atio6axx|atioglxx|amdxx\w*)|(?P<nvidia>nvoglv\w*|libnvidia-glcore)|(?P<intel>ig\w*icd\w*))[.+]",
        )
    });
    static OPENGL: LazyLock<Regex> = LazyLock::new(|| {
        re(r"Pixel format not accelerated|WGL: The driver does not appear to support OpenGL")
    });
    if let Some(c) = FRAME.captures(text) {
        let vendor = [
            ("amd", Vendor::Amd),
            ("nvidia", Vendor::Nvidia),
            ("intel", Vendor::Intel),
        ]
        .into_iter()
        .find_map(|(name, vendor)| c.name(name).map(|_| vendor));
        return Some(found(CrashCause::GraphicsDriver { vendor }, &c));
    }
    OPENGL
        .captures(text)
        .map(|c| found(CrashCause::GraphicsDriver { vendor: None }, &c))
}

/// A class file version is the Java version plus 44 (Java 17 writes 61, Java 21 writes 65).
fn java_of_class_version(v: &str) -> Option<u32> {
    v.parse::<u32>().ok()?.checked_sub(44)
}

fn java_too_old(text: &str, _: MixinOwner) -> Option<Found> {
    static CLASS: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"UnsupportedClassVersionError: \S+ has been compiled by a more recent version of the Java Runtime \(class file version (?P<needs>\d+)\.\d+\), this version of the Java Runtime only recognizes class file versions up to (?P<has>\d+)\.\d+",
        )
    });
    static FABRIC: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"requires version (?P<needs>\d+)[^\n]*?\(java\)(?:, but only the wrong version is present: (?P<has>\d+))?",
        )
    });
    if let Some(c) = CLASS.captures(text) {
        let required = java_of_class_version(&c["needs"])?;
        let running = java_of_class_version(&c["has"]);
        return Some(found(CrashCause::JavaTooOld { required, running }, &c));
    }
    let c = FABRIC.captures(text)?;
    let required = c["needs"].parse().ok()?;
    let running = c.name("has").and_then(|m| m.as_str().parse().ok());
    Some(found(CrashCause::JavaTooOld { required, running }, &c))
}

fn java_too_new(text: &str, _: MixinOwner) -> Option<Found> {
    // LaunchWrapper (Forge 1.12 and older) assumes Java 8's class loader.
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re(r"ClassLoaders\$AppClassLoader cannot be cast to (?:class )?java\.net\.URLClassLoader")
    });
    RE.captures(text).map(|c| found(CrashCause::JavaTooNew, &c))
}

/// What a mod can depend on that isn't another mod: the game, its loader, Java.
fn is_platform(id: &str) -> bool {
    matches!(
        id,
        "minecraft" | "java" | "fabricloader" | "quilt_loader" | "forge" | "neoforge"
    )
}

/// FML's dependency line: `Mod ID: 'dep', Requested by: 'mod', …, Actual version: '…'`.
static FML_DEPENDENCY: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"Mod ID: '(?P<dep>[^']+)', Requested by: '(?P<id>[^']+)', Expected range: '[^']*', Actual version: '(?P<actual>[^']*)'",
    )
});

fn fml_mod(caps: &Captures) -> ModRef {
    ModRef {
        id: caps["id"].to_string(),
        name: None,
        file: None,
    }
}

fn missing_dependency(text: &str, _: MixinOwner) -> Option<Found> {
    static FABRIC: LazyLock<Regex> = LazyLock::new(|| {
        re(&format!(
            r"{FABRIC_MOD} requires [^\n]*? of (?:mod )?(?:'[^'\n]*' \()?'?(?P<dep>[\w.-]+)'?\)?, which is missing!"
        ))
    });
    let fabric = FABRIC.captures_iter(text).find(|c| !is_platform(&c["dep"]));
    if let Some(c) = fabric {
        let cause = CrashCause::ModMissingDependency {
            module: fabric_mod(&c),
            missing: c["dep"].to_string(),
        };
        return Some(found(cause, &c));
    }
    let c = FML_DEPENDENCY
        .captures_iter(text)
        .find(|c| &c["actual"] == "[MISSING]" && !is_platform(&c["dep"]))?;
    let cause = CrashCause::ModMissingDependency {
        module: fml_mod(&c),
        missing: c["dep"].to_string(),
    };
    Some(found(cause, &c))
}

fn mods_clash(text: &str, _: MixinOwner) -> Option<Found> {
    // Fabric's "breaks" (an error), not its "conflicts" (a warning the game carries on past).
    static RE: LazyLock<Regex> = LazyLock::new(|| {
        re(&format!(
            r"(?m)^\s*- {FABRIC_MOD} is incompatible with [^\n]*? of (?:mod )?'(?P<other_name>.+?)' \((?P<other>[\w.-]+)\)"
        ))
    });
    let c = RE.captures(text)?;
    let cause = CrashCause::ModsClash {
        module: fabric_mod(&c),
        other: ModRef {
            id: c["other"].to_string(),
            name: group(&c, "other_name"),
            file: None,
        },
    };
    Some(found(cause, &c))
}

fn wrong_version(text: &str, _: MixinOwner) -> Option<Found> {
    static FABRIC: LazyLock<Regex> = LazyLock::new(|| {
        re(&format!(
            r"{FABRIC_MOD} requires [^\n]*?\((?:minecraft|fabricloader|quilt_loader)\)"
        ))
    });
    if let Some(c) = FABRIC.captures(text) {
        return Some(found(
            CrashCause::ModWrongVersion {
                module: fabric_mod(&c),
            },
            &c,
        ));
    }
    let c = FML_DEPENDENCY
        .captures_iter(text)
        .find(|c| matches!(&c["dep"], "minecraft" | "forge" | "neoforge"))?;
    Some(found(
        CrashCause::ModWrongVersion {
            module: fml_mod(&c),
        },
        &c,
    ))
}

fn mod_failed(text: &str, owner: MixinOwner) -> Option<Found> {
    const CONFIG: &str = r"(?P<config>[\w.-]*mixins?[\w.-]*\.json)";
    static APPLY: LazyLock<Regex> = LazyLock::new(|| {
        re(&format!(
            r"Mixin apply (?:for mod (?P<id>[\w.-]+) )?failed {CONFIG}"
        ))
    });
    static INJECT: LazyLock<Regex> = LazyLock::new(|| {
        re(&format!(
            r"(?:Critical injection failure|InvalidMixinException|InvalidInjectionException|MixinApplyError)[^\n]*?-> {CONFIG}:\S+(?: from mod (?P<id>[\w.-]+))?"
        ))
    });
    // Without a mod that owns it, a mixin error names no culprit: it stays generic.
    [&*APPLY, &*INJECT].into_iter().find_map(|re| {
        re.captures_iter(text).find_map(|c| {
            let id = group(&c, "id").or_else(|| owner(&c["config"]))?;
            let module = ModRef {
                id,
                name: None,
                file: None,
            };
            Some(found(CrashCause::ModFailed { module }, &c))
        })
    })
}

static ZIP_ERROR: LazyLock<Regex> = LazyLock::new(|| {
    re(
        r"ZipException|invalid LOC header|zip END header not found|zip file is empty|Invalid or corrupt jarfile",
    )
});

/// The first line with a zip error that also matches `path`.
fn zip_error_naming<'t>(text: &'t str, path: &Regex) -> Option<(Range<usize>, Captures<'t>)> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if ZIP_ERROR.is_match(line) {
            if let Some(c) = path.captures(line) {
                let at = offset..offset + line.trim_end().len();
                return Some((at, c));
            }
        }
        offset += line.len();
    }
    None
}

fn mod_file_damaged(text: &str, _: MixinOwner) -> Option<Found> {
    static PATH: LazyLock<Regex> =
        LazyLock::new(|| re(r#"mods[/\\](?P<file>[^/\\\[\]\n:'"]+?\.jar)\b"#));
    let (at, c) = zip_error_naming(text, &PATH)?;
    let cause = CrashCause::ModFileDamaged {
        file: c["file"].to_string(),
    };
    Some(Found { cause, at })
}

fn game_files_damaged(text: &str, _: MixinOwner) -> Option<Found> {
    static PATH: LazyLock<Regex> = LazyLock::new(|| re(r"(?:libraries|versions)[/\\]\S+\.jar"));
    // The entry classes of the game and of each loader: when they can't load, the jars are bad.
    static MAIN: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"(?:Could not find or load main class|ClassNotFoundException:) (?:net\.minecraft\.client\.main\.Main|net\.fabricmc\.loader\.impl\.launch\.knot\.KnotClient|org\.quiltmc\.loader\.impl\.launch\.knot\.KnotClient|cpw\.mods\.bootstraplauncher\.BootstrapLauncher|net\.minecraftforge\.bootstrap\.ForgeBootstrap|net\.minecraft\.launchwrapper\.Launch)\b",
        )
    });
    if let Some((at, _)) = zip_error_naming(text, &PATH) {
        return Some(Found {
            cause: CrashCause::GameFilesDamaged,
            at,
        });
    }
    MAIN.captures(text)
        .map(|c| found(CrashCause::GameFilesDamaged, &c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launch::crash::Source;
    use std::collections::BTreeMap;

    struct Case {
        source: Source,
        owners: BTreeMap<String, String>,
        params: BTreeMap<String, String>,
        text: String,
    }

    fn fixture(code: ErrorCode, hit: bool) -> &'static str {
        macro_rules! files {
            ($($variant:ident => $name:literal),* $(,)?) => {
                match (code, hit) {
                    $(
                        (ErrorCode::$variant, true) => include_str!(concat!("testdata/", $name, ".hit.txt")),
                        (ErrorCode::$variant, false) => include_str!(concat!("testdata/", $name, ".miss.txt")),
                    )*
                    _ => panic!("{code:?} has no fixtures"),
                }
            };
        }
        files! {
            CrashOutOfMemory => "crash_out_of_memory",
            CrashGraphicsDriver => "crash_graphics_driver",
            CrashJavaTooOld => "crash_java_too_old",
            CrashJavaTooNew => "crash_java_too_new",
            CrashModMissingDependency => "crash_mod_missing_dependency",
            CrashModsClash => "crash_mods_clash",
            CrashModWrongVersion => "crash_mod_wrong_version",
            CrashModFailed => "crash_mod_failed",
            CrashModFileDamaged => "crash_mod_file_damaged",
            CrashGameFilesDamaged => "crash_game_files_damaged",
        }
    }

    fn cases(code: ErrorCode, hit: bool) -> Vec<Case> {
        let data = fixture(code, hit).replace("\r\n", "\n");
        data.split("\n---\n")
            .map(|block| {
                let mut case = Case {
                    source: Source::Tail,
                    owners: BTreeMap::new(),
                    params: BTreeMap::new(),
                    text: String::new(),
                };
                let mut lines = block.lines().peekable();
                while let Some(header) = lines.peek().and_then(|l| l.strip_prefix("# ")) {
                    let (key, value) = header.split_once(' ').unwrap_or((header, ""));
                    let pair = || value.split_once('=').expect("k=v");
                    match key {
                        "source:" => {
                            case.source = match value {
                                "tail" => Source::Tail,
                                "crash_report" => Source::CrashReport,
                                "latest_log" => Source::LatestLog,
                                "hs_err" => Source::HsErr,
                                other => panic!("unknown source {other}"),
                            }
                        }
                        "param" => {
                            let (k, v) = pair();
                            case.params.insert(k.into(), v.into());
                        }
                        "owner" => {
                            let (k, v) = pair();
                            case.owners.insert(k.into(), v.into());
                        }
                        // An hs_err file starts with "#" lines of its own.
                        _ => break,
                    }
                    lines.next();
                }
                case.text = lines.collect::<Vec<_>>().join("\n");
                case
            })
            .collect()
    }

    fn run(case: &Case) -> Option<Crash> {
        let evidence = [Evidence {
            source: case.source,
            text: case.text.clone(),
        }];
        classify(&evidence, &|config| case.owners.get(config).cloned())
    }

    #[test]
    fn every_rule_has_a_hit_and_a_near_miss() {
        for code in rule_codes() {
            assert!(!cases(code, true).is_empty(), "{code:?}");
            assert!(!cases(code, false).is_empty(), "{code:?}");
        }
    }

    #[test]
    fn hits_classify_as_their_rule_with_their_params() {
        for code in rule_codes() {
            for (i, case) in cases(code, true).iter().enumerate() {
                let crash = run(case).unwrap_or_else(|| panic!("{code:?} hit {i} found nothing"));
                assert_eq!(crash.code(), code, "{code:?} hit {i}");
                let error = crash.error();
                for (k, v) in &case.params {
                    assert_eq!(error.params.get(k), Some(v), "{code:?} hit {i} param {k}");
                }
                assert!(!crash.detail.is_empty(), "{code:?} hit {i} has a detail");
            }
        }
    }

    #[test]
    fn near_misses_are_not_their_rule() {
        for code in rule_codes() {
            for (i, case) in cases(code, false).iter().enumerate() {
                let got = run(case).map(|c| c.code());
                assert_ne!(got, Some(code), "{code:?} miss {i}");
            }
        }
    }

    #[test]
    fn a_cause_wins_over_its_consequence() {
        let text = "\
[12:00:01] [main/ERROR]: java.lang.OutOfMemoryError: Java heap space
[12:00:02] [main/FATAL] [mixin/]: Mixin apply for mod create failed create.mixins.json:X -> y";
        let evidence = [Evidence {
            source: Source::Tail,
            text: text.into(),
        }];
        let crash = classify(&evidence, &|_| None).unwrap();
        assert_eq!(crash.cause, CrashCause::OutOfMemory);
    }

    #[test]
    fn the_detail_is_the_matched_line_capped() {
        let long = format!("x {} java.lang.OutOfMemoryError", "é".repeat(MAX_DETAIL));
        let evidence = [Evidence {
            source: Source::Tail,
            text: format!("before\n{long}\nafter"),
        }];
        let crash = classify(&evidence, &|_| None).unwrap();
        assert!(crash.detail.starts_with("x "));
        assert!(crash.detail.len() <= MAX_DETAIL + '…'.len_utf8());
        assert!(!crash.detail.contains("before"));
    }

    #[test]
    fn ordinary_output_has_no_cause() {
        let evidence = [Evidence {
            source: Source::Tail,
            text: "Stopping!".into(),
        }];
        assert_eq!(classify(&evidence, &|_| None), None);
        assert_eq!(classify(&[], &|_| None), None);
    }
}
