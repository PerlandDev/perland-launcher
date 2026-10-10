//! Likely causes for a game that exited on its own with an error.
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CrashHint {
    /// Java ran out of memory: the player can raise the instance's memory.
    OutOfMemory,
}

/// Look through the last lines of output for a cause the launcher can explain.
pub fn diagnose(tail: &[String]) -> Option<CrashHint> {
    tail.iter()
        .any(|line| line.contains("java.lang.OutOfMemoryError"))
        .then_some(CrashHint::OutOfMemory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_memory_is_recognised() {
        let tail = vec![
            "[12:00:01] [Render thread/INFO]: Loading".to_string(),
            "Exception in thread \"main\" java.lang.OutOfMemoryError: Java heap space".to_string(),
        ];
        assert_eq!(diagnose(&tail), Some(CrashHint::OutOfMemory));
    }

    #[test]
    fn ordinary_output_has_no_hint() {
        assert_eq!(diagnose(&["Stopping!".to_string()]), None);
        assert_eq!(diagnose(&[]), None);
    }
}
