use super::prelude::*;

trait AssetScanner {
    fn scan(&self, source: &Source) -> InfraResult<Vec<Asset>>;
}

struct SkillScanner;
struct MixedScanner;

pub(crate) fn scan_source(source: &Source) -> InfraResult<Vec<Asset>> {
    match source.scanner_kind {
        SourceScannerKind::Skill => SkillScanner.scan(source),
        _ => MixedScanner.scan(source),
    }
}

pub(crate) fn scan_skill_source(source: &Source) -> InfraResult<Vec<Asset>> {
    SkillScanner.scan(source)
}

impl AssetScanner for SkillScanner {
    fn scan(&self, source: &Source) -> InfraResult<Vec<Asset>> {
        skill::scan_skill_assets(source)
    }
}

impl AssetScanner for MixedScanner {
    fn scan(&self, source: &Source) -> InfraResult<Vec<Asset>> {
        mixed::scan_mixed_assets(source)
    }
}
