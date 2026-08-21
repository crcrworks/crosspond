use crate::skill_safety::SafetyReport;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillEndpoints {
    pub search_url: String,
    pub github_api_url: String,
    pub github_raw_url: String,
    pub audit_url: Option<String>,
    pub validate_ssrf: bool,
}

impl Default for SkillEndpoints {
    fn default() -> Self {
        Self {
            search_url: "https://skills.sh/api/search".into(),
            github_api_url: "https://api.github.com".into(),
            github_raw_url: "https://raw.githubusercontent.com".into(),
            audit_url: Some("https://add-skill.vercel.sh/audit".into()),
            validate_ssrf: true,
        }
    }
}

impl SkillEndpoints {
    /// Point every skills HTTP client at a local mock. SSRF checks are off.
    pub fn for_local_mock(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            search_url: format!("{base}/api/search"),
            github_api_url: base.to_string(),
            github_raw_url: format!("{base}/raw"),
            audit_url: Some(format!("{base}/audit")),
            validate_ssrf: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkillFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedSkillInstall {
    pub name: String,
    pub source: String,
    pub files: Vec<SkillFile>,
    pub safety: SafetyReport,
    pub overwrite: bool,
}

impl PreparedSkillInstall {
    pub fn refuse_message(&self) -> String {
        self.safety.refuse_message("install")
    }

    pub fn approval_copy(&self) -> (String, String) {
        let title = format!("Install skill {} from {}", self.name, self.source);
        let mut description = format!("safety={}", self.safety.verdict.as_str());
        let categories = self.safety.category_list();
        if !categories.is_empty() {
            description.push_str(" findings=");
            description.push_str(&categories);
        }
        if self.overwrite {
            description.push_str(" (replaces the installed copy)");
        }
        (title, description)
    }

    pub fn needs_review(&self) -> bool {
        self.safety.verdict.needs_review()
    }

    pub fn is_fail(&self) -> bool {
        self.safety.is_fail()
    }
}
