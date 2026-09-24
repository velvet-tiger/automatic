use crate::core;

// ── Contexts ─────────────────────────────────────────────────────────────────
//
// Thin wrappers over `core::contexts` and `core::context_sources`. Contexts
// are delivered to agents through MCP only, so none of these trigger a
// project sync.

#[tauri::command]
pub fn list_contexts() -> Result<Vec<core::Context>, String> {
    let mut contexts = Vec::new();
    for slug in core::list_contexts()? {
        match core::read_context(&slug) {
            Ok(context) => contexts.push(context),
            // One malformed file should not hide the rest of the library.
            Err(e) => eprintln!("contexts: skipping '{}': {}", slug, e),
        }
    }
    Ok(contexts)
}

#[tauri::command]
pub fn read_context(slug: &str) -> Result<core::Context, String> {
    core::read_context(slug)
}

/// Create or update a context. Returns it with timestamps filled in.
#[tauri::command]
pub fn save_context(context: core::Context) -> Result<core::Context, String> {
    core::save_context(context)
}

#[tauri::command]
pub fn delete_context(slug: &str) -> Result<core::ContextReferences, String> {
    core::delete_context(slug)
}

#[tauri::command]
pub fn rename_context(old_slug: &str, new_slug: &str) -> Result<core::ContextReferences, String> {
    core::rename_context(old_slug, new_slug)
}

#[tauri::command]
pub fn get_context_references(slug: &str) -> Result<core::ContextReferences, String> {
    core::find_context_references(slug)
}

#[tauri::command]
pub fn attach_context(target: core::ContextTarget, slug: &str) -> Result<bool, String> {
    core::attach_context(&target, slug)
}

#[tauri::command]
pub fn detach_context(target: core::ContextTarget, slug: &str) -> Result<bool, String> {
    core::detach_context(&target, slug)
}

// ── Documentation pages ──────────────────────────────────────────────────────

#[tauri::command]
pub fn list_context_documentation_pages(slug: &str, source_id: &str) -> Result<Vec<String>, String> {
    core::list_documentation_pages(slug, source_id)
}

#[tauri::command]
pub fn read_context_documentation_page(
    slug: &str,
    source_id: &str,
    path: &str,
) -> Result<String, String> {
    core::read_documentation_page(slug, source_id, path)
}

#[tauri::command]
pub fn write_context_documentation_page(
    slug: &str,
    source_id: &str,
    path: &str,
    content: &str,
) -> Result<(), String> {
    core::write_documentation_page(slug, source_id, path, content)
}

#[tauri::command]
pub fn delete_context_documentation_page(
    slug: &str,
    source_id: &str,
    path: &str,
) -> Result<(), String> {
    core::delete_documentation_page(slug, source_id, path)
}

#[tauri::command]
pub fn move_context_documentation_page(
    slug: &str,
    source_id: &str,
    from: &str,
    to: &str,
) -> Result<(), String> {
    core::move_documentation_page(slug, source_id, from, to)
}

// ── Source preview ───────────────────────────────────────────────────────────

#[tauri::command]
pub async fn list_context_sources(slug: String) -> Result<Vec<core::SourceSummary>, String> {
    let context = core::read_context(&slug)?;
    core::list_context_sources(&context).await
}

#[tauri::command]
pub async fn list_context_source_entries(
    slug: String,
    source_id: String,
) -> Result<core::SourceListing, String> {
    let context = core::read_context(&slug)?;
    core::list_source_entries(&context, &source_id).await
}

#[tauri::command]
pub async fn read_context_source_entry(
    slug: String,
    source_id: String,
    path: String,
) -> Result<String, String> {
    let context = core::read_context(&slug)?;
    core::read_source_entry(&context, &source_id, &path).await
}
