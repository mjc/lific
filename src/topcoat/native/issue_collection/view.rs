use super::{data::Collection, model::Selection};
use topcoat::{
    context::Cx,
    view::{Attributes, BoxView},
};

pub(super) fn region<'a>(
    _cx: &'a Cx,
    _collection: &Collection,
    _selection: &Selection,
    _clear_filters: Attributes,
) -> BoxView<'a> {
    unimplemented!("native issue collection body is not implemented")
}
