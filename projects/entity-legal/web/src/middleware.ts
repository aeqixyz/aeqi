import { NextRequest, NextResponse } from "next/server";

export function middleware(request: NextRequest) {
  const host = request.headers.get("host") || "";

  // api.entity.legal root → return API docs JSON
  if (host.startsWith("api.") && request.nextUrl.pathname === "/") {
    return NextResponse.json(
      {
        name: "entity.legal API",
        version: "v1",
        status: "coming soon",
        base_url: "https://api.entity.legal",
        docs: "https://entity.legal/docs",
        endpoints: [
          {
            method: "POST",
            path: "/v1/incorporate",
            description: "Form a new legal entity",
            example: "curl -X POST https://api.entity.legal/v1/incorporate",
          },
          {
            method: "GET",
            path: "/v1/incorporate",
            description: "Endpoint documentation",
          },
          {
            method: "GET",
            path: "/v1/entities/:id",
            description: "Retrieve entity details",
          },
          {
            method: "GET",
            path: "/v1/entities/:id/shares",
            description: "Cap table and shareholder registry",
          },
        ],
        website: "https://entity.legal",
        contact: "hello@entity.legal",
      },
      { status: 200 }
    );
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/"],
};
