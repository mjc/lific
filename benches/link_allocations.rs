use std::{fmt::Write as _, hint::black_box};

use gungraun::{Dhat, prelude::*};

#[allow(dead_code, unused_imports)]
#[path = "../src/links.rs"]
mod links;

use links::IssueLinkContext;

fn context() -> IssueLinkContext {
    IssueLinkContext::parse("https://tracker.example/lific").unwrap()
}

fn writer() -> (IssueLinkContext, String) {
    (context(), String::with_capacity(128))
}

#[library_benchmark]
#[bench::write_issue(setup = writer)]
fn write_issue_markdown((context, mut output): (IssueLinkContext, String)) -> String {
    write!(output, "{}", context.issue_markdown(black_box("LIF-42"))).unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::issue(setup = writer)]
fn issue_markdown((context, mut output): (IssueLinkContext, String)) -> String {
    write!(output, "{}", context.issue_markdown(black_box("LIF-42"))).unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::issue(setup = writer)]
fn issue_url((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context.issue_url(black_box("LIF-42")).unwrap()
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::project(setup = writer)]
fn project_markdown((context, mut output): (IssueLinkContext, String)) -> String {
    write!(output, "{}", context.project_markdown(black_box("LIF"))).unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::project(setup = writer)]
fn project_url((context, mut output): (IssueLinkContext, String)) -> String {
    write!(output, "{}", context.project_url(black_box("LIF")).unwrap()).unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::page(setup = writer)]
fn page_markdown((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context.page_markdown(black_box("LIF-DOC-3"), black_box(17))
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::page(setup = writer)]
fn page_url((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context
            .page_url(black_box("LIF-DOC-3"), black_box(17))
            .unwrap()
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::plan(setup = writer)]
fn plan_markdown((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context.plan_markdown(black_box("LIF-PLAN-4"), black_box(19))
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::plan(setup = writer)]
fn plan_url((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context
            .plan_url(black_box("LIF-PLAN-4"), black_box(19))
            .unwrap()
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::module(setup = writer)]
fn module_markdown((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context.module_markdown(
            black_box("LIF"),
            black_box(23),
            black_box("Backend [internal]"),
        )
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::module(setup = writer)]
fn module_url((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context.module_url(black_box("LIF"), black_box(23)).unwrap()
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::issue_comment(setup = writer)]
fn issue_comment_markdown((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context.issue_comment_markdown(black_box("LIF-42"), black_box(7))
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::issue_comment(setup = writer)]
fn issue_comment_url((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context
            .issue_comment_url(black_box("LIF-42"), black_box(7))
            .unwrap()
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::page_comment(setup = writer)]
fn page_comment_markdown((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context.page_comment_markdown(black_box("LIF-DOC-3"), black_box(17), black_box(8),)
    )
    .unwrap();
    black_box(output)
}

#[library_benchmark]
#[bench::page_comment(setup = writer)]
fn page_comment_url((context, mut output): (IssueLinkContext, String)) -> String {
    write!(
        output,
        "{}",
        context
            .page_comment_url(black_box("LIF-DOC-3"), black_box(17), black_box(8))
            .unwrap()
    )
    .unwrap();
    black_box(output)
}

library_benchmark_group!(
    name = link_allocations,
    benchmarks = [
        write_issue_markdown,
        issue_markdown,
        issue_url,
        project_markdown,
        project_url,
        page_markdown,
        page_url,
        plan_markdown,
        plan_url,
        module_markdown,
        module_url,
        issue_comment_markdown,
        issue_comment_url,
        page_comment_markdown,
        page_comment_url
    ]
);

main!(
    config = LibraryBenchmarkConfig::default().tool(Dhat::default()),
    library_benchmark_groups = link_allocations
);
