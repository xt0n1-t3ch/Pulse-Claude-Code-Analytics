#[cfg(windows)]
use std::process::Stdio;

#[cfg(any(windows, test))]
const OPENCODE_PROCESS_NAMES: [&str; 3] = ["OpenCode.exe", "opencode.exe", "opencode-cli.exe"];
#[cfg(any(windows, test))]
const CODEX_APP_PROCESS_NAME: &str = "Codex.exe";

pub fn is_opencode_running() -> bool {
    is_opencode_running_impl()
}

pub fn is_codex_app_running() -> bool {
    is_codex_app_running_impl()
}

pub fn is_desktop_surface_running() -> bool {
    is_opencode_running() || is_codex_app_running()
}

#[cfg(windows)]
fn is_opencode_running_impl() -> bool {
    read_tasklist().is_some_and(|text| tasklist_has_opencode(&text))
}

#[cfg(not(windows))]
fn is_opencode_running_impl() -> bool {
    false
}

#[cfg(windows)]
fn is_codex_app_running_impl() -> bool {
    read_tasklist().is_some_and(|text| tasklist_has_codex_app(&text))
}

#[cfg(not(windows))]
fn is_codex_app_running_impl() -> bool {
    false
}

#[cfg(windows)]
fn read_tasklist() -> Option<String> {
    let output = crate::util::silent_command("tasklist")
        .arg("/FO")
        .arg("CSV")
        .arg("/NH")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output();

    let Ok(output) = output else {
        return None;
    };
    if !output.status.success() {
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(any(windows, test))]
pub(crate) fn tasklist_has_opencode(output: &str) -> bool {
    output.lines().any(|line| {
        let Some(name) = tasklist_image_name(line) else {
            return false;
        };
        OPENCODE_PROCESS_NAMES
            .iter()
            .any(|expected| name.eq_ignore_ascii_case(expected))
    })
}

#[cfg(any(windows, test))]
pub(crate) fn tasklist_has_codex_app(output: &str) -> bool {
    output.lines().any(|line| {
        let Some(name) = tasklist_image_name(line) else {
            return false;
        };
        name == CODEX_APP_PROCESS_NAME
    })
}

#[cfg(any(windows, test))]
fn tasklist_image_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed
            .eq_ignore_ascii_case("INFO: No tasks are running which match the specified criteria.")
    {
        return None;
    }

    let name = tasklist_csv_image_name(trimmed)
        .unwrap_or_else(|| trimmed.split_whitespace().next().unwrap_or(trimmed));
    if name.is_empty() || name.eq_ignore_ascii_case("Image Name") {
        None
    } else {
        Some(name)
    }
}

#[cfg(any(windows, test))]
fn tasklist_csv_image_name(line: &str) -> Option<&str> {
    if let Some(rest) = line.strip_prefix('"') {
        let end = rest.find('"')?;
        return Some(&rest[..end]);
    }

    let mut fields = line.split(',');
    let first = fields.next()?.trim();
    let second = fields.next()?.trim();
    if fields.count() < 3 {
        return None;
    }

    if second.eq_ignore_ascii_case("PID") || second.parse::<u32>().is_ok() {
        Some(first)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasklist_parser_detects_opencode_names_case_insensitively() {
        let output = r#"
"Image Name","PID","Session Name","Session#","Mem Usage"
"OpenCode.exe","1234","Console","1","42,000 K"
"opencode-cli.exe","2345","Console","1","10,000 K"
"#;

        assert!(tasklist_has_opencode(output));
    }

    #[test]
    fn tasklist_parser_rejects_partial_names() {
        let output = r#"
"not-opencode.exe","1234","Console","1","42,000 K"
"opencode-helper.exe","2345","Console","1","10,000 K"
"#;

        assert!(!tasklist_has_opencode(output));
    }

    #[test]
    fn tasklist_parser_supports_table_output() {
        let output = r#"
Image Name                     PID Session Name        Session#    Mem Usage
========================= ======== ================ =========== ============
opencode.exe                  7777 Console                    1     12,000 K
"#;

        assert!(tasklist_has_opencode(output));
    }

    #[test]
    fn tasklist_parser_detects_official_codex_app() {
        let output = r#"
"Image Name","PID","Session Name","Session#","Mem Usage"
"Codex.exe","1234","Console","1","148,000 K"
"codex.exe","2345","Console","1","30,000 K"
"#;

        assert!(tasklist_has_codex_app(output));
    }
}
