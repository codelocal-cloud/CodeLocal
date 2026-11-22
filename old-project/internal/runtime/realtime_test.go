package runtime

import (
	"errors"
	"fmt"
	"net"
	"os"
	"syscall"
	"testing"
)

func TestRuntimeControlURL(t *testing.T) {
	cases := map[string]string{
		"https://codelocal.cloud":      "wss://codelocal.cloud/api/client/runtime/ws",
		"http://localhost:3333":        "ws://localhost:3333/api/client/runtime/ws",
		"https://example.com/old/path": "wss://example.com/api/client/runtime/ws",
	}
	for input, expected := range cases {
		if actual := runtimeControlURL(input); actual != expected {
			t.Fatalf("runtimeControlURL(%q) = %q, want %q", input, actual, expected)
		}
	}
}

func TestIsGatewayNotReadyUsesTypedConnectionRefused(t *testing.T) {
	refused := &net.OpError{Op: "dial", Net: "tcp", Err: os.NewSyscallError("connect", syscall.ECONNREFUSED)}
	if !isGatewayNotReady(refused) {
		t.Fatal("typed ECONNREFUSED must be treated as gateway-not-ready")
	}
	if !isGatewayNotReady(fmt.Errorf("wrapped dial failure: %w", refused)) {
		t.Fatal("wrapped ECONNREFUSED must be detected through errors.Is")
	}
	if isGatewayNotReady(errors.New("connection refused")) {
		t.Fatal("plain error text must not trigger gateway-not-ready retry policy")
	}
	unreachable := &net.OpError{Op: "dial", Net: "tcp", Err: os.NewSyscallError("connect", syscall.EHOSTUNREACH)}
	if isGatewayNotReady(unreachable) {
		t.Fatal("non-ECONNREFUSED network failures must keep exponential backoff")
	}
}
