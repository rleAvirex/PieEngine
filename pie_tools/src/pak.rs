/// Re-export the shared pak types from pie_runtime.
///
/// The pak format types live in `pie_runtime` so both the cooker (pie_tools) and
/// the runtime (pie_runtime) can read/write the same binary format.
pub use pie_runtime::assets::{CookedAssetKind, PakAsset, PakFile};
