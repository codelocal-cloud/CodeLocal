import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "CodeLocal Beta — Your AI development infrastructure",
  description: "A new Rust-powered foundation for CodeLocal.",
};

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
