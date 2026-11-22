package mcpgateway

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestMCPRequestMethodHandlesUnknownContentLength(t *testing.T) {
	req := httptest.NewRequest(http.MethodPost, "/mcp", strings.NewReader(`{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}`))
	req.ContentLength = -1
	if got := MCPRequestMethod(req); got != "tools/list" {
		t.Fatalf("method=%q want tools/list", got)
	}
	if req.ContentLength <= 0 {
		t.Fatalf("request body should be restored with concrete length, got %d", req.ContentLength)
	}
}
