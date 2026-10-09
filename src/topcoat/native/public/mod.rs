//! Anonymous readers of explicitly published projects.

mod collection;
mod comments;
mod data;
mod layout;
mod pages;
mod view;

pub(crate) fn screen<'a>(
    cx: &'a topcoat::context::Cx,
    route: &super::public_route::Route,
) -> topcoat::Result<topcoat::view::BoxView<'a>> {
    if let super::public_route::Route::Redirect(destination) = route {
        return Err(topcoat::router::error::redirect_permanent(destination).into());
    }
    let snapshot = super::session::read(cx, data::load(cx, route))?;
    let content = view::content(cx, &snapshot);
    Ok(layout::shell(cx, &snapshot.project, route, content))
}

pub(super) fn markdown_view<'a>(
    cx: &'a topcoat::context::Cx,
    project: &str,
    source: &str,
) -> topcoat::view::BoxView<'a> {
    super::markdown::images::published_view(cx, project, source)
}

#[cfg(test)]
mod production;

#[cfg(test)]
mod collection_production;

#[cfg(test)]
mod collection_subtabs_production;

#[cfg(test)]
mod detail_production;

#[cfg(test)]
mod detail_state_production;

#[cfg(test)]
pub(super) mod paging_production;
