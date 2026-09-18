use super::*;
use crate::backend::runtime::AppError;

fn run_batch_items<T, BeforeItem, ApplyItem>(
    asset_ids: &[String],
    before_item: &mut BeforeItem,
    mut apply_item: ApplyItem,
) -> AppResult<(Vec<T>, Vec<BatchMountItemError>)>
where
    BeforeItem: FnMut(usize, usize, &str) -> AppResult<()>,
    ApplyItem: FnMut(&str) -> AppResult<T>,
{
    let total = asset_ids.len();
    let mut results = Vec::new();
    let mut errors = Vec::new();
    for (index, asset_id) in asset_ids.iter().enumerate() {
        before_item(index, total, asset_id)?;
        match apply_item(asset_id) {
            Ok(result) => results.push(result),
            Err(error) => errors.push(BatchMountItemError {
                asset_id: asset_id.clone(),
                message: error.to_string(),
            }),
        }
    }
    Ok((results, errors))
}

#[test]
fn batch_cancel_is_checked_before_the_next_item() {
    let asset_ids = vec![
        "asset-a".to_string(),
        "asset-b".to_string(),
        "asset-c".to_string(),
    ];
    let mut started = Vec::new();
    let result = run_batch_items(
        &asset_ids,
        &mut |index, _, _| {
            if index == 1 {
                return Err(AppError::Cancelled("batch cancelled".to_string()));
            }
            Ok(())
        },
        |asset_id| {
            started.push(asset_id.to_string());
            Ok::<_, AppError>(asset_id.to_string())
        },
    );

    assert!(matches!(result, Err(AppError::Cancelled(_))));
    assert_eq!(started, vec!["asset-a"]);
}
