import { NextResponse } from "next/server";

export function GET() {
  return NextResponse.json({ service: "codelocal-web", state: "bootstrap" });
}
