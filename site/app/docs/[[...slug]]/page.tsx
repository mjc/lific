import { source } from "@/lib/source";
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
} from "fumadocs-ui/layouts/docs/page";
import { notFound } from "next/navigation";
import { createRelativeLink } from "fumadocs-ui/mdx";
import { getMDXComponents } from "@/mdx-components";
import type { Metadata } from "next";

// BreadcrumbList structured data: Docs root, any intermediate folder
// that has an index page, then the current page.
function breadcrumbJsonLd(slug: string[] | undefined, title: string): string {
  const items: { name: string; url: string }[] = [
    { name: "Docs", url: "https://lific.dev/docs" },
  ];
  const parts = slug ?? [];
  for (let i = 1; i < parts.length; i++) {
    const parent = source.getPage(parts.slice(0, i));
    if (parent) {
      items.push({
        name: parent.data.title,
        url: `https://lific.dev${parent.url}`,
      });
    }
  }
  if (parts.length > 0) {
    items.push({
      name: title,
      url: `https://lific.dev${["/docs", ...parts].join("/")}`,
    });
  }
  return JSON.stringify({
    "@context": "https://schema.org",
    "@type": "BreadcrumbList",
    itemListElement: items.map((item, i) => ({
      "@type": "ListItem",
      position: i + 1,
      name: item.name,
      item: item.url,
    })),
  });
}

export default async function Page(props: {
  params: Promise<{ slug?: string[] }>;
}) {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  const MDX = page.data.body;

  return (
    <DocsPage toc={page.data.toc} full={page.data.full}>
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{
          __html: breadcrumbJsonLd(params.slug, page.data.title),
        }}
      />
      <DocsTitle>{page.data.title}</DocsTitle>
      <DocsDescription>{page.data.description}</DocsDescription>
      <DocsBody>
        <MDX
          components={getMDXComponents({
            // Allows linking to other docs pages with relative file paths.
            a: createRelativeLink(source, page),
          })}
        />
      </DocsBody>
    </DocsPage>
  );
}

export async function generateStaticParams() {
  return source.generateParams();
}

export async function generateMetadata(props: {
  params: Promise<{ slug?: string[] }>;
}): Promise<Metadata> {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  const title = `${page.data.title} · Lific Docs`;
  const description = page.data.description;

  return {
    title,
    description,
    alternates: { canonical: page.url },
    openGraph: {
      title,
      description,
      url: page.url,
      siteName: "Lific",
      type: "article",
      images: [{ url: "/og.png", width: 1920, height: 1080 }],
    },
    twitter: {
      card: "summary_large_image",
      title,
      description,
      images: ["/og.png"],
    },
  };
}
