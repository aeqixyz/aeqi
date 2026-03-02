interface ArticleSchemaProps {
  title: string;
  description: string;
  url: string;
  publishedDate: string;
  updatedDate: string;
  breadcrumbs: { name: string; url: string }[];
}

export function ArticleSchema({
  title,
  description,
  url,
  publishedDate,
  updatedDate,
  breadcrumbs,
}: ArticleSchemaProps) {
  const articleSchema = {
    "@context": "https://schema.org",
    "@type": "Article",
    headline: title,
    description,
    url,
    datePublished: publishedDate,
    dateModified: updatedDate,
    author: {
      "@type": "Organization",
      name: "entity.legal",
      url: "https://entity.legal",
    },
    publisher: {
      "@type": "Organization",
      name: "entity.legal",
      url: "https://entity.legal",
      logo: {
        "@type": "ImageObject",
        url: "https://entity.legal/og-image.png",
      },
    },
    mainEntityOfPage: {
      "@type": "WebPage",
      "@id": url,
    },
    isPartOf: {
      "@type": "WebSite",
      name: "entity.legal",
      url: "https://entity.legal",
    },
  };

  const breadcrumbSchema = {
    "@context": "https://schema.org",
    "@type": "BreadcrumbList",
    itemListElement: breadcrumbs.map((crumb, i) => ({
      "@type": "ListItem",
      position: i + 1,
      name: crumb.name,
      item: crumb.url,
    })),
  };

  return (
    <>
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(articleSchema) }}
      />
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(breadcrumbSchema) }}
      />
    </>
  );
}
