import type { Metadata } from "next";
import { validateBlogRegistry } from "@/lib/blog-validation";
import { getTranslations } from "@/lib/i18n/server";
import { BlogFooter, BlogHeader } from "../blog/_components";
import styles from "../blog/blog.module.css";

validateBlogRegistry();

export async function generateMetadata(): Promise<Metadata> {
  const t = await getTranslations();
  return {
    title: {
      default: t("Journal"),
      template: `%s · ${t("CodeLocal Blog")}`,
    },
    description: t("Practical notes on local-first AI infrastructure, agents and project intelligence."),
    openGraph: {
      title: t("CodeLocal Blog"),
      description: t("Practical notes on local-first AI infrastructure, agents and project intelligence."),
      type: "website",
      images: [{ url: "/opengraph-image", width: 1200, height: 630 }],
    },
    twitter: {
      card: "summary_large_image",
      title: t("CodeLocal Blog"),
      description: t("Practical notes on local-first AI infrastructure, agents and project intelligence."),
      images: ["/twitter-image"],
    },
  };
}

export default function BlogsLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <div className={styles.blogShell}>
      <BlogHeader />
      {children}
      <BlogFooter />
    </div>
  );
}
