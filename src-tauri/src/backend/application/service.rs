use super::prelude::*;

pub(crate) struct AppService {
    pub(super) db: crate::backend::store::Database,
    pub(super) db_path: PathBuf,
}
