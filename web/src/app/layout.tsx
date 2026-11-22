import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import "./globals.css";
import { getLocale } from "@/lib/i18n/server";
import { LocaleProvider } from "@/lib/i18n/provider";
import { PublicBuildNotice } from "./public-build-notice";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  metadataBase: new URL(process.env.NEXT_PUBLIC_SITE_URL || "https://codelocal.cloud"),
  title: {
    default: "CodeLocal",
    template: "%s · CodeLocal",
  },
  description:
    "Connect AI coding clients to an authorized local runtime and durable Project Brain without moving raw source execution into the cloud.",
  openGraph: {
    type: "website",
    url: "/",
    siteName: "CodeLocal",
    title: "CodeLocal",
    description:
      "Connect AI coding clients to an authorized local runtime and durable Project Brain without moving raw source execution into the cloud.",
    images: [{ url: "/opengraph-image", width: 1200, height: 630, alt: "CodeLocal — AI coding with local execution and durable project intelligence" }],
  },
  twitter: {
    card: "summary_large_image",
    title: "CodeLocal",
    description:
      "Connect AI coding clients to an authorized local runtime and durable Project Brain without moving raw source execution into the cloud.",
    images: ["/twitter-image"],
  },
  manifest: "/manifest.webmanifest",
  applicationName: "CodeLocal",
  appleWebApp: { capable: true, title: "CodeLocal", statusBarStyle: "black-translucent" },
  icons: {
    icon: [
      { url: "/favicon.ico", sizes: "16x16 32x32 48x48 64x64" },
      { url: "/icon.png", sizes: "512x512", type: "image/png" },
    ],
    shortcut: "/favicon.ico",
    apple: [{ url: "/apple-icon.png", sizes: "180x180", type: "image/png" }],
  },
};

export const viewport = { themeColor: "#07101a", width: "device-width", initialScale: 1, viewportFit: "cover" as const };

export default async function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  const locale = await getLocale();
  return (
    <html lang={locale} data-scroll-behavior="smooth" className={`${geistSans.variable} ${geistMono.variable}`}>
      <body>
        <LocaleProvider locale={locale}>
          {children}
          <PublicBuildNotice />
        </LocaleProvider>
      </body>
    </html>
  );
}
