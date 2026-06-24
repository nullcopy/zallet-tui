//! Small display-formatting helpers shared across views.

/// The number of leading characters of an identifier kept by [`short_uuid`].
const SHORT_UUID_LEN: usize = 8;

/// Shortens an account UUID to a stable prefix for compact display.
///
/// Operates on characters rather than bytes so it never splits a multi-byte code point,
/// even though wallet UUIDs are ASCII in practice.
pub(crate) fn short_uuid(uuid: &str) -> String {
    truncate_chars(uuid, SHORT_UUID_LEN)
}

/// Truncates `s` to at most `max` characters, appending an ellipsis when shortened.
///
/// The returned string is never longer than `max` characters (the ellipsis replaces the
/// final character). `max == 0` yields an empty string.
pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// Keeps at most the first `max` characters of `s`, with no ellipsis.
fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// The number of zatoshi in one ZEC.
pub(crate) const ZATOSHIS_PER_ZEC: u64 = 100_000_000;

/// Formats a zatoshi amount as a fixed-point ZEC decimal string with 8 fractional digits
/// and no unit suffix (e.g. `100000000` → `"1.00000000"`).
pub(crate) fn zec_decimal(zat: u64) -> String {
    let whole = zat / ZATOSHIS_PER_ZEC;
    let frac = zat % ZATOSHIS_PER_ZEC;
    format!("{whole}.{frac:08}")
}

/// Encodes bytes as a lowercase hexadecimal string (two characters per byte).
pub(crate) fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing a byte to a `String` is infallible.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_uuid_keeps_prefix() {
        assert_eq!(short_uuid("0123456789abcdef"), "01234567");
        assert_eq!(short_uuid("short"), "short");
        assert_eq!(short_uuid(""), "");
    }

    #[test]
    fn truncate_appends_ellipsis_only_when_shortened() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello", 4), "hel…");
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn truncate_is_char_aware() {
        // Five 2-byte code points: truncation must count characters, not bytes.
        let s = "ααααα";
        assert_eq!(truncate(s, 3), "αα…");
        assert_eq!(truncate(s, 5), s);
    }

    #[test]
    fn hex_encode_known_vectors() {
        assert_eq!(hex_encode(b""), "");
        assert_eq!(hex_encode(b"hello"), "68656c6c6f");
        assert_eq!(hex_encode(&[0x00, 0x0f, 0xff]), "000fff");
    }

    use proptest::prelude::*;

    proptest! {
        /// Truncation never exceeds the requested character budget, for any input.
        #[test]
        fn truncate_never_exceeds_max(s in ".*", max in 0usize..64) {
            prop_assert!(truncate(&s, max).chars().count() <= max);
        }

        /// Strings already within budget are returned unchanged.
        #[test]
        fn truncate_is_identity_within_budget(s in ".*", slack in 0usize..16) {
            let max = s.chars().count() + slack;
            prop_assert_eq!(truncate(&s, max), s);
        }

        /// A shortened UUID is always a character-prefix of the input, capped at 8 chars.
        #[test]
        fn short_uuid_is_a_capped_prefix(s in ".*") {
            let out = short_uuid(&s);
            prop_assert!(out.chars().count() <= 8);
            prop_assert!(s.starts_with(&out));
        }

        /// Hex encoding doubles the length and decodes back to the original bytes.
        #[test]
        fn hex_encode_round_trips(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
            let hex = hex_encode(&bytes);
            prop_assert_eq!(hex.len(), bytes.len() * 2);
            let decoded: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            prop_assert_eq!(decoded, bytes);
        }

        /// Fixed-point ZEC formatting round-trips: recombining the whole and fractional
        /// parts recovers the exact zatoshi input, for every `u64`.
        #[test]
        fn zec_decimal_round_trips(zat in any::<u64>()) {
            let s = zec_decimal(zat);
            let (whole, frac) = s.split_once('.').expect("formatted amount has a dot");
            prop_assert_eq!(frac.len(), 8);
            let recovered =
                whole.parse::<u64>().unwrap() * ZATOSHIS_PER_ZEC + frac.parse::<u64>().unwrap();
            prop_assert_eq!(recovered, zat);
        }
    }
}
