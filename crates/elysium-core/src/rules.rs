//! Evaluation of Mojang's library/argument `rules`.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Regex matched against the OS version string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

/// Operating system as named in Mojang metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsName {
    Windows,
    Osx,
    Linux,
}

impl OsName {
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Osx
        } else {
            Self::Linux
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Osx => "osx",
            Self::Linux => "linux",
        }
    }
}

/// CPU architecture of the JVM that will run the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86,
    X86_64,
    Arm64,
}

impl Arch {
    pub fn current() -> Self {
        if cfg!(target_arch = "x86") {
            Self::X86
        } else if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else {
            Self::X86_64
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::X86_64 => "x86_64",
            Self::Arm64 => "arm64",
        }
    }

    /// Value substituted for `${arch}` in legacy native classifiers.
    pub fn bits(self) -> &'static str {
        match self {
            Self::X86 => "32",
            Self::X86_64 | Self::Arm64 => "64",
        }
    }
}

/// The environment rules are evaluated against.
#[derive(Debug, Clone)]
pub struct Environment {
    pub os: OsName,
    pub os_version: String,
    pub arch: Arch,
    pub features: HashMap<String, bool>,
}

impl Environment {
    pub fn current() -> Self {
        Self {
            os: OsName::current(),
            os_version: os_version(),
            arch: Arch::current(),
            features: HashMap::new(),
        }
    }

    pub fn with_feature(mut self, name: &str, enabled: bool) -> Self {
        self.features.insert(name.to_owned(), enabled);
        self
    }

    fn feature(&self, name: &str) -> bool {
        self.features.get(name).copied().unwrap_or(false)
    }
}

fn os_version() -> String {
    // Only used for the macOS 10.5 rules in very old versions, so a best effort is fine.
    std::env::var("ELYSIUM_OS_VERSION").unwrap_or_default()
}

impl Rule {
    fn matches(&self, env: &Environment) -> bool {
        if let Some(os) = &self.os {
            if let Some(name) = &os.name
                && name != env.os.as_str()
            {
                return false;
            }
            if let Some(arch) = &os.arch
                && arch != env.arch.as_str()
            {
                return false;
            }
            if let Some(pattern) = &os.version {
                match regex::Regex::new(pattern) {
                    Ok(re) if re.is_match(&env.os_version) => {}
                    _ => return false,
                }
            }
        }
        if let Some(features) = &self.features
            && features
                .iter()
                .any(|(name, want)| env.feature(name) != *want)
        {
            return false;
        }
        true
    }
}

/// Applies `rules` in order; the last matching rule wins. No rules means "allowed".
pub fn is_allowed(rules: &[Rule], env: &Environment) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for rule in rules {
        if rule.matches(env) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(os: OsName, arch: Arch) -> Environment {
        Environment {
            os,
            os_version: "10.0".into(),
            arch,
            features: HashMap::new(),
        }
    }

    fn rules(json: &str) -> Vec<Rule> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn empty_rules_allow() {
        assert!(is_allowed(&[], &env(OsName::Linux, Arch::X86_64)));
    }

    #[test]
    fn os_allow_only() {
        let r = rules(r#"[{"action":"allow","os":{"name":"osx"}}]"#);
        assert!(is_allowed(&r, &env(OsName::Osx, Arch::Arm64)));
        assert!(!is_allowed(&r, &env(OsName::Windows, Arch::X86_64)));
    }

    #[test]
    fn allow_then_disallow() {
        let r = rules(r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx"}}]"#);
        assert!(is_allowed(&r, &env(OsName::Windows, Arch::X86_64)));
        assert!(!is_allowed(&r, &env(OsName::Osx, Arch::X86_64)));
    }

    #[test]
    fn os_version_regex() {
        let r = rules(
            r#"[{"action":"allow"},{"action":"disallow","os":{"name":"osx","version":"^10\\.5\\.\\d$"}}]"#,
        );
        let mut e = env(OsName::Osx, Arch::X86_64);
        assert!(is_allowed(&r, &e));
        e.os_version = "10.5.8".into();
        assert!(!is_allowed(&r, &e));
    }

    #[test]
    fn arch_rule() {
        let r = rules(r#"[{"action":"allow","os":{"arch":"x86"}}]"#);
        assert!(is_allowed(&r, &env(OsName::Windows, Arch::X86)));
        assert!(!is_allowed(&r, &env(OsName::Windows, Arch::X86_64)));
    }

    #[test]
    fn features() {
        let r = rules(r#"[{"action":"allow","features":{"has_custom_resolution":true}}]"#);
        let e = env(OsName::Windows, Arch::X86_64);
        assert!(!is_allowed(&r, &e));
        assert!(is_allowed(
            &r,
            &e.with_feature("has_custom_resolution", true)
        ));
    }
}
