//! The player's own Java arguments: split a typed string into arguments and refuse the ones
//! that would take the launch away from the launcher.

use crate::error::{ErrorCode, LauncherError, Result};

/// Longest raw string, in characters.
pub const MAX_JVM_ARGS_CHARS: usize = 8192;

/// Options the launcher sets itself: the class path, the module path, and the main class.
const OWNED: &[&str] = &[
    "-cp",
    "-classpath",
    "--class-path",
    "-p",
    "--module-path",
    "-jar",
    "-m",
    "--module",
    // These make Java print something and exit before the game starts.
    "-version",
    "--version",
    "-help",
    "--help",
    "-h",
    "-?",
    "-X",
    "--help-extra",
    "--dry-run",
    "--list-modules",
    "-d",
    "--describe-module",
    "--validate-modules",
];

/// Options that set the maximum heap, which the memory slider owns.
const MEMORY_PREFIXES: &[&str] = &[
    "-Xmx",
    "-XX:MaxHeapSize=",
    "-XX:MaxRAMPercentage=",
    "-XX:MaxRAM=",
];

/// Long options that also take their value after an `=`.
const OWNED_WITH_VALUE: &[&str] = &[
    "--class-path=",
    "--module-path=",
    "--module=",
    "--describe-module=",
];

/// Module options that take their value as the next word (`--add-opens a/b=c`) as well as after
/// an `=`.
const TAKES_VALUE: &[&str] = &[
    "--add-opens",
    "--add-exports",
    "--add-reads",
    "--add-modules",
    "--enable-native-access",
    "--patch-module",
    "--limit-modules",
];

/// Split `raw` into arguments. Whitespace (newlines too) separates them; `"…"` and `'…'` group
/// text with spaces and are removed, even in the middle of a token. A backslash is always
/// literal. Each argument must be an option the launcher doesn't own, and memory goes through
/// the slider. A module option may be followed by its value, which needn't start with a dash.
pub fn parse(raw: &str) -> Result<Vec<String>> {
    if raw.chars().count() > MAX_JVM_ARGS_CHARS {
        return Err(LauncherError::java(
            ErrorCode::JvmArgsTooLong,
            format!("The Java arguments are over {MAX_JVM_ARGS_CHARS} characters"),
        )
        .with_param("max", MAX_JVM_ARGS_CHARS)
        .with_fix("Remove the arguments you don't need"));
    }
    let args = split(raw)?;
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        check(arg)?;
        // The value after a module option is not an option; a dashed word is still checked.
        if TAKES_VALUE.contains(&arg.as_str()) && iter.peek().is_some_and(|v| !v.starts_with('-')) {
            iter.next();
        }
    }
    Ok(args)
}

fn split(raw: &str) -> Result<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    // Set once a token starts, so an empty `""` still makes one.
    let mut in_token = false;
    for c in raw.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_token = true;
            }
            None if c.is_whitespace() => {
                if in_token {
                    args.push(std::mem::take(&mut current));
                    in_token = false;
                }
            }
            None => {
                current.push(c);
                in_token = true;
            }
        }
    }
    if quote.is_some() {
        return Err(LauncherError::java(
            ErrorCode::JvmArgsUnclosedQuote,
            "A quote in the Java arguments isn't closed",
        )
        .with_fix("Add the closing quote, or remove the opening one"));
    }
    if in_token {
        args.push(current);
    }
    Ok(args)
}

