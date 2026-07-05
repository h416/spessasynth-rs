/// fill_with_defaults.rs
/// purpose: Merge an optional partial options value with a set of defaults.
/// Ported from: src/utils/fill_with_defaults.ts (spessasynth_core 4.3.0, new file; the same
/// content already existed unported in 4.2.0)
///
/// TypeScript's `fillWithDefaults<T>(obj: Partial<T> | undefined, defObj: T): T` spreads
/// `defObj` first and then `obj` on top of it, so any field present on `obj` overrides the
/// corresponding default, while fields missing from `obj` (or `obj` being `undefined`
/// entirely) fall back to `defObj`.
///
/// Rust has no runtime `Partial<T>` equivalent: struct fields are always fully present.
/// The idiomatic Rust way to express "some fields overridden, others defaulted" is to build
/// the full value at the call site with struct-update syntax
/// (`MyOptions { field: value, ..MyOptions::default() }`), which already performs the
/// per-field merge that TypeScript does at runtime. By the time such a value reaches this
/// function it is a *complete* `T`, so `fillWithDefaults` only needs to choose between that
/// complete override (`Some`) and the plain default (`None`) — matching how call sites in
/// this codebase already handle optional `...Options` parameters (see `audio_to_wav` in
/// `write_wav.rs`).
///
/// As of this port, no caller in the Rust codebase uses this function yet (the TS call sites
/// — `basic_midi.ts`, `midi_builder.ts`, `downloadable_sounds.ts`, `soundfont/write/write.ts`,
/// `synthesizer/processor.ts` — belong to files ported in later phase-2 tasks). It is added
/// now, self-contained and unit-tested, so those later tasks can adopt it directly.
///
/// Equivalent to: `fillWithDefaults<T>(obj, defObj)`
pub fn fill_with_defaults<T>(obj: Option<T>, def_obj: T) -> T {
    obj.unwrap_or(def_obj)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    struct DummyOptions {
        a: i32,
        b: bool,
        c: String,
    }

    #[test]
    fn test_none_returns_default() {
        let def = DummyOptions {
            a: 1,
            b: true,
            c: "default".to_string(),
        };
        let result = fill_with_defaults(None, def.clone());
        assert_eq!(result, def);
    }

    #[test]
    fn test_some_returns_provided_value_entirely() {
        let def = DummyOptions {
            a: 1,
            b: true,
            c: "default".to_string(),
        };
        let provided = DummyOptions {
            a: 42,
            b: false,
            c: "custom".to_string(),
        };
        let result = fill_with_defaults(Some(provided.clone()), def);
        assert_eq!(result, provided);
    }

    #[test]
    fn test_partial_override_via_struct_update_syntax() {
        // Mirrors the TS `{ ...defObj, ...obj }` semantics: only `a` is overridden,
        // `b` and `c` fall back to the defaults via Rust's own `..` struct-update syntax
        // at the call site (see module doc comment).
        let def = DummyOptions {
            a: 1,
            b: true,
            c: "default".to_string(),
        };
        let partial = DummyOptions {
            a: 99,
            ..def.clone()
        };
        let result = fill_with_defaults(Some(partial), def.clone());
        assert_eq!(
            result,
            DummyOptions {
                a: 99,
                b: true,
                c: "default".to_string(),
            }
        );
    }

    #[test]
    fn test_works_with_primitive_types() {
        assert_eq!(fill_with_defaults(Some(5), 10), 5);
        assert_eq!(fill_with_defaults(None, 10), 10);
    }

    #[test]
    fn test_works_with_option_string() {
        assert_eq!(
            fill_with_defaults(Some("custom".to_string()), "default".to_string()),
            "custom"
        );
        assert_eq!(
            fill_with_defaults(None, "default".to_string()),
            "default"
        );
    }
}
