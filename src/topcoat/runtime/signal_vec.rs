//! Generic browser-only vector signal writes, matching `Signal<String>::push_str`.
use topcoat::runtime::{
    OptionSurrogate, SignalSurrogate, Surrogate, Surrogated, UsizeSurrogate, VecSurrogate,
};

pub(crate) trait VecPositionExt<T: Surrogated + PartialEq> {
    fn position(&self, value: T::Surrogate) -> OptionSurrogate<usize>;
}

impl<T: Surrogated + PartialEq> VecPositionExt<T> for VecSurrogate<T> {
    fn position(&self, value: T::Surrogate) -> OptionSurrogate<usize> {
        let value = value.into_real();
        self.into_real()
            .iter()
            .position(|candidate| candidate == &value)
            .into_surrogate()
    }
}

pub(crate) trait SignalVecExt<T: Surrogated> {
    fn push(&self, value: T::Surrogate);
    fn remove(&self, index: UsizeSurrogate);
}
impl<T: Surrogated> SignalVecExt<T> for SignalSurrogate<Vec<T>> {
    fn push(&self, _value: T::Surrogate) {
        panic!("expressions in which a signal is written to cannot be run server-side");
    }
    fn remove(&self, _index: UsizeSurrogate) {
        panic!("expressions in which a signal is written to cannot be run server-side");
    }
}

#[cfg(test)]
mod tests {
    use super::SignalVecExt;
    use topcoat::runtime::{I64Surrogate, SignalSurrogate, UsizeSurrogate, expr};

    #[test]
    fn vector_position_matches_rust_and_returns_typed_optional_indices() {
        use super::VecPositionExt;
        use topcoat::runtime::{Surrogate, Surrogated};
        for (items, needle, expected) in [
            (vec!["first", "😀", "first"], "first", Some(0_usize)),
            (vec!["first", "😀", "first"], "😀", Some(1)),
            (vec!["first"], "missing", None),
            (vec![], "first", None),
        ] {
            let items = items.into_iter().map(str::to_owned).collect::<Vec<_>>();
            let needle = needle.to_owned();
            let (actual, js) =
                expr!(items.clone().position(needle.clone())).into_evaluated_and_js();
            assert_eq!(actual, expected);
            assert!(js.to_source().contains(".position("));
            assert_eq!(
                items
                    .into_surrogate()
                    .position(needle.into_surrogate())
                    .into_real(),
                expected
            );
        }
    }

    #[test]
    fn vector_signal_writes_typecheck_and_emit_typed_operations() {
        let expression = expr!(|values: SignalSurrogate<Vec<i64>>,
                                value: I64Surrogate,
                                index: UsizeSurrogate| {
            values.push(value);
            values.remove(index);
        });
        let (_closure, js) = expression.into_evaluated_and_js();
        let js = js.to_source();
        assert_eq!(js.matches(".push(").count(), 1, "{js}");
        assert_eq!(js.matches(".remove(").count(), 1, "{js}");
        assert!(!js.contains("fetch("), "{js}");
    }
}
