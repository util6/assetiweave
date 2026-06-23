use super::prelude::*;

pub(super) fn build_glob_set(patterns: &[String], fallback: &[&str]) -> AppResult<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let effective: Vec<String> = if patterns.is_empty() {
        fallback
            .iter()
            .map(|pattern| (*pattern).to_string())
            .collect()
    } else {
        patterns.to_vec()
    };
    for pattern in effective {
        builder.add(Glob::new(&pattern).map_err(|error| error.to_string())?);
    }
    builder.build().map_err(|error| error.to_string())
}
