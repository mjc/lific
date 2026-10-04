//! Generic browser-only vector signal writes, matching Signal<String>::push_str.
use topcoat::runtime::{SignalSurrogate, Surrogated, UsizeSurrogate};

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
