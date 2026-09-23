//! Find files on disk. The `fff-search` file picker with frecency lands
//! in phase 2 (SPEC §11).

use corvo_core::{Action, Command, CommandError, ExecutionContext, SearchContext, SearchResult};

#[derive(Default)]
pub struct FileSearchCommand;

corvo_core::register_command!(FileSearchCommand);

#[async_trait::async_trait]
impl Command for FileSearchCommand {
    fn id(&self) -> &'static str {
        "file-search"
    }

    fn keywords(&self) -> &'static [&'static str] {
        &["file", "files", "find", "documents"]
    }

    async fn search(&self, _query: &str, _ctx: &SearchContext) -> Vec<SearchResult> {
        // Phase 2: fff-search FilePicker over the home directory with a
        // background index task feeding a cache.
        Vec::new()
    }

    async fn execute(&self, _result_id: &str, _ctx: &ExecutionContext) -> Result<Action, CommandError> {
        Err(CommandError::Unsupported)
    }
}
