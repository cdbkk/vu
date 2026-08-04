use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

/// Embeds vu's own icons (Phosphor) from `assets/icons/`.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets/icons"]
#[include = "**/*.svg"]
struct VuIcons;

/// Embeds top-level app images such as the macOS app icon PNG.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../assets"]
#[include = "*.png"]
struct VuImages;

/// Asset source that serves vu's icons first, then falls back to
/// gpui-component's bundled icons (Lucide).
pub struct VuAssets;

impl AssetSource for VuAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        // Try vu's own icons first
        if let Some(data) = VuIcons::get(path) {
            return Ok(Some(data.data));
        }

        if let Some(data) = VuImages::get(path) {
            return Ok(Some(data.data));
        }

        // Fall back to gpui-component's bundled assets
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut results: Vec<SharedString> = VuIcons::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| p.into())
            .collect();

        results.extend(
            VuImages::iter()
                .filter(|p| p.starts_with(path))
                .map(|p| p.into()),
        );

        if let Ok(mut component_results) = gpui_component_assets::Assets.list(path) {
            results.append(&mut component_results);
        }

        Ok(results)
    }
}
