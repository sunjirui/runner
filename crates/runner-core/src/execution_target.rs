use serde::{Deserialize, Serialize};

/// Where an agent executes. Runner and its authoritative state stay on the host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutionTarget {
    #[default]
    Native,
    Wsl {
        distro: String,
        user: String,
    },
}

impl ExecutionTarget {
    pub fn is_native(&self) -> bool {
        matches!(self, Self::Native)
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Self::Wsl { distro, user } = self {
            for (name, value) in [("distribution", distro), ("Linux user", user)] {
                if value.is_empty()
                    || value.trim() != value
                    || value.starts_with('-')
                    || matches!(value.as_str(), "." | "..")
                    || value
                        .chars()
                        .any(|c| matches!(c, ':' | '<' | '>' | '"' | '|' | '?' | '*'))
                    || value
                        .chars()
                        .any(|c| c.is_control() || matches!(c, '/' | '\\'))
                {
                    return Err(format!("invalid WSL {name}: {value:?}"));
                }
            }
        }
        Ok(())
    }

    /// Only the executable boundary adds these flags; no shell parses user argv.
    pub fn wsl_args(&self, cwd: Option<&str>) -> Result<Vec<String>, String> {
        self.validate()?;
        let Self::Wsl { distro, user } = self else {
            return Err("native target has no WSL command".into());
        };
        let mut args = vec![
            "--distribution".into(),
            distro.clone(),
            "--user".into(),
            user.clone(),
        ];
        if let Some(cwd) = cwd {
            validate_linux_path(cwd)?;
            args.extend(["--cd".into(), cwd.into()]);
        }
        args.push("--exec".into());
        Ok(args)
    }

    /// A host view of a Linux path, not a path to pass to an agent.
    pub fn host_path(&self, linux_path: &str) -> Result<String, String> {
        self.validate()?;
        let Self::Wsl { distro, .. } = self else {
            return Ok(linux_path.to_owned());
        };
        validate_linux_path(linux_path)?;
        if linux_path.contains('\\') || linux_path.contains(':') {
            return Err("Linux path cannot be represented through WSL UNC".into());
        }
        Ok(format!(
            r"\\wsl.localhost\{distro}{}",
            linux_path.replace('/', r"\")
        ))
    }

    pub fn label(&self) -> String {
        match self {
            Self::Native => "Native".into(),
            Self::Wsl { distro, user } => format!("WSL2 · {distro} · {user}"),
        }
    }
}

pub fn validate_linux_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains('\0') {
        return Err("WSL working directory must be an absolute Linux path".into());
    }
    if path.split('/').any(|part| part == ".." || part == ".") {
        return Err("WSL path must be resolved before crossing the host boundary".into());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WslDistribution {
    pub name: String,
    pub version: u8,
    pub state: String,
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WslIntegration {
    pub target: ExecutionTarget,
    pub home: String,
    pub path: String,
    pub agents: std::collections::BTreeMap<String, String>,
    pub build: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WslStatus {
    pub distributions: Vec<WslDistribution>,
    pub integrations: Vec<WslIntegration>,
}

/// `wsl --list --verbose` is UTF-16LE on redirected stdout. Parse columns
/// from the right: names may contain spaces and state names are localized.
pub fn parse_distributions(bytes: &[u8]) -> Result<Vec<WslDistribution>, String> {
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.contains(&0) {
        if !bytes.len().is_multiple_of(2) {
            return Err("truncated UTF-16 WSL distribution list".into());
        }
        String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect::<Vec<_>>(),
        )
        .map_err(|e| e.to_string())?
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|e| e.to_string())?
    };
    let mut rows = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (prefix, version) = line
            .rsplit_once(char::is_whitespace)
            .ok_or("invalid WSL distribution row")?;
        let version = version.parse::<u8>().map_err(|_| "invalid WSL version")?;
        let prefix = prefix.trim_end();
        let (name, state) = prefix
            .rsplit_once("  ")
            .or_else(|| prefix.rsplit_once('\t'))
            .or_else(|| prefix.rsplit_once(char::is_whitespace))
            .ok_or("invalid WSL distribution state")?;
        let name = name.trim();
        let default = name.starts_with('*');
        let name = name.trim_start_matches('*').trim().to_owned();
        if name.is_empty() {
            return Err("missing WSL distribution name".into());
        }
        rows.push(WslDistribution {
            name,
            version,
            state: state.trim().into(),
            default,
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_round_trip_and_keep_identity() {
        let a = ExecutionTarget::Wsl {
            distro: "Ubuntu".into(),
            user: "me".into(),
        };
        assert_eq!(
            serde_json::from_str::<ExecutionTarget>(&serde_json::to_string(&a).unwrap()).unwrap(),
            a
        );
        assert_ne!(
            a,
            ExecutionTarget::Wsl {
                distro: "Debian".into(),
                user: "me".into()
            }
        );
        assert_ne!(
            a,
            ExecutionTarget::Wsl {
                distro: "Ubuntu".into(),
                user: "root".into()
            }
        );
        assert!(
            serde_json::from_str::<ExecutionTarget>(r#"{"kind":"wsl","distro":"Ubuntu"}"#).is_err()
        );
    }

    #[test]
    fn argv_and_paths_preserve_spaces_case_unicode_and_metacharacters() {
        let target = ExecutionTarget::Wsl {
            distro: "My Ubuntu".into(),
            user: "me".into(),
        };
        let cwd = "/home/me/项目 A;$(echo nope)";
        assert_eq!(
            target.wsl_args(Some(cwd)).unwrap(),
            [
                "--distribution",
                "My Ubuntu",
                "--user",
                "me",
                "--cd",
                cwd,
                "--exec"
            ]
        );
        assert_eq!(
            target.host_path(cwd).unwrap(),
            r"\\wsl.localhost\My Ubuntu\home\me\项目 A;$(echo nope)"
        );
        assert_ne!(
            target.host_path("/repo/A").unwrap(),
            target.host_path("/repo/a").unwrap()
        );
        for bad in [
            "relative",
            "C:\\repo",
            "/home/../root",
            "/a\0b",
            "//host/share",
        ] {
            assert!(target.wsl_args(Some(bad)).is_err(), "{bad:?}");
        }
        assert!(ExecutionTarget::Wsl {
            distro: "../Ubuntu".into(),
            user: "me".into()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn redirected_distribution_list_is_localized_utf16() {
        let text = "\u{feff}  NAME            STATE     VERSION\r\n* My Ubuntu       Running   2\r\n  Debian          已停止    1\r\n";
        let bytes = text
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>();
        let rows = parse_distributions(&bytes).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0],
            WslDistribution {
                name: "My Ubuntu".into(),
                version: 2,
                state: "Running".into(),
                default: true
            }
        );
        assert_eq!(rows[1].state, "已停止");
        assert_eq!(rows[1].version, 1);
        assert!(parse_distributions(&bytes[..bytes.len() - 1]).is_err());
    }
}
