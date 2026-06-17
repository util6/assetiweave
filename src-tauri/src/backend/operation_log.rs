use crate::backend::{
    dto::{AssetMountStatus, PhysicalMountStateDto, SourceInput},
    models::{Asset, Source, TargetProfile},
};

pub(crate) type LogField = (&'static str, String);

pub(crate) fn log_info(operation: &str, message: &str, fields: &[LogField]) {
    crate::backend::logs::record_info(operation, message, fields);
}

pub(crate) fn log_warn(operation: &str, message: &str, fields: &[LogField]) {
    crate::backend::logs::record_warn(operation, message, fields);
}

pub(crate) fn log_error<E: std::fmt::Display + ?Sized>(
    operation: &str,
    message: &str,
    error: &E,
    fields: &[LogField],
) {
    let mut fields = fields.to_vec();
    fields.push(("error", error.to_string()));
    crate::backend::logs::record_error(operation, message, &fields);
}

pub(crate) fn source_input_log_fields(source: &SourceInput) -> Vec<LogField> {
    let mut fields = vec![
        ("name", source.name.clone()),
        ("root_path", source.root_path.clone()),
        ("kind", format!("{:?}", source.kind)),
        (
            "scanner_kind",
            source
                .scanner_kind
                .map(|kind| format!("{kind:?}"))
                .unwrap_or_else(|| "Mixed".to_string()),
        ),
        (
            "source_origin",
            source
                .source_origin
                .map(|origin| format!("{origin:?}"))
                .unwrap_or_else(|| "LocalFolder".to_string()),
        ),
        ("enabled", source.enabled.to_string()),
        ("priority", source.priority.to_string()),
    ];
    if let Some(id) = &source.id {
        fields.push(("source_id", id.clone()));
    }
    if let Some(origin_app_kind) = source.origin_app_kind {
        fields.push(("origin_app_kind", format!("{origin_app_kind:?}")));
    }
    fields
}

pub(crate) fn source_log_fields(source: &Source) -> Vec<LogField> {
    let mut fields = vec![
        ("source_id", source.id.clone()),
        ("name", source.name.clone()),
        ("root_path", source.root_path.clone()),
        ("scanner_kind", format!("{:?}", source.scanner_kind)),
        ("source_origin", format!("{:?}", source.source_origin)),
        ("enabled", source.enabled.to_string()),
        ("priority", source.priority.to_string()),
    ];
    if let Some(origin_app_kind) = source.origin_app_kind {
        fields.push(("origin_app_kind", format!("{origin_app_kind:?}")));
    }
    if let Some(status) = &source.last_scan_status {
        fields.push(("last_scan_status", status.clone()));
    }
    fields
}

pub(crate) fn asset_log_fields(asset: &Asset) -> Vec<LogField> {
    vec![
        ("asset_id", asset.id.clone()),
        ("skill_name", asset.name.clone()),
        ("source_id", asset.source_id.clone()),
        ("asset_kind", format!("{:?}", asset.kind)),
        ("relative_path", asset.relative_path.clone()),
        ("absolute_path", asset.absolute_path.clone()),
    ]
}

pub(crate) fn profile_log_fields(profile: &TargetProfile) -> Vec<LogField> {
    vec![
        ("profile_id", profile.id.clone()),
        ("profile_name", profile.name.clone()),
        ("app_kind", format!("{:?}", profile.app_kind)),
        ("enabled", profile.enabled.to_string()),
        (
            "deployment_strategy",
            format!("{:?}", profile.deployment_strategy),
        ),
        ("target_paths", profile.target_paths.join(",")),
    ]
}

pub(crate) fn status_summary_fields(statuses: &[AssetMountStatus]) -> Vec<LogField> {
    let mounted = statuses
        .iter()
        .filter(|status| status.state == PhysicalMountStateDto::Mounted)
        .count();
    let issues = statuses
        .iter()
        .filter(|status| {
            matches!(
                status.state,
                PhysicalMountStateDto::Conflict | PhysicalMountStateDto::Broken
            )
        })
        .count();

    vec![
        ("count", statuses.len().to_string()),
        ("mounted", mounted.to_string()),
        ("issues", issues.to_string()),
    ]
}
