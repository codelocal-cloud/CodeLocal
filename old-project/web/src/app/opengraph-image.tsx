import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { ImageResponse } from "next/og";
import { SocialCard, socialImageAlt, socialImageSize } from "./social-card";

export const alt = socialImageAlt;
export const size = socialImageSize;
export const contentType = "image/png";
export const runtime = "nodejs";

export default async function OpenGraphImage() {
  const brandIcon = await readFile(join(process.cwd(), "public", "codelocal-icon-512.png"));
  const brandIconSrc = `data:image/png;base64,${brandIcon.toString("base64")}`;
  return new ImageResponse(<SocialCard brandIconSrc={brandIconSrc} />, size);
}