fn check(arg: &str) -> Result<()> {
    if !arg.starts_with('-') {
        return Err(LauncherError::java(
            ErrorCode::JvmArgNotOption,
            "A Java argument doesn't start with a dash",
        )
        .with_param("arg", if arg.is_empty() { "\"\"" } else { arg })
        .with_fix("Remove it, or add the dash it's missing"));
    }
    if MEMORY_PREFIXES.iter().any(|p| arg.starts_with(p)) {
        return Err(LauncherError::java(
            ErrorCode::JvmArgsMemory,
            "Memory is set with the slider, not in the Java arguments",
        )
        .with_fix("Remove it and move the memory slider instead"));
    }
    if OWNED.contains(&arg) || OWNED_WITH_VALUE.iter().any(|p| arg.starts_with(p)) {
        return Err(LauncherError::java(
            ErrorCode::JvmArgNotAllowed,
            "The launcher sets this Java option itself",
        )
        .with_param("arg", arg)
        .with_fix("Remove it from the Java arguments"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn splits_on_whitespace_and_newlines() {
        assert_eq!(
            parse("-XX:+UseG1GC\n  -Dx=1").unwrap(),
            strs(&["-XX:+UseG1GC", "-Dx=1"])
        );
        assert!(parse("").unwrap().is_empty());
        assert!(parse("  \n").unwrap().is_empty());
    }

    #[test]
    fn quotes_group_and_are_removed() {
        assert_eq!(
            parse(r#"-Dname="My Pack" -Dq='say "hi"'"#).unwrap(),
            strs(&["-Dname=My Pack", "-Dq=say \"hi\""])
        );
        assert_eq!(parse(r#"-Dx="""#).unwrap(), strs(&["-Dx="]));
        let err = parse(r#""""#).unwrap_err();
        assert_eq!(err.code, ErrorCode::JvmArgNotOption);
        // Two quote characters, so the message reads as an empty argument.
        assert_eq!(err.params["arg"], r#""""#);
    }

    #[test]
    fn backslashes_are_literal() {
        assert_eq!(
            parse(r#"-Djava.io.tmpdir="C:\Users\José\Mi Carpeta""#).unwrap(),
            strs(&[r"-Djava.io.tmpdir=C:\Users\José\Mi Carpeta"])
        );
    }

    #[test]
    fn unclosed_quote_is_refused() {
        let err = parse(r#"-Dx="a"#).unwrap_err();
        assert_eq!(err.code, ErrorCode::JvmArgsUnclosedQuote);
    }

    #[test]
    fn every_arg_is_an_option() {
        for token in ["foo", "@args.txt"] {
            let err = parse(&format!("-Dx=1 {token}")).unwrap_err();
            assert_eq!(err.code, ErrorCode::JvmArgNotOption);
            assert_eq!(err.params["arg"], token);
        }
    }

    #[test]
    fn launcher_owned_args_are_refused() {
        for arg in [
            "-cp",
            "-classpath",
            "--class-path=x",
            "-p",
            "--module-path=x",
            "-jar",
            "-m",
            "--module=x",
            "-version",
            "--help",
            "-?",
            "-X",
            "--dry-run",
        ] {
            let err = parse(arg).unwrap_err();
            assert_eq!(err.code, ErrorCode::JvmArgNotAllowed, "{arg}");
            assert_eq!(err.params["arg"], arg);
        }
        assert_eq!(
            parse("-Xss4M -Xms2G -XX:+UseZGC --add-opens=java.base/java.lang=ALL-UNNAMED")
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn memory_goes_through_the_slider() {
        assert_eq!(parse("-Xmx8G").unwrap_err().code, ErrorCode::JvmArgsMemory);
        for arg in [
            "-XX:MaxHeapSize=8g",
            "-XX:MaxRAMPercentage=75",
            "-XX:MaxRAM=16g",
        ] {
            assert_eq!(
                parse(arg).unwrap_err().code,
                ErrorCode::JvmArgsMemory,
                "{arg}"
            );
        }
        // The starting size can still be set.
        assert_eq!(parse("-Xms2G -XX:InitialHeapSize=2g").unwrap().len(), 2);
    }

    #[test]
    fn module_options_take_the_next_token_as_their_value() {
        assert_eq!(
            parse("--add-opens java.base/java.lang=ALL-UNNAMED").unwrap(),
            strs(&["--add-opens", "java.base/java.lang=ALL-UNNAMED"])
        );
        assert_eq!(
            parse("--add-modules jdk.incubator.vector -XX:+UseG1GC").unwrap(),
            strs(&["--add-modules", "jdk.incubator.vector", "-XX:+UseG1GC"])
        );
        assert_eq!(
            parse("--add-opens=java.base/java.lang=ALL-UNNAMED")
                .unwrap()
                .len(),
            1
        );
        for opt in [
            "--add-opens",
            "--add-exports",
            "--add-reads",
            "--add-modules",
            "--enable-native-access",
            "--patch-module",
            "--limit-modules",
        ] {
            assert_eq!(parse(&format!("{opt} a/b=c")).unwrap().len(), 2, "{opt}");
            // With no value after it, it passes through for Java to report.
            assert_eq!(parse(opt).unwrap(), strs(&[opt]), "{opt}");
        }
        // Only a token that isn't an option is taken as the value.
        let err = parse("--add-opens -cp").unwrap_err();
        assert_eq!(err.code, ErrorCode::JvmArgNotAllowed);
        assert_eq!(err.params["arg"], "-cp");
        // A value is consumed once; the next plain word is still refused.
        let err = parse("--add-opens a/b=c stray").unwrap_err();
        assert_eq!(err.code, ErrorCode::JvmArgNotOption);
        assert_eq!(err.params["arg"], "stray");
        // Other options don't take a following word.
        assert_eq!(
            parse("-Xss4M value").unwrap_err().code,
            ErrorCode::JvmArgNotOption
        );
    }

    #[test]
    fn too_long_is_refused() {
        let raw = "-Da ".repeat(MAX_JVM_ARGS_CHARS / 4 + 1);
        let err = parse(&raw).unwrap_err();
        assert_eq!(err.code, ErrorCode::JvmArgsTooLong);
        assert_eq!(err.params["max"], "8192");
        // Characters are counted, not bytes.
        let exact = "é".repeat(MAX_JVM_ARGS_CHARS);
        assert_eq!(
            parse(&format!("-D{exact}")).unwrap_err().code,
            ErrorCode::JvmArgsTooLong
        );
        assert!(parse(&format!("-D{}", "é".repeat(MAX_JVM_ARGS_CHARS - 2))).is_ok());
    }
}
