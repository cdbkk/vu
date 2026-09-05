use super::{LocalPathCompletion, longest_common_prefix};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn path_token(input: &str) -> Option<(usize, &str, bool)> {
    let token_start = input
        .char_indices()
        .rev()
        .find_map(|(idx, ch)| ch.is_whitespace().then_some(idx + ch.len_utf8()))
        .unwrap_or(0);
    let token = &input[token_start..];
    if token.is_empty() {
        return None;
    }

    let head = input[..token_start].trim_end();
    let first_word = head.split_whitespace().next().unwrap_or_default();
    let completes_path = first_word == "cd"
        || token.starts_with('~')
        || token.starts_with('.')
        || token.contains('/');
    if !completes_path {
        return None;
    }

    Some((token_start, token, first_word == "cd"))
}

pub(super) fn complete_path(
    cwd: &str,
    input: &str,
    cancelled: &AtomicBool,
) -> Option<LocalPathCompletion> {
    if cancelled.load(Ordering::Relaxed) {
        return None;
    }
    let (token_start, token, directories_only) = path_token(input)?;
    let home_dir = dirs::home_dir();
    let (search_dir, dir_prefix, search_prefix) = if let Some(stripped) = token.strip_prefix("~/") {
        let home = home_dir?;
        match stripped.rsplit_once('/') {
            Some((dir, prefix)) => (home.join(dir), format!("~/{dir}/"), prefix.to_string()),
            None => (home, "~/".to_string(), stripped.to_string()),
        }
    } else if token == "~" {
        let home = home_dir?;
        (home, String::new(), "~".to_string())
    } else if let Some((dir, prefix)) = token.rsplit_once('/') {
        let base = if dir.is_empty() {
            PathBuf::from("/")
        } else if Path::new(dir).is_absolute() {
            PathBuf::from(dir)
        } else {
            PathBuf::from(&cwd).join(dir)
        };
        (base, format!("{dir}/"), prefix.to_string())
    } else {
        (PathBuf::from(&cwd), String::new(), token.to_string())
    };

    let mut matches = std::fs::read_dir(&search_dir)
        .ok()?
        .take_while(|_| !cancelled.load(Ordering::Relaxed))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            if directories_only && !file_type.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            name.starts_with(&search_prefix)
                .then_some((name, file_type.is_dir()))
        })
        .collect::<Vec<_>>();
    if cancelled.load(Ordering::Relaxed) || matches.is_empty() {
        return None;
    }
    matches.sort_by(|a, b| a.0.cmp(&b.0));

    let matched_name = if matches.len() == 1 {
        let (name, is_dir) = &matches[0];
        let mut single = name.clone();
        if *is_dir {
            single.push('/');
        }
        single
    } else {
        let prefix = longest_common_prefix(matches.iter().map(|(name, _)| name.as_str()));
        if prefix.chars().count() <= search_prefix.chars().count() {
            let candidates = matches
                .into_iter()
                .map(|(name, is_dir)| {
                    let mut candidate = if token == "~" {
                        name
                    } else {
                        format!("{dir_prefix}{name}")
                    };
                    if is_dir {
                        candidate.push('/');
                    }
                    format!("{}{}", &input[..token_start], candidate)
                })
                .collect::<Vec<_>>();
            return Some(LocalPathCompletion::Candidates(candidates));
        }
        prefix
    };

    let completed_token = if token == "~" {
        matched_name
    } else {
        format!("{dir_prefix}{matched_name}")
    };

    Some(LocalPathCompletion::Inline(format!(
        "{}{}",
        &input[..token_start],
        completed_token
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn completion_preserves_prefixes_order_unicode_and_directory_filtering() {
        let root = std::env::temp_dir().join(format!("vu-completion-{}", std::process::id()));
        fs::create_dir_all(root.join("alpha")).unwrap();
        fs::create_dir_all(root.join("alpine")).unwrap();
        fs::create_dir_all(root.join("ไทย")).unwrap();
        fs::write(root.join("albatross.txt"), "").unwrap();
        let cwd = root.to_str().unwrap();
        let active = AtomicBool::new(false);

        assert_eq!(
            complete_path(cwd, "cd a", &active),
            Some(LocalPathCompletion::Inline("cd alp".into()))
        );
        assert_eq!(
            complete_path(cwd, "cd alp", &active),
            Some(LocalPathCompletion::Candidates(vec![
                "cd alpha/".into(),
                "cd alpine/".into()
            ]))
        );
        assert_eq!(
            complete_path(cwd, "cat ./al", &active),
            Some(LocalPathCompletion::Candidates(vec![
                "cat ./albatross.txt".into(),
                "cat ./alpha/".into(),
                "cat ./alpine/".into()
            ]))
        );
        assert_eq!(
            complete_path(cwd, "cd ไ", &active),
            Some(LocalPathCompletion::Inline("cd ไทย/".into()))
        );
        assert_eq!(complete_path(cwd, "git sta", &active), None);
        assert_eq!(complete_path(cwd, "cd missing", &active), None);
        assert_eq!(complete_path(cwd, "cd a", &AtomicBool::new(true)), None);
        fs::remove_dir_all(root).unwrap();
    }
}
