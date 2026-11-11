package cloudmcp

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"strings"
	"sync/atomic"
	"testing"

	plugindomain "github.com/0xmarkhydra/codelocal/internal/plugins"
	"github.com/modelcontextprotocol/go-sdk/mcp"
)

func TestEndpointAndAddressPolicy(t *testing.T) {
	for _, raw := range []string{"http://example.com/mcp", "https://localhost/mcp", "https://localhost./mcp", "https://x.localhost/mcp", "https://u:p@example.com/mcp", "https://example.com/mcp?token=x", "https://example.com/mcp#x", "https://[fe80::1%25en0]/"} {
		if _, err := ValidateEndpoint(raw); err == nil {
			t.Fatalf("accepted %s", raw)
		}
	}
	for _, ip := range []string{"127.0.0.1", "10.0.0.1", "172.16.0.1", "192.168.0.1", "169.254.169.254", "0.0.0.0", "100.64.1.1", "224.0.0.1", "240.0.0.1", "192.0.2.1", "198.18.0.1", "::1", "fc00::1", "fe80::1", "ff02::1", "::ffff:127.0.0.1", "64:ff9b::a00:1", "2002:7f00:1::1"} {
		if publicAddress(netip.MustParseAddr(ip)) {
			t.Fatalf("accepted %s", ip)
		}
	}
	for _, ip := range []string{"1.1.1.1", "8.8.8.8", "2606:4700::1111"} {
		if !publicAddress(netip.MustParseAddr(ip)) {
			t.Fatalf("rejected %s", ip)
		}
	}
}

func TestDialPinsAddressAndRejectsMixedDNS(t *testing.T) {
	calls := 0
	dialed := ""
	ips := []netip.Addr{netip.MustParseAddr("1.1.1.1")}
	d := publicDialer{lookup: func(context.Context, string) ([]netip.Addr, error) { calls++; return ips, nil }, dial: func(_ context.Context, _ string, addr string) (net.Conn, error) {
		dialed = addr
		return nil, errors.New("fixture")
	}}
	_, _ = d.DialContext(context.Background(), "tcp", "example.com:443")
	if calls != 1 || dialed != "1.1.1.1:443" {
		t.Fatalf("not pinned %d %s", calls, dialed)
	}
	ips = append(ips, netip.MustParseAddr("127.0.0.1"))
	dialed = ""
	_, err := d.DialContext(context.Background(), "tcp", "example.com:443")
	if err == nil || dialed != "" {
		t.Fatal("mixed public/private DNS dialed")
	}
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }

