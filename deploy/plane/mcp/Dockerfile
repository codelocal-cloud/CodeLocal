# The official Plane MCP image includes FastMCP, plane-sdk and its tool catalog.
# Community Edition does not expose Plane OAuth; serve only the PAT-authenticated transport.
FROM makeplane/plane-mcp-server:stable
COPY header_server.py /app/header_server.py
EXPOSE 8211
ENTRYPOINT ["python", "/app/header_server.py"]
