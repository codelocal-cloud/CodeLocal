"""PAT-only Plane MCP gateway for self-hosted Plane Community Edition.

The official image's default HTTP entry point also initializes OAuth, which
requires a Plane OAuth client unavailable in Community Edition. Mount only its
first-party header authentication server; never create an unauthenticated MCP.
"""

import os
from contextlib import asynccontextmanager

import uvicorn
from plane_mcp.server import get_header_mcp
from starlette.applications import Starlette
from starlette.routing import Mount

plane_mcp = get_header_mcp()
mcp_app = plane_mcp.http_app(stateless_http=True)


@asynccontextmanager
async def lifespan(_app):
    async with mcp_app.lifespan(mcp_app):
        yield


app = Starlette(routes=[Mount("/http/api-key", app=mcp_app)], lifespan=lifespan)

if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=int(os.environ.get("PORT", "8211")), access_log=False)
