//! Typed keepalive adapter over the registry procedure vocabulary.
use std::marker::PhantomData;
use topcoat::runtime::{ProcedureSurrogate, Surrogated, TypedProcedure};

pub(crate) trait ProcedureKeepaliveExt<P: TypedProcedure> {
    fn with_keepalive(&self) -> KeepaliveProcedure<'_, P>;
}

impl<P: TypedProcedure> ProcedureKeepaliveExt<P> for ProcedureSurrogate<P> {
    fn with_keepalive(&self) -> KeepaliveProcedure<'_, P> {
        KeepaliveProcedure(PhantomData)
    }
}

pub(crate) struct KeepaliveProcedure<'a, P: TypedProcedure>(PhantomData<&'a ProcedureSurrogate<P>>);

impl<P: TypedProcedure> KeepaliveProcedure<'_, P> {
    /// Client runtime only; uses the registered procedure's typed arguments/output.
    #[allow(clippy::unused_async)]
    pub(crate) async fn call(
        &self,
        _args: <P::Args as Surrogated>::Surrogate,
    ) -> <P::Output as Surrogated>::Surrogate {
        panic!("procedures cannot be executed on the server");
    }
}

#[cfg(test)]
mod tests {
    use super::ProcedureKeepaliveExt;
    use topcoat::{
        context::Cx,
        runtime::{expr, procedure},
    };

    #[procedure("/__runtime_keepalive/empty")]
    async fn empty(cx: &Cx) -> topcoat::Result<bool> {
        let _ = cx;
        Ok(true)
    }

    #[procedure("/__runtime_keepalive/unit")]
    async fn unit(cx: &Cx, _value: ()) -> topcoat::Result<bool> {
        let _ = cx;
        Ok(true)
    }

    #[procedure("/__runtime_keepalive/multiple")]
    async fn multiple(cx: &Cx, flag: bool, text: String) -> topcoat::Result<String> {
        let _ = cx;
        Ok(if flag { text } else { String::new() })
    }

    #[test]
    fn registry_procedures_typecheck_and_emit_generic_keepalive_calls() {
        let unit_value = ();
        let expression = expr!(async || {
            let accepted = empty.with_keepalive()().await;
            let _unit_accepted = unit.with_keepalive()(unit_value).await;
            let text = multiple.with_keepalive()(accepted, "draft".to_owned()).await;
            text
        });
        let (_closure, js) = expression.into_evaluated_and_js();
        let js = js.to_source();
        assert_eq!(js.matches(".with_keepalive().call(").count(), 3, "{js}");
        assert!(js.contains("draft"), "{js}");
        assert!(!js.contains("fetch("), "{js}");
    }
}