func TestHTTPRejectsRedirectCrossOriginAndLargeResponse(t *testing.T) {
	client, closeClient := newHTTPClient("https://example.com/mcp", "secret", "")
	defer closeClient()
	if client.CheckRedirect(nil, nil) == nil {
		t.Fatal("redirect allowed")
	}
	reached := false
	transport := &guardedTransport{origin: "https://example.com", bearer: "secret", base: roundTripFunc(func(r *http.Request) (*http.Response, error) {
		reached = true
		if r.Header.Get("Authorization") != "Bearer secret" || r.Header.Get("Cookie") != "" {
			t.Fatal("wrong headers")
		}
		return &http.Response{StatusCode: 200, Body: io.NopCloser(strings.NewReader(strings.Repeat("x", maxResponseBytes+10))), ContentLength: -1}, nil
	})}
	req, _ := http.NewRequest("GET", "https://other.example/mcp", nil)
	if _, err := transport.RoundTrip(req); err == nil || reached {
		t.Fatal("cross origin accepted")
	}
	req, _ = http.NewRequest("GET", "https://example.com/mcp", nil)
	req.Header.Set("Cookie", "session=x")
	res, err := transport.RoundTrip(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if _, err = io.ReadAll(res.Body); err == nil {
		t.Fatal("response limit not enforced")
	}
}

func TestPlanePATWorkspaceHeaderOnlyAtTrustedEndpoints(t *testing.T) {
	calls := 0
	transport := &guardedTransport{
		origin:        "https://mcp.plane.so",
		bearer:        "secret",
		workspaceSlug: "team-a",
		base: roundTripFunc(func(r *http.Request) (*http.Response, error) {
			calls++
			if r.Header.Get("Authorization") != "Bearer secret" || r.Header.Get("x-workspace-slug") != "team-a" {
				t.Fatal("Plane MCP headers were not set from authenticated configuration")
			}
			return &http.Response{StatusCode: http.StatusOK, Body: io.NopCloser(strings.NewReader("{}"))}, nil
		}),
	}
	req, _ := http.NewRequest(http.MethodPost, "https://mcp.plane.so/http/api-key/mcp", nil)
	req.Header.Set("x-workspace-slug", "untrusted")
	res, err := transport.RoundTrip(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
	if calls != 1 {
		t.Fatalf("expected exactly 1 request, got %d", calls)
	}

	req, _ = http.NewRequest(http.MethodPost, "https://mcp.plane.so/http/mcp", nil)
	if _, err := transport.RoundTrip(req); err == nil {
		t.Fatal("workspace header leaked to another Plane endpoint")
	}
	req, _ = http.NewRequest(http.MethodPost, "https://example.com/mcp", nil)
	if _, err := transport.RoundTrip(req); err == nil {
		t.Fatal("cross-origin Plane header leaked")
	}
	if calls != 1 {
		t.Fatalf("unexpected extra request: %d", calls)
	}

	// The CodeLocal-hosted gateway receives the same per-account headers, but
	// never on an alternate path or through a cross-origin redirect.
	gateway := &guardedTransport{
		origin:        "https://planemcp-plane.up.railway.app",
		bearer:        "secret",
		workspaceSlug: "team-a",
		base: roundTripFunc(func(r *http.Request) (*http.Response, error) {
			if r.Header.Get("Authorization") != "Bearer secret" || r.Header.Get("x-workspace-slug") != "team-a" {
				t.Fatal("CodeLocal gateway missing authenticated headers")
			}
			return &http.Response{StatusCode: 200, Body: io.NopCloser(strings.NewReader("{}"))}, nil
		}),
	}
	req, _ = http.NewRequest(http.MethodPost, plugindomain.PlaneCodeLocalMCPPATURL, nil)
	res, err = gateway.RoundTrip(req)
	if err != nil {
		t.Fatalf("CodeLocal Plane gateway rejected: %v", err)
	}
	res.Body.Close()
	req, _ = http.NewRequest(http.MethodPost, "https://planemcp-plane.up.railway.app/http/mcp", nil)
	if _, err := gateway.RoundTrip(req); err == nil {
		t.Fatal("CodeLocal Plane gateway sent token to a different path")
	}

	other := &guardedTransport{
		origin: "https://example.com",
		base: roundTripFunc(func(r *http.Request) (*http.Response, error) {
			if r.Header.Get("x-workspace-slug") != "" {
				t.Fatal("caller-supplied Plane header must be removed for other MCPs")
			}
			return &http.Response{StatusCode: http.StatusOK, Body: io.NopCloser(strings.NewReader("{}"))}, nil
		}),
	}
	req, _ = http.NewRequest(http.MethodGet, "https://example.com/mcp", nil)
	req.Header.Set("x-workspace-slug", "spoofed")
	res, err = other.RoundTrip(req)
	if err != nil {
		t.Fatal(err)
	}
	res.Body.Close()
}

func TestPlaneWorkspaceHeaderRequiresTrustedEndpoint(t *testing.T) {
	manager := NewManager()
	_, err := manager.Discover(context.Background(), "user", Config{Endpoint: "https://example.com/mcp", WorkspaceSlug: "team-a"})
	if err == nil || !strings.Contains(err.Error(), "trusted Plane PAT") {
		t.Fatalf("unexpected result: %v", err)
	}
	if manager.total != 0 {
		t.Fatal("failed Plane validation retained MCP concurrency slot")
	}
}

func fixtureManager(t *testing.T) (*Manager, *atomic.Int32) {
	t.Helper()
	calls := &atomic.Int32{}
	server := mcp.NewServer(&mcp.Implementation{Name: "fixture", Version: "1"}, nil)
	schema := map[string]any{"type": "object", "properties": map[string]any{"value": map[string]any{"type": "string"}}, "required": []string{"value"}, "additionalProperties": false}
	for i := 0; i < 3; i++ {
		server.AddTool(&mcp.Tool{Name: fmt.Sprintf("write_%d", i), InputSchema: schema, Annotations: &mcp.ToolAnnotations{ReadOnlyHint: true}}, func(context.Context, *mcp.CallToolRequest) (*mcp.CallToolResult, error) {
			calls.Add(1)
			return &mcp.CallToolResult{Content: []mcp.Content{&mcp.TextContent{Text: "written"}}}, nil
		})
	}
	handler := mcp.NewStreamableHTTPHandler(func(*http.Request) *mcp.Server { return server }, &mcp.StreamableHTTPOptions{Stateless: true, JSONResponse: true})
	httpServer := httptest.NewTLSServer(handler)
	t.Cleanup(httpServer.Close)
	manager := NewManager()
	// Only the network destination is swapped for a TLS fixture; URL/origin policy stays enabled.
	manager.client = func(endpoint, bearer, workspaceSlug string) (*http.Client, func()) {
		client := &http.Client{}
		base := httpServer.Client().Transport
		client.Transport = &guardedTransport{origin: "https://example.com", bearer: bearer, base: roundTripFunc(func(r *http.Request) (*http.Response, error) {
			copy := r.Clone(r.Context())
			u := *r.URL
			copy.URL = &u
			copy.URL.Host = strings.TrimPrefix(httpServer.URL, "https://")
			copy.Host = copy.URL.Host
			return base.RoundTrip(copy)
		})}
		return client, func() {}
	}
	return manager, calls
}

func TestDiscoverySchemaAndExplicitApprovedWrite(t *testing.T) {
	manager, calls := fixtureManager(t)
	cfg := Config{Endpoint: "https://example.com/mcp"}
	tools, err := manager.Discover(context.Background(), "A", cfg)
	if err != nil {
		t.Fatal(err)
	}
	if len(tools) != 3 || !json.Valid(tools[0].InputSchema) {
		t.Fatalf("lost tools/schema: %#v", tools)
	}
	if calls.Load() != 0 {
		t.Fatal("discovery executed a tool")
	}
	if _, err := manager.CallApproved(context.Background(), "A", cfg, "write_0", map[string]any{"value": 7}); err == nil || calls.Load() != 0 {
		t.Fatal("invalid schema dispatched")
	}
	if _, err := manager.CallApproved(context.Background(), "A", cfg, "write_0", map[string]any{"value": "ok"}); err != nil || calls.Load() != 1 {
		t.Fatalf("approved write %v %d", err, calls.Load())
	}
	if manager.total != 0 || len(manager.active) != 0 {
		t.Fatal("session/account state retained")
	}
	if _, err := manager.Discover(context.Background(), "", cfg); err == nil {
		t.Fatal("missing identity accepted")
	}
}

func TestConcurrencyLimitAndRemoteSchemaRef(t *testing.T) {
	m := NewManager()
	release := []func(){}
	for i := 0; i < 4; i++ {
		r, err := m.acquire("A")
		if err != nil {
			t.Fatal(err)
		}
		release = append(release, r)
	}
	if _, err := m.acquire("A"); err == nil {
		t.Fatal("account limit bypassed")
	}
	r, err := m.acquire("B")
	if err != nil {
		t.Fatal("other account blocked")
	}
	r()
	for _, r := range release {
		r()
	}
	if err := validateArguments(json.RawMessage(`{"$ref":"https://127.0.0.1/schema"}`), map[string]any{}); err == nil {
		t.Fatal("remote schema allowed")
	}
}
