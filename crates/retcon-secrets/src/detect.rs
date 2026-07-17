//! Regex and entropy-based secret detection.

use std::sync::OnceLock;

use regex::Regex;

/// How a secret was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingKind {
    Pattern,
    Entropy,
}

/// A single secret finding inside scanned content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretFinding {
    pub kind: FindingKind,
    pub label: String,
    pub start: usize,
    pub end: usize,
}

/// Outcome of scanning one or more text blobs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScanResult {
    pub findings: Vec<SecretFinding>,
}

impl ScanResult {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        self.findings.extend(other.findings);
        self
    }

    /// Short, non-sensitive summary for logs and RPC errors.
    #[must_use]
    pub fn summary(&self) -> String {
        if self.findings.is_empty() {
            return "no secrets detected".into();
        }
        let labels: Vec<_> = self
            .findings
            .iter()
            .map(|finding| finding.label.as_str())
            .collect();
        format!("{} finding(s): {}", self.findings.len(), labels.join(", "))
    }
}

struct PatternRule {
    regex: Regex,
    label: &'static str,
}

fn pattern_rules() -> &'static [PatternRule] {
    static RULES: OnceLock<Vec<PatternRule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let specs: &[(&str, &str)] = &[
            (
                r"(?i)\b(?:api[_-]?key|secret[_-]?key|access[_-]?token)\s*[:=]\s*\S+",
                "assignment",
            ),
            (
                r"(?i)\b(?:password|passwd|pwd)\s*[:=]\s*\S+",
                "password assignment",
            ),
            (
                r"(?i)\b(?:token|secret|authorization)\s*[:=]\s*\S+",
                "credential assignment",
            ),
            (r"(?i)Bearer\s+[A-Za-z0-9\-._~+/]+=*", "bearer token"),
            (
                r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----",
                "private key",
            ),
            (r"\bAKIA[0-9A-Z]{16}\b", "AWS access key"),
            (r"\b(?:ASIA)[0-9A-Z]{16}\b", "AWS session key"),
            (r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b", "GitHub token"),
            (r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b", "Slack token"),
            (
                r"\bsk_(?:live|test)_[A-Za-z0-9]{16,}\b",
                "Stripe secret key",
            ),
            (
                r"\brk_(?:live|test)_[A-Za-z0-9]{16,}\b",
                "Stripe restricted key",
            ),
            (
                r"\beyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9._-]{10,}\.[A-Za-z0-9._-]{10,}\b",
                "JWT",
            ),
            (
                r"(?i)(?:postgres|mysql|mongodb|redis)://[^\s/:@]+:[^\s/@]+@",
                "database URL with password",
            ),
        ];
        specs
            .iter()
            .filter_map(|(pattern, label)| {
                Regex::new(pattern)
                    .ok()
                    .map(|regex| PatternRule { regex, label })
            })
            .collect()
    })
}

/// Scan a single text blob for secrets.
#[must_use]
pub fn scan_text(text: &str) -> ScanResult {
    scan_texts([text])
}

/// Scan multiple text blobs and merge findings.
#[must_use]
pub fn scan_texts<'a, I>(texts: I) -> ScanResult
where
    I: IntoIterator<Item = &'a str>,
{
    let mut findings = Vec::new();
    for text in texts {
        findings.extend(scan_patterns(text));
        findings.extend(scan_entropy(text));
    }
    ScanResult { findings }
}

fn scan_patterns(text: &str) -> Vec<SecretFinding> {
    let mut findings = Vec::new();
    for rule in pattern_rules() {
        for capture in rule.regex.find_iter(text) {
            findings.push(SecretFinding {
                kind: FindingKind::Pattern,
                label: rule.label.into(),
                start: capture.start(),
                end: capture.end(),
            });
        }
    }
    findings
}

fn scan_entropy(text: &str) -> Vec<SecretFinding> {
    let mut findings = Vec::new();
    for token in text.split_whitespace() {
        let trimmed = token.trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '`' | ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}'
            )
        });
        if trimmed.len() < 20 || trimmed.len() > 256 {
            continue;
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '=' | '+' | '/'))
        {
            continue;
        }
        if trimmed.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if shannon_entropy(trimmed) < 4.2 {
            continue;
        }
        if let Some(start) = text.find(trimmed) {
            findings.push(SecretFinding {
                kind: FindingKind::Entropy,
                label: "high-entropy token".into(),
                start,
                end: start + trimmed.len(),
            });
        }
    }
    findings
}

fn shannon_entropy(value: &str) -> f64 {
    let mut counts = [0u32; 256];
    for byte in value.bytes() {
        counts[byte as usize] += 1;
    }
    let length = value.len() as f64;
    counts
        .iter()
        .filter(|&&count| count > 0)
        .map(|&count| {
            let probability = count as f64 / length;
            -probability * probability.log2()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_api_key_assignment() {
        let result = scan_text("export API_KEY=example_api_key");
        assert!(!result.is_clean());
        assert!(
            result
                .findings
                .iter()
                .any(|f| f.label.contains("assignment"))
        );
    }

    #[test]
    fn detects_private_key_header() {
        let text = "-----BEGIN RSA PRIVATE KEY-----\nMIIE";
        let result = scan_text(text);
        assert!(result.findings.iter().any(|f| f.label == "private key"));
    }

    #[test]
    fn detects_github_token() {
        let result = scan_text("token ghp_1234567890abcdefghijklmnopqrstuvwxyz");
        assert!(result.findings.iter().any(|f| f.label == "GitHub token"));
    }

    #[test]
    fn clean_text_passes() {
        let result = scan_text("Please refactor the login module and add tests.");
        assert!(result.is_clean());
    }

    #[test]
    fn summary_lists_labels_without_secret_values() {
        let result = scan_text("password=hunter2");
        let summary = result.summary();
        assert!(summary.contains("finding"));
        assert!(!summary.contains("hunter2"));
    }
}
