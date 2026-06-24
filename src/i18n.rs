//! Localization support for `zallet-tui`.
//!
//! This mirrors the i18n setup used by Zallet itself: a Fluent language loader backed by
//! the `.ftl` assets embedded from the `i18n/` directory.

use std::sync::LazyLock;

use i18n_embed::{
    fluent::{FluentLanguageLoader, fluent_language_loader},
    unic_langid::LanguageIdentifier,
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n"]
struct Localizations;

/// The shared Fluent language loader used by the [`crate::fl`] macro.
pub(crate) static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> =
    LazyLock::new(|| fluent_language_loader!());

/// Selects the most suitable available language in order of preference by
/// `requested_languages`, and loads it using the [`static@LANGUAGE_LOADER`] from the
/// languages available in `i18n/`.
///
/// Returns the available languages that were negotiated as being the most suitable to be
/// selected, and were loaded by [`i18n_embed::select`].
pub(crate) fn load_languages(
    requested_languages: &[LanguageIdentifier],
) -> Vec<LanguageIdentifier> {
    let supported_languages =
        i18n_embed::select(&*LANGUAGE_LOADER, &Localizations, requested_languages)
            .expect("language loading should not fail for embedded assets");
    // Unfortunately the common Windows terminals don't support Unicode Directionality
    // Isolation Marks, so we disable them for now.
    LANGUAGE_LOADER.set_use_isolating(false);
    supported_languages
}
