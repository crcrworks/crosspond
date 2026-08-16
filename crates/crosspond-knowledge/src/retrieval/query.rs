const COMMAND_SUFFIXES: &[&str] = &["してください", "してほしい", "してね", "やって", "して"];

const QUESTION_SUFFIXES: &[&str] = &["でしょうか", "ですか", "って何", "ってなに", "とは", "かな"];

pub fn looks_like_command(prompt: &str) -> bool {
    let text = prompt.trim();
    if text.is_empty() {
        return false;
    }
    if text.ends_with('?') || text.ends_with('？') {
        return false;
    }
    let lower = text.to_lowercase();
    if lower.contains("って何")
        || lower.contains("とは")
        || lower.starts_with("what ")
        || lower.starts_with("what's ")
        || lower.starts_with("who ")
        || lower.starts_with("where ")
    {
        return false;
    }
    text.contains("して")
        || text.contains("やって")
        || text.contains("してほしい")
        || text.contains("ください")
        || text.ends_with('て')
}

pub fn looks_like_read_later(prompt: &str) -> bool {
    let lower = prompt.trim().to_lowercase();
    !lower.is_empty()
        && (lower.contains("あとで読む")
            || lower.contains("後で読む")
            || lower.contains("read later")
            || lower.contains("save for later")
            || lower.contains("save this page")
            || lower.contains("このページを保存"))
}

pub fn search_queries(prompt: &str) -> Vec<String> {
    let trimmed = prompt.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut queries = Vec::new();
    push_unique(&mut queries, trimmed.to_string());
    let stripped = strip_prompt_affixes(trimmed);
    push_unique(&mut queries, stripped.clone());
    if stripped.contains('の') {
        push_unique(&mut queries, stripped.replace('の', ""));
    }
    for token in ascii_tokens(&stripped) {
        push_unique(&mut queries, token);
    }
    queries
}

fn strip_prompt_affixes(text: &str) -> String {
    let mut stripped = text.trim().to_string();
    loop {
        let mut next = stripped
            .trim_end_matches(['。', '！', '!', '.', '?', '？'])
            .trim()
            .to_string();
        let mut changed = next != stripped;
        for suffix in QUESTION_SUFFIXES.iter().chain(COMMAND_SUFFIXES) {
            if let Some(base) = next.strip_suffix(suffix) {
                next = base.trim().to_string();
                changed = true;
                break;
            }
        }
        if !changed {
            return next;
        }
        stripped = next;
    }
}

fn ascii_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            if current.len() >= 2 {
                tokens.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if current.len() >= 2 {
        tokens.push(current);
    }
    tokens
}

fn push_unique(queries: &mut Vec<String>, query: String) {
    if query.is_empty() || queries.iter().any(|existing| existing == &query) {
        return;
    }
    queries.push(query);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_prompt_strips_shite() {
        let queries = search_queries("研究室の課題確認して");
        assert!(queries.iter().any(|query| query == "研究室の課題確認"));
        assert!(looks_like_command("研究室の課題確認して"));
        assert!(!looks_like_command("研究室のVPNって何?"));
        assert!(looks_like_read_later("このページをあとで読む"));
        assert!(looks_like_read_later("Save this page for later"));
        assert!(!looks_like_read_later("経費精算して"));
    }

    #[test]
    fn question_prompt_extracts_vpn_terms() {
        let queries = search_queries("研究室のVPNって何?");
        assert!(queries.iter().any(|query| query == "研究室のVPN"));
        assert!(queries.iter().any(|query| query == "研究室VPN"));
        assert!(queries.iter().any(|query| query == "VPN"));
    }
}
