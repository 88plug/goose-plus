pub(super) mod gemini;
pub(super) mod open_plugins;

use std::path::Path;

/// True when a checkout carries a plugin any adapter can install. Used to tell
/// a marketplace-only repository apart from one that also ships a plugin.
pub(super) fn has_installable_plugin(checkout_dir: &Path) -> bool {
    open_plugins::has_plugin(checkout_dir) || checkout_dir.join(gemini::MANIFEST).is_file()
}
