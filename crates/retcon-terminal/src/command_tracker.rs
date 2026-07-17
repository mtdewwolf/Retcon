//! Tracks interactive shell input to detect submitted commands.

/// Accumulates keystrokes until Enter and emits completed command lines.
#[derive(Clone, Debug, Default)]
pub struct CommandLineTracker {
    buffer: String,
}

impl CommandLineTracker {
    /// Create an empty command-line tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push terminal input bytes interpreted as UTF-8 lossy text.
    ///
    /// Returns command lines completed by an Enter key (`\r` or `\n`).
    pub fn push_input(&mut self, data: &str) -> Vec<String> {
        let mut commands = Vec::new();
        for ch in data.chars() {
            match ch {
                '\r' | '\n' => {
                    let command = self.buffer.trim().to_owned();
                    if !command.is_empty() {
                        commands.push(command);
                    }
                    self.buffer.clear();
                }
                '\x7f' | '\x08' => {
                    self.buffer.pop();
                }
                c if c.is_control() && c != '\t' => {}
                c => self.buffer.push(c),
            }
        }
        commands
    }

    #[must_use]
    /// Current in-progress command line without a trailing Enter.
    pub fn line(&self) -> &str {
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::CommandLineTracker;

    #[test]
    fn enter_emits_trimmed_command() {
        let mut tracker = CommandLineTracker::new();
        tracker.push_input("cargo test");
        let commands = tracker.push_input("\r\n");
        assert_eq!(commands, vec!["cargo test".to_owned()]);
        assert!(tracker.line().is_empty());
    }

    #[test]
    fn backspace_edits_current_line() {
        let mut tracker = CommandLineTracker::new();
        tracker.push_input("ls\x08\x08dir");
        assert_eq!(tracker.line(), "dir");
    }

    #[test]
    fn blank_enter_is_ignored() {
        let mut tracker = CommandLineTracker::new();
        assert!(tracker.push_input("   \n").is_empty());
    }
}
